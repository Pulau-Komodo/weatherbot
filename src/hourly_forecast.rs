use ab_glyph::FontRef;
use chrono::{DateTime, FixedOffset, Timelike};
use graph::{
	common_types::{GradientPoint, MultiPointGradient, Range},
	drawing::{MarkIntervals, Padding, Spacing},
	generic_graph::{AxisGridLabels, Chart, GradientBars, HorizontalLines, Rgb, SolidBars},
	util::{composite, make_png, next_multiple},
};
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateAttachment, CreateCommand,
	CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use sqlx::{Pool, Sqlite};

use crate::{
	error::Error,
	location::{Coordinates, Location},
	util::convert_num,
};

#[derive(Debug, Deserialize)]
struct HourlyWeather {
	time: Vec<i64>,
	uv_index: Vec<f32>,
	uv_index_clear_sky: Vec<f32>,
	temperature_2m: Vec<f32>,
	apparent_temperature: Vec<f32>,
	relative_humidity_2m: Vec<i32>,
	precipitation_probability: Vec<u8>,
	precipitation: Vec<f32>,
	wind_speed_10m: Vec<f32>,
	wind_gusts_10m: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct HourlyResult {
	#[serde(rename = "latitude")]
	_latitude: f32,
	#[serde(rename = "longitude")]
	_longitude: f32,
	utc_offset_seconds: i32,
	hourly: HourlyWeather,
}

impl HourlyResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[("hourly", "uv_index")])
			.query(&[("hourly", "uv_index_clear_sky")])
			.query(&[("hourly", "temperature_2m")])
			.query(&[("hourly", "relative_humidity_2m")])
			.query(&[("hourly", "apparent_temperature")])
			.query(&[("hourly", "precipitation_probability")])
			.query(&[("hourly", "precipitation")])
			.query(&[("hourly", "wind_speed_10m")])
			.query(&[("hourly", "wind_gusts_10m")])
			.query(&[("wind_speed_unit", "ms")])
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			.query(&[("forecast_hours", 48)])
			.query(&[
				("latitude", coordinates.latitude),
				("longitude", coordinates.longitude),
			])
			.send()
			.await?
			.json::<HourlyResult>()
			.await?)
	}
}

/// Get the hour of the day (from 0 to 23) for a given Unix timestamp, and a timezone offset in seconds.
fn hour_from_timestamp(timestamp: i64, offset_seconds: i32) -> u8 {
	DateTime::from_timestamp(timestamp, 0)
		.unwrap()
		.with_timezone(&FixedOffset::east_opt(offset_seconds).unwrap())
		.hour() as u8
}

