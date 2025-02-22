use ab_glyph::{FontRef, PxScale};
use chrono::{DateTime, Datelike, FixedOffset};
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
struct DailyWeather {
	time: Vec<i64>,
	temperature_2m_min: Vec<f32>,
	temperature_2m_max: Vec<f32>,
	apparent_temperature_min: Vec<f32>,
	apparent_temperature_max: Vec<f32>,
	precipitation_sum: Vec<f32>,
	precipitation_probability_max: Vec<u8>,
	wind_speed_10m_max: Vec<f32>,
	wind_gusts_10m_max: Vec<f32>,
	uv_index_max: Vec<f32>,
	uv_index_clear_sky_max: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct DailyResult {
	#[serde(rename = "latitude")]
	_latitude: f32,
	#[serde(rename = "longitude")]
	_longitude: f32,
	utc_offset_seconds: i32,
	daily: DailyWeather,
}

impl DailyResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[
				("daily", "temperature_2m_min"),
				("daily", "temperature_2m_max"),
				("daily", "apparent_temperature_min"),
				("daily", "apparent_temperature_max"),
				("daily", "precipitation_sum"),
				("daily", "precipitation_probability_max"),
				("daily", "wind_speed_10m_max"),
				("daily", "wind_gusts_10m_max"),
				("daily", "uv_index_max"),
				("daily", "uv_index_clear_sky_max"),
				("wind_speed_unit", "ms"),
				("timeformat", "unixtime"),
				("timezone", "auto"),
			])
			.query(&[("forecast_days", 14)])
			.query(&[
				("latitude", coordinates.latitude),
				("longitude", coordinates.longitude),
			])
			.send()
			.await?
			.json::<DailyResult>()
			.await?)
	}
}

/// Get the day of the month (from 1 to 31) for a given Unix timestamp, and a timezone offset in seconds.
fn day_from_timestamp(timestamp: i64, offset_seconds: i32) -> u8 {
	DateTime::from_timestamp(timestamp, 0)
		.unwrap()
		.with_timezone(&FixedOffset::east_opt(offset_seconds).unwrap())
		.day() as u8
}

const LABEL_SIZE: PxScale = PxScale { x: 14.0, y: 14.0 };
const AXIS_LABEL_SIZE: PxScale = PxScale { x: 14.0, y: 14.0 };

