use ab_glyph::FontRef;
use chrono::{DateTime, Datelike, FixedOffset};
use graph::{
	common_types::Range,
	drawing::MarkIntervals,
	generic_graph::{AxisGridLabels, Line, Rgb},
	util::previous_and_next_multiple,
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
	geocoding::GeocodingResult,
	location::{Coordinates, Location},
};

#[derive(Debug, Deserialize)]
struct DailyWeather {
	time: Vec<u32>,
	temperature_2m_min: Vec<f32>,
	temperature_2m_max: Vec<f32>,
	apparent_temperature_min: Vec<f32>,
	apparent_temperature_max: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct DailyResult {
	latitude: f32,
	longitude: f32,
	utc_offset_seconds: i32,
	daily: DailyWeather,
}

impl DailyResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[("daily", "temperature_2m_min")])
			.query(&[("daily", "temperature_2m_max")])
			.query(&[("daily", "apparent_temperature_min")])
			.query(&[("daily", "apparent_temperature_max")])
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			//	.query(&[("forecast_days", 7)])
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

fn day_from_timestamp(timestamp: i64, offset_seconds: i32) -> u8 {
	DateTime::from_timestamp(timestamp, 0)
		.unwrap()
		.with_timezone(&FixedOffset::east_opt(offset_seconds).unwrap())
		.day() as u8
}

pub async fn handle_daily(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	font: &FontRef<'static>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = match interaction
		.data
		.options
		.first()
		.and_then(|option| option.value.as_str())
	{
		Some(arg) => Location::from_geocoding_result(GeocodingResult::get(arg, &client).await?),
		None => Location::get_for_user(
			database,
			interaction.user.id,
			interaction
				.guild_id
				.ok_or_else(|| Error::custom("Somehow could not get guild ID"))?,
		)
		.await?
		.ok_or_else(|| Error::friendly("No location set, and no location provided"))?,
	};

	let result = DailyResult::get(location.coordinates(), &client).await?;

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
	let temp_range = Range::new((min * 100.0).round() as i32, (max * 100.0).round() as i32);
	let chart_temp_range =
		previous_and_next_multiple(Range::new(temp_range.start(), temp_range.end()), 4);

	let padding = graph::drawing::Padding {
		above: 7,
		below: 19,
		left: 21,
		right: 9,
	};
	let spacing = graph::drawing::Spacing {
		horizontal: 25,
		vertical: 3,
	};
	let mut chart = graph::generic_graph::Chart::new(
		result.daily.temperature_2m_max.len(),
		chart_temp_range.len() as u32,
		spacing,
		padding,
	);

	let convert_num = |n: f32| (n * 100.0).round() as i32;

	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(2, 4),
		horizontal_intervals: MarkIntervals::new(1, 1),
		vertical_label_range: chart_temp_range,
		horizontal_labels: result
			.daily
			.time
			.into_iter()
			.map(|time| day_from_timestamp(time as i64, result.utc_offset_seconds)),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: ab_glyph::PxScale { x: 14.0, y: 14.0 },
	});
	chart.draw(Line {
		colour: Rgb([0, 255, 33]),
		data: result
			.daily
			.apparent_temperature_min
			.into_iter()
			.map(convert_num),
		max: chart_temp_range.end(),
	});
	chart.draw(Line {
		colour: Rgb([0, 255, 33]),
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

	let temp_image = graph::util::make_png(chart.into_canvas());

	interaction
		.create_response(
			context,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.add_file(CreateAttachment::bytes(temp_image, "temperature.png")),
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