pub async fn handle_hourly(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	font: &FontRef<'static>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = Location::get_from_argument_or_for_user(interaction, &client, database).await?;

	let result = HourlyResult::get(location.coordinates(), &client).await?;
	let times = result
		.hourly
		.time
		.into_iter()
		.map(|time| hour_from_timestamp(time, result.utc_offset_seconds))
		.collect::<Vec<_>>();

	let max_uv = result
		.hourly
		.uv_index
		.iter()
		.chain(&result.hourly.uv_index_clear_sky)
		.fold(0.0f32, |acc, num| acc.max(*num));
	let uv_range = Range::new(0, next_multiple(convert_num(max_uv), 1));

	let padding = Padding {
		above: 7,
		below: 19,
		left: 21,
		right: 3,
	};
	let spacing = Spacing {
		horizontal: 8,
		vertical: 10,
	};
	let mut chart = Chart::new(
		result.hourly.uv_index.len() + 1,
		uv_range.len() as u32,
		spacing,
		padding,
	);

	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(1, 1),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: uv_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: ab_glyph::PxScale { x: 14.0, y: 14.0 },
	});
	chart.draw(HorizontalLines {
		colour: Rgb([255, 255, 255]),
		data: result
			.hourly
			.uv_index_clear_sky
			.into_iter()
			.map(convert_num),
	});
	chart.draw(GradientBars {
		gradient: MultiPointGradient::new(vec![
			GradientPoint::from_rgb(padding.below, [0, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 9 / 2, [255, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 9, [255, 0, 33]),
		]),
		data: result.hourly.uv_index.into_iter().map(convert_num),
	});

	let uvi_image = chart.into_canvas();

	let padding = Padding {
		above: 7,
		below: 19,
		left: 21,
		right: 3,
	};
	let spacing = Spacing {
		horizontal: 8,
		vertical: 1,
	};

	let probability_range = Range::new(0, 100 * 100);
	let mut chart = Chart::new(
		result.hourly.precipitation_probability.len() + 1,
		probability_range.len() as u32,
		spacing,
		padding,
	);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(10, 20),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: probability_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: ab_glyph::PxScale { x: 14.0, y: 14.0 },
	});
	chart.draw(SolidBars {
		colour: Rgb([0, 180, 255]),
		data: result
			.hourly
			.precipitation_probability
			.into_iter()
			.map(|n| n as i32 * 100),
	});

	let pop_image = chart.into_canvas();

	let padding = Padding {
		above: 7,
		below: 19,
		left: 21,
		right: 3,
	};
	let spacing = Spacing {
		horizontal: 8,
		vertical: 16,
	};
	let max_precipitation = result
		.hourly
		.precipitation
		.iter()
		.fold(0.0f32, |acc, num| acc.max(*num));

	let precipitation_range = Range::new(0, next_multiple(convert_num(max_precipitation), 1));

	let mut chart = Chart::new(
		result.hourly.precipitation.len() + 1,
		precipitation_range.len() as u32,
		spacing,
		padding,
	);

	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(1, 1),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: precipitation_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: ab_glyph::PxScale { x: 14.0, y: 14.0 },
	});
	chart.draw(SolidBars {
		colour: Rgb([0, 148, 255]),
		data: result.hourly.precipitation.into_iter().map(convert_num),
	});

	let padding: Padding = Padding {
		above: 7,
		below: 19,
		left: 21,
		right: 3,
	};
	let spacing: Spacing = Spacing {
		horizontal: 8,
		vertical: 5,
	};

	let max_chart_speed = next_multiple(
		result
			.hourly
			.wind_speed_10m
			.iter()
			.zip(&result.hourly.wind_gusts_10m)
			.flat_map(|(a, b)| [a, b])
			.copied()
			.map(convert_num)
			.max()
			.unwrap_or(0) as i32,
		5,
	);

	let data_range = Range::new(0, max_chart_speed);

	let precipitation_image = chart.into_canvas();

	let temp_image = graph::modules::hourly_temp::create(
		font,
		(0..result.hourly.temperature_2m.len())
			.map(|i| {
				graph::modules::hourly_temp::HourlyTemps::new(
					times[i],
					result.hourly.temperature_2m[i],
					result.hourly.apparent_temperature[i],
					result.hourly.relative_humidity_2m[i],
				)
			})
			.collect(),
	);

	let mut chart = Chart::new(
		result.hourly.wind_speed_10m.len() + 1,
		data_range.len() as u32,
		spacing,
		padding,
	);

	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(5, 5),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: data_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: ab_glyph::PxScale { x: 14.0, y: 14.0 },
	});
	chart.draw(GradientBars {
		gradient: MultiPointGradient::new(vec![
			GradientPoint::from_rgb(padding.below, [70, 119, 67]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 7, [118, 118, 62]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 14, [122, 67, 62]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 21, [103, 78, 122]),
		]),
		data: result.hourly.wind_gusts_10m.into_iter().map(convert_num),
	});
	chart.draw(GradientBars {
		gradient: MultiPointGradient::new(vec![
			GradientPoint::from_rgb(padding.below, [0, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 7, [255, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 14, [255, 0, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 21, [188, 66, 255]),
		]),
		data: result.hourly.wind_speed_10m.into_iter().map(convert_num),
	});

	let wind_image = chart.into_canvas();

	let composite = composite(&[
		temp_image,
		pop_image,
		precipitation_image,
		wind_image,
		uvi_image,
	]);
	let image = make_png(composite);

	interaction
		.create_response(
			context,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.add_file(CreateAttachment::bytes(image, "hourly.png")),
			),
		)
		.await?;
	Ok(())
}

pub fn create_hourly() -> CreateCommand {
	CreateCommand::new("hourly")
		.description("Hourly weather forecast")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the weather forecast of.",
			)
			.required(false),
		)
}
