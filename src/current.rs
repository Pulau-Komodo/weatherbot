use std::sync::LazyLock;

use ab_glyph::FontRef;
use chrono::Duration;
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
	CreateInteractionResponse, CreateInteractionResponseMessage,
};
use sqlx::{Pool, Sqlite};

use crate::{
	error::Error,
	location::{Coordinates, Location},
	util::weather_code_to_str,
};

#[derive(Debug, Deserialize)]
struct CurrentWeather {
	time: i64,
	/// Pretty sure this is the interval of the weather reports it interpolated between to get current weather.
	interval: Option<u32>,
	temperature_2m: f32,
	relative_humidity_2m: f32,
	apparent_temperature: f32,
	precipitation: f32,
	rain: f32,
	showers: f32,
	snowfall: f32,
	weather_code: u8,
	cloud_cover: f32,
	wind_speed_10m: f32,
	wind_direction_10m: f32,
	wind_gusts_10m: f32,
	uv_index: f32,
	uv_index_clear_sky: f32,
}

#[derive(Debug, Deserialize)]
struct CurrentResult {
	#[serde(rename = "latitude")]
	_latitude: f32,
	#[serde(rename = "longitude")]
	_longitude: f32,
	utc_offset_seconds: i32,
	current: CurrentWeather,
}

impl CurrentResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[("current", "temperature_2m")])
			.query(&[("current", "relative_humidity_2m")])
			.query(&[("current", "apparent_temperature")])
			.query(&[("current", "precipitation")])
			.query(&[("current", "rain")])
			.query(&[("current", "showers")])
			.query(&[("current", "snowfall")])
			.query(&[("current", "weather_code")])
			.query(&[("current", "cloud_cover")])
			.query(&[("current", "wind_speed_10m")])
			.query(&[("current", "wind_direction_10m")])
			.query(&[("current", "wind_gusts_10m")])
			.query(&[("current", "uv_index")])
			.query(&[("current", "uv_index_clear_sky")])
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			.query(&[
				("latitude", coordinates.latitude),
				("longitude", coordinates.longitude),
			])
			.send()
			.await?
			.json::<Self>()
			.await?)
	}
}

pub async fn handle_current(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	_font: &FontRef<'static>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = Location::get_from_argument_or_for_user(interaction, &client, database).await?;

	let weather = CurrentResult::get(location.coordinates(), &client).await?;
	let current = weather.current;

	let interval_text = current
		.interval
		.and_then(|interval| {
			static CONFIG: LazyLock<stringify_interval::DisplayConfigConstant> =
				LazyLock::new(|| {
					stringify_interval::DisplayConfigConstant::default().with_seconds()
				});
			static TEXT: LazyLock<stringify_interval::Text> =
				LazyLock::new(stringify_interval::Text::default);
			stringify_interval::without_date(Duration::seconds(interval as i64), &CONFIG, &TEXT)
				.inspect_err(|error| eprintln!("{error}"))
				.ok()
		})
		.unwrap_or(String::from("unknown"));

	let content = format!(
		"Temperature: {}°C, apparent temperature: {}°C, relative humidity: {}%, precipitation: {}mm, rain: {}mm, showers: {}mm, snowfall: {}cm, weather code: {}, cloud cover: {}%, wind speed: {}km/h, wind direction: {}°, wind gusts: {}km/h, UVI: {}, clear-sky UVI: {}, interval: {}",
		current.temperature_2m,
		current.apparent_temperature,
		current.relative_humidity_2m,
		current.precipitation,
		current.rain,
		current.showers,
		current.snowfall,
		weather_code_to_str(current.weather_code).unwrap_or("?"),
		current.cloud_cover,
		current.wind_speed_10m,
		current.wind_direction_10m,
		current.wind_gusts_10m,
		current.uv_index,
		current.uv_index_clear_sky,
		interval_text
	);

	interaction
		.create_response(
			context,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new().content(content),
			),
		)
		.await?;
	Ok(())
}

pub fn create_current() -> CreateCommand {
	CreateCommand::new("current")
		.description("Current weather")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the weather of.",
			)
			.required(false),
		)
}
