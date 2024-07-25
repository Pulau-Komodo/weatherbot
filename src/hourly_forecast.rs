use ab_glyph::FontRef;
use chrono::{DateTime, FixedOffset, Timelike};
use graph::util::{composite, make_png};
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
};

#[derive(Debug, Deserialize)]
struct HourlyWeather {
	time: Vec<u32>,
	uv_index: Vec<f32>,
	uv_index_clear_sky: Vec<f32>,
	temperature_2m: Vec<f32>,
	apparent_temperature: Vec<f32>,
	relative_humidity_2m: Vec<i32>,
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
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			.query(&[("forecast_hours", 24)])
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
	let location = match interaction
		.data
		.options
		.first()
		.and_then(|option| option.value.as_str())
	{
		Some(arg) => Location::try_from_arg(arg, &client).await?,
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

	let result = HourlyResult::get(location.coordinates(), &client).await?;

	let uvi_image = graph::modules::hourly_uvi::create(
		font,
		(0..result.hourly.uv_index.len())
			.map(|i| {
				graph::modules::hourly_uvi::HourlyUvi::new(
					hour_from_timestamp(result.hourly.time[i] as i64, result.utc_offset_seconds),
					result.hourly.uv_index[i],
				)
			})
			.collect(),
	);
	let temp_image = graph::modules::hourly_temp::create(
		font,
		(0..result.hourly.temperature_2m.len())
			.map(|i| {
				graph::modules::hourly_temp::HourlyTemps::new(
					hour_from_timestamp(result.hourly.time[i] as i64, result.utc_offset_seconds),
					result.hourly.temperature_2m[i],
					result.hourly.apparent_temperature[i],
					result.hourly.relative_humidity_2m[i],
				)
			})
			.collect(),
	);
	let composite = composite(&[temp_image, uvi_image]);
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