pub async fn handle_daily(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	font: &FontRef<'static>,
	header_font: &FontRef<'static>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = Location::get_from_argument_or_for_user(interaction, &client, database).await?;

	let result = DailyResult::get(location.coordinates(), &client).await?;

	let times = result
		.daily
		.time
		.into_iter()
		.map(|time| day_from_timestamp(time, result.utc_offset_seconds))
		.collect::<Vec<_>>();

	let padding = Padding {
		above: 3,
		below: 19,
		left: 21,
		right: 9,
	};

	let (&min, &max) = result
		.daily
		.apparent_temperature_max
		.iter()
		.chain(&result.daily.apparent_temperature_min)
		.chain(&result.daily.temperature_2m_max)
		.chain(&result.daily.temperature_2m_min)
		.minmax()
		.into_option()
		.unwrap_or((&0.0, &0.0));
	let temp_range = Range::new(convert_num(min), convert_num(max));
	let chart_temp_range =
		previous_and_next_multiple(Range::new(temp_range.start(), temp_range.end()), 4);

	let spacing = Spacing {
		horizontal: 25,
		vertical: 3,
	};
	let label = TextBox::new(
		&[
			TextSegment::new("Minimum", Rgb([0, 148, 255])),
			TextSegment::white(", "),
			TextSegment::new("maximum", Rgb([255, 0, 0])),
			TextSegment::white(" and "),
			TextSegment::new("apparent minimum and maximum", Rgb([0, 170, 33])),
			TextSegment::white(" temperatures (°C)"),
		],
		header_font.clone(),
		LABEL_SIZE,
		(result.daily.temperature_2m_max.len() as u32 - 1) * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.daily.temperature_2m_max.len(),
		chart_temp_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			left: padding.left + spacing.horizontal / 2,
			right: padding.right + spacing.horizontal / 2,
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(2, 4),
		horizontal_intervals: MarkIntervals::new(1, 1),
		vertical_label_range: chart_temp_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(Line {
		colour: Rgb([0, 170, 33]),
		data: result
			.daily
			.apparent_temperature_min
			.into_iter()
			.map(convert_num),
		max: chart_temp_range.end(),
	});
	chart.draw(Line {
		colour: Rgb([0, 170, 33]),
		data: result
			.daily
			.apparent_temperature_max
			.into_iter()
			.map(convert_num),
		max: chart_temp_range.end(),
	});
	chart.draw(Line {
		colour: Rgb([0, 148, 255]),
		data: result.daily.temperature_2m_min.into_iter().map(convert_num),
		max: chart_temp_range.end(),
	});
	chart.draw(Line {
		colour: Rgb([255, 0, 0]),
		data: result.daily.temperature_2m_max.into_iter().map(convert_num),
		max: chart_temp_range.end(),
	});
	let temp_image = chart.into_canvas();

	let max_precipitation = result
		.daily
		.precipitation_sum
		.iter()
		.fold(0.0f32, |acc, num| acc.max(*num));
	let precipitation_range = Range::new(0, next_multiple(convert_num(max_precipitation), 5));

	let spacing = Spacing {
		horizontal: 25,
		vertical: 1,
	};
	let label = TextBox::new(
		&[
			TextSegment::white("Total "),
			TextSegment::new("precipitation", Rgb([0, 148, 255])),
			TextSegment::white(" (mm)"),
		],
		header_font.clone(),
		LABEL_SIZE,
		result.daily.precipitation_sum.len() as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.daily.precipitation_sum.len() + 1,
		precipitation_range.end() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(25, 25),
		horizontal_intervals: MarkIntervals::new(1, 1),
		vertical_label_range: precipitation_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(SolidBars {
		colour: Rgb([0, 148, 255]),
		data: result
			.daily
			.precipitation_sum
			.iter()
			.copied()
			.map(convert_num),
	});
	let precipitation_image = chart.into_canvas();

	let max_wind = result
		.daily
		.wind_gusts_10m_max
		.iter()
		.chain(&result.daily.wind_speed_10m_max)
		.fold(0.0f32, |acc, num| acc.max(*num));
	let wind_range = Range::new(0, next_multiple(convert_num(max_wind), 5));

	let spacing = Spacing {
		horizontal: 25,
		vertical: 5,
	};
	let label = TextBox::new(
		&[
			TextSegment::white("Maximum "),
			TextSegment::new("wind", Rgb([0, 255, 33])),
			TextSegment::white(" and "),
			TextSegment::new("gust", Rgb([70, 119, 67])),
			TextSegment::white(" speeds (m/s)"),
		],
		header_font.clone(),
		LABEL_SIZE,
		result.daily.wind_gusts_10m_max.len() as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.daily.wind_gusts_10m_max.len() + 1,
		wind_range.end() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(5, 5),
		horizontal_intervals: MarkIntervals::new(1, 1),
		vertical_label_range: wind_range,
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
		data: result
			.daily
			.wind_gusts_10m_max
			.iter()
			.copied()
			.map(convert_num),
	});
	chart.draw(GradientBars {
		gradient: MultiPointGradient::new(vec![
			GradientPoint::from_rgb(padding.below, [0, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 7, [255, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 14, [255, 0, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 21, [188, 66, 255]),
		]),
		data: result
			.daily
			.wind_speed_10m_max
			.iter()
			.copied()
			.map(convert_num),
	});
	let wind_image = chart.into_canvas();

	let max_uv = result
		.daily
		.uv_index_max
		.iter()
		.chain(&result.daily.uv_index_clear_sky_max)
		.fold(0.0f32, |acc, num| acc.max(*num));
	let uv_range = Range::new(0, next_multiple(convert_num(max_uv), 1));

	let spacing = Spacing {
		horizontal: 25,
		vertical: 10,
	};

	let label = TextBox::new(
		&[
			TextSegment::new("UV index", Rgb([0, 255, 33])),
			TextSegment::white(" (and "),
			TextSegment::new("clear sky UV", Rgb([118, 215, 234])),
			TextSegment::white(")"),
		],
		header_font.clone(),
		LABEL_SIZE,
		(result.daily.uv_index_max.len() as u32 - 1) * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		result.daily.uv_index_max.len() + 1,
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
		horizontal_intervals: MarkIntervals::new(1, 1),
		vertical_label_range: uv_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: true,
		font: font.clone(),
		font_scale: ab_glyph::PxScale { x: 14.0, y: 14.0 },
	});
	chart.draw(HorizontalLines {
		colour: Rgb([118, 215, 234]),
		data: result
			.daily
			.uv_index_clear_sky_max
			.into_iter()
			.map(convert_num),
	});
	chart.draw(GradientBars {
		gradient: MultiPointGradient::new(vec![
			GradientPoint::from_rgb(padding.below, [0, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 9 / 2, [255, 255, 33]),
			GradientPoint::from_rgb(padding.below + spacing.vertical * 9, [255, 0, 33]),
		]),
		data: result.daily.uv_index_max.into_iter().map(convert_num),
	});
	let uvi_image = chart.into_canvas();
	let composite = composite(&[temp_image, precipitation_image, wind_image, uvi_image]);
	let image = make_png(composite);

	interaction
		.create_response(
			context,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.add_file(CreateAttachment::bytes(image, "daily.png")),
			),
		)
		.await?;
	Ok(())
}

pub fn create_daily() -> CreateCommand {
	CreateCommand::new("daily")
		.description("Daily weather forecast")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the weather forecast of.",
			)
			.required(false),
		)
}
