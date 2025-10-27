use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
};
use sqlx::{Pool, Sqlite};

use crate::{
	error::Error,
	location::{Coordinates, Location},
	reply_shortcuts::ReplyShortcuts,
	util::ResponseExt,
};

#[derive(Debug, Deserialize)]
struct SunriseSunset {
	sunrise: Vec<i64>,
	sunset: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct SunResult {
	#[serde(rename = "latitude")]
	_latitude: f32,
	#[serde(rename = "longitude")]
	_longitude: f32,
	utc_offset_seconds: i32,
	daily: SunriseSunset,
}

impl SunResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[
				("daily", "sunrise"),
				("daily", "sunset"),
				("timeformat", "unixtime"),
				("timezone", "auto"),
			])
			.query(&[("forecast_days", 2)])
			.query(&[
				("latitude", coordinates.latitude),
				("longitude", coordinates.longitude),
			])
			.send()
			.await?
			.json_or_raw::<SunResult>()
			.await
	}
	fn next_sunrise_and_sunset(self) -> (i64, i64) {
		let now = Utc::now().timestamp();
		let sunrise = self
			.daily
			.sunrise
			.into_iter()
			.find(|time| *time > now)
			.unwrap() + self.utc_offset_seconds as i64;
		let sunset = self
			.daily
			.sunset
			.into_iter()
			.find(|time| *time > now)
			.unwrap() + self.utc_offset_seconds as i64;
		(sunrise, sunset)
	}
}

pub async fn handle_sun(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = Location::get_from_argument_or_for_user(interaction, &client, database).await?;

	let (sunrise, sunset) = SunResult::get(location.coordinates(), &client)
		.await?
		.next_sunrise_and_sunset();
	let sunrise_date = timestamp_to_date(sunrise)?;
	let sunset_date = timestamp_to_date(sunset)?;
	let message = match sunrise.cmp(&sunset) {
		Ordering::Less => format!(
			"🌅{} 🌃{}",
			sunrise_date.format("%H:%M"),
			sunset_date.format("%H:%M")
		),
		Ordering::Greater => format!(
			" 🌃{} 🌅{}",
			sunset_date.format("%H:%M"),
			sunrise_date.format("%H:%M")
		),
		Ordering::Equal => String::from("Eternal day or night?"),
	};
	interaction.public_reply(&context.http, message).await?;
	Ok(())
}

fn timestamp_to_date(timestamp: i64) -> Result<DateTime<Utc>, Error> {
	DateTime::from_timestamp(timestamp, 0)
		.ok_or(Error::custom_unfriendly("Failed to parse timestamp"))
}

pub fn create_sun() -> CreateCommand {
	CreateCommand::new("sun")
		.description("Next sunrise and sunset")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the next sunrise and sunset of.",
			)
			.required(false),
		)
}
