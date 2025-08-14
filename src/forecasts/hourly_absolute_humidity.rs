use std::f32;

use ab_glyph::{FontRef, PxScale};
use graph::{
	common_types::Range,
	drawing::{MarkIntervals, Padding, Spacing},
	generic_graph::{AxisGridLabels, Chart, Line, Rgb},
	text_box::{TextBox, TextSegment},
	util::{make_png, next_multiple},
};
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateAttachment, CreateCommand,
	CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseFollowup,
	CreateInteractionResponseMessage,
};
use sqlx::{Pool, Sqlite};

use crate::{
	error::Error,
	location::{Coordinates, Location},
	util::{CommandInteractionExt as _, convert_num},
};

use super::hourly::hour_from_timestamp;

#[derive(Debug, Deserialize)]
struct HourlyAbsoluteHumidity {
	time: Vec<i64>,
	temperature_2m: Vec<f32>,
	relative_humidity_2m: Vec<i32>,
}

#[derive(Debug, Deserialize)]
struct HourlyAbsoluteHumidityResult {
	#[serde(rename = "latitude")]
	_latitude: f32,
	#[serde(rename = "longitude")]
	_longitude: f32,
	utc_offset_seconds: i32,
	hourly: HourlyAbsoluteHumidity,
}

impl HourlyAbsoluteHumidityResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[("hourly", "temperature_2m")])
			.query(&[("hourly", "relative_humidity_2m")])
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			.query(&[("forecast_hours", 48)])
			.query(&[
				("latitude", coordinates.latitude),
				("longitude", coordinates.longitude),
			])
			.send()
			.await?
			.json::<HourlyAbsoluteHumidityResult>()
			.await?)
	}
}

const LABEL_SIZE: PxScale = PxScale { x: 18.0, y: 18.0 };
const AXIS_LABEL_SIZE: PxScale = PxScale { x: 14.0, y: 14.0 };

pub async fn handle_hourly_absolute_humidity(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	font: &FontRef<'static>,
	header_font: &FontRef<'static>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = Location::get_from_argument_or_for_user(interaction, &client, database).await?;

	let result = interaction
		.defer_and(
			HourlyAbsoluteHumidityResult::get(location.coordinates(), &client),
			context,
		)
		.await?;
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

	let abs_humidity: Vec<_> = result
		.hourly
		.temperature_2m
		.into_iter()
		.zip(result.hourly.relative_humidity_2m)
		.map(|(temp, hum)| absolute_humidity(hum as f32 / 100.0, temp))
		.map(convert_num)
		.collect();
	let max_humidity = abs_humidity.iter().max().copied().unwrap_or(0);

	let chart_range = Range::new(0, next_multiple(max_humidity, 4));

	let spacing = Spacing {
		horizontal: 8,
		vertical: 3,
	};
	let label = TextBox::new(
		&[TextSegment::new("Absolute humidity", Rgb([0, 148, 255]))],
		header_font.clone(),
		LABEL_SIZE,
		(abs_humidity.len() - 1) as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		abs_humidity.len(),
		chart_range.len() as u32,
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
		vertical_label_range: chart_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});
	chart.draw(Line {
		colour: Rgb([0, 148, 255]),
		data: abs_humidity.into_iter(),
		max: chart_range.end(),
	});

	let image = make_png(chart.into_canvas());

	interaction
		.create_followup(
			context,
			CreateInteractionResponseFollowup::new().add_file(CreateAttachment::bytes(
				image,
				"hourly_absolute_humidity.png",
			)),
		)
		.await?;
	Ok(())
}

/// Takes relative humidity from 0 to 1 (where 0 means perfectly dry and 1 means saturated), and temperature in degrees Celsius, and returns absolute humidity in g/m^3.
fn absolute_humidity(relative_humidity: f32, temperature: f32) -> f32 {
	f32::consts::E.powf(17.67 * temperature / (temperature + 243.5))
		* 6.112
		* relative_humidity
		* 100.0
		* 2.1674
		/ (temperature + 273.15)
}

pub fn create_hourly_absolute_humidity() -> CreateCommand {
	CreateCommand::new("absolute_humidity")
		.description("Hourly absolute humidity forecast")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the weather forecast of.",
			)
			.required(false),
		)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_abs_humidity() {
		for (relative_humidity, temperature) in [(0.5, 30.0), (1.0, 30.0), (0.5, 10.0), (0.8, 10.0)]
		{
			println!(
				"{} and {} degC: {} g/m^3",
				relative_humidity,
				temperature,
				absolute_humidity(relative_humidity, temperature)
			);
		}
	}
}
