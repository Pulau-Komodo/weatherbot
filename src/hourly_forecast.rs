use ab_glyph::{FontRef, PxScale};
use chrono::{DateTime, FixedOffset, Timelike};
use graph::{
	common_types::{GradientPoint, MultiPointGradient, Range},
	drawing::{MarkIntervals, Padding, Spacing},
	generic_graph::{AxisGridLabels, Chart, GradientBars, HorizontalLines, Line, Rgb, SolidBars},
	text_box::{TextBox, TextSegment},
	util::{composite, make_png, next_multiple, previous_and_next_multiple},
};
use itertools::Itertools;
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

const LABEL_SIZE: PxScale = PxScale { x: 18.0, y: 18.0 };
const AXIS_LABEL_SIZE: PxScale = PxScale { x: 14.0, y: 14.0 };

pub async fn handle_hourly(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	font: &FontRef<'static>,
	header_font: &FontRef<'static>,
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

	let padding = Padding {
		above: 3,
		below: 19,
		left: 21,
		right: 3,
	};

	let temps: Vec<_> = result
		.hourly
		.temperature_2m
		.into_iter()
		.zip(result.hourly.apparent_temperature)
		.zip(result.hourly.relative_humidity_2m)
		.map(|((temp, apparent), humidity)| {
			[temp, apparent, wet_bulb_temp(temp, humidity as f32)].map(convert_num)
		})
		.collect();

	let temp_range = temps
		.iter()
		.flatten()
		.copied()
		.minmax()
		.into_option()
		.unwrap_or((0, 0));
	let chart_temp_range = previous_and_next_multiple(Range::new(temp_range.0, temp_range.1), 4);

	let spacing = Spacing {
		horizontal: 8,
		vertical: 3,
	};
	let label = TextBox::new(
		&[
			TextSegment::new("Dry bulb", Rgb([255, 0, 0])),
			TextSegment::white(", "),
			TextSegment::new("wet bulb", Rgb([0, 148, 255])),
			TextSegment::white(" and "),
			TextSegment::new("apparent", Rgb([0, 255, 33])),
			TextSegment::white(" temperatures (°C)"),
		],
		header_font.clone(),
		LABEL_SIZE,
		(temps.len() - 1) as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		temps.len(),
		chart_temp_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(2, 4),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: chart_temp_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(Line {
		colour: Rgb([0, 255, 33]),
		data: temps.iter().map(|[_, apparent, _]| apparent).copied(),
		max: chart_temp_range.end(),
	});
	chart.draw(Line {
		colour: Rgb([0, 148, 255]),
		data: temps.iter().map(|[_, _, wet_bulb]| wet_bulb).copied(),
		max: chart_temp_range.end(),
	});
	chart.draw(Line {
		colour: Rgb([255, 0, 0]),
		data: temps.iter().map(|[temp, _, _]| temp).copied(),
		max: chart_temp_range.end(),
	});

	let temp_image = chart.into_canvas();

	let spacing = Spacing {
		horizontal: 8,
		vertical: 1,
	};
	let humidity_range = Range::new(0, 100 * 100);

	let label = TextBox::new(
		&[TextSegment::new("Relative humidity", Rgb([0, 148, 255]))],
		header_font.clone(),
		LABEL_SIZE,
		(result.hourly.relative_humidity_2m.len() - 1) as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.hourly.relative_humidity_2m.len(),
		humidity_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(10, 20),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: humidity_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(Line {
		colour: Rgb([0, 148, 255]),
		data: result.hourly.relative_humidity_2m.iter().map(|x| x * 100),
		max: humidity_range.end(),
	});

	let humidity_image = chart.into_canvas();

	let max_uv = result
		.hourly
		.uv_index
		.iter()
		.chain(&result.hourly.uv_index_clear_sky)
		.fold(0.0f32, |acc, num| acc.max(*num));
	let uv_range = Range::new(0, next_multiple(convert_num(max_uv), 1));

	let spacing = Spacing {
		horizontal: 8,
		vertical: 10,
	};

	let label = TextBox::new(
		&[
			TextSegment::new("UV index", Rgb([0, 255, 33])),
			TextSegment::white(" (and "),
			TextSegment::new("clear sky UVI", Rgb([118, 215, 234])),
			TextSegment::white(")"),
		],
		header_font.clone(),
		LABEL_SIZE,
		result.hourly.uv_index.len() as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.hourly.uv_index.len() + 1,
		uv_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(1, 1),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: uv_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(HorizontalLines {
		colour: Rgb([118, 215, 234]),
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

	let spacing = Spacing {
		horizontal: 8,
		vertical: 1,
	};
	let probability_range = Range::new(0, 100 * 100);

	let label = TextBox::new(
		&[
			TextSegment::white("Probability of "),
			TextSegment::new("precipitation", Rgb([0, 180, 255])),
		],
		header_font.clone(),
		LABEL_SIZE,
		result.hourly.precipitation_probability.len() as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.hourly.precipitation_probability.len() + 1,
		probability_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(10, 20),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: probability_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
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

	let label = TextBox::new(
		&[
			TextSegment::white("Amount of "),
			TextSegment::new("precipitation", Rgb([0, 148, 255])),
			TextSegment::white(" (mm)"),
		],
		header_font.clone(),
		LABEL_SIZE,
		result.hourly.precipitation.len() as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.hourly.precipitation.len() + 1,
		precipitation_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(1, 1),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: precipitation_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(SolidBars {
		colour: Rgb([0, 148, 255]),
		data: result.hourly.precipitation.into_iter().map(convert_num),
	});

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

	let label = TextBox::new(
		&[
			TextSegment::new("Wind", Rgb([0, 255, 33])),
			TextSegment::white(" and "),
			TextSegment::new("gust", Rgb([70, 119, 67])),
			TextSegment::white(" speed (m/s)"),
		],
		header_font.clone(),
		LABEL_SIZE,
		result.hourly.wind_speed_10m.len() as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.hourly.wind_speed_10m.len() + 1,
		data_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(5, 5),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: data_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
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

/// Calculates wet bulb temperature in °C given dry bulb temperature in °C and relative humidity * 100 (0-100).
///
/// Supposedly this is only accurate for temperatures between -20 °C and 50 °C, and relative humidities between .05 and .99 (5 and 99).
fn wet_bulb_temp(temp: f32, humidity: f32) -> f32 {
	temp * (0.15197 * (humidity + 8.313659).sqrt()).atan() + (temp + humidity).atan()
		- (humidity - 1.676331).atan()
		+ 0.00391838 * humidity.powf(1.5) * (0.023101 * humidity).atan()
		- 4.686035
}
