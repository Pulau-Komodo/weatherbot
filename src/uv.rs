use std::fmt::Write;

use ab_glyph::FontRef;
use chrono::{FixedOffset, Timelike};
use graph::modules::make_png;
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateAttachment, CreateCommand,
	CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use sqlx::{Pool, Sqlite};

use crate::{error::Error, geocoding::GeocodingResult, user_locations::Location};

#[derive(Debug, Deserialize)]
struct HourlyUvi {
	time: Vec<u32>,
	uv_index: Vec<f32>,
	uv_index_clear_sky: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct UviResult {
	latitude: f32,
	longitude: f32,
	utc_offset_seconds: i32,
	hourly: HourlyUvi,
}

impl UviResult {
	async fn get(latitude: f32, longitude: f32, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[("hourly", "uv_index")])
			.query(&[("hourly", "uv_index_clear_sky")])
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			.query(&[("forecast_hours", 24)])
			.query(&[("latitude", latitude), ("longitude", longitude)])
			.send()
			.await?
			.json::<UviResult>()
			.await?)
	}
}

pub async fn handle_uvi(
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

	let result = UviResult::get(location.latitude(), location.longitude(), &client).await?;

	let image = graph::modules::hourly_uvi::create(
		font,
		(0..result.hourly.uv_index.len())
			.map(|i| graph::modules::hourly_uvi::HourlyUvi {
				hour: chrono::DateTime::from_timestamp(result.hourly.time[i] as i64, 0)
					.unwrap()
					.with_timezone(&FixedOffset::east_opt(result.utc_offset_seconds).unwrap())
					.hour() as u8,
				uvi: (result.hourly.uv_index[i] * 100.0) as u16,
			})
			.collect(),
	);
	let image = make_png(image);

	let content: String =
		result
			.hourly
			.uv_index
			.into_iter()
			.fold(String::new(), |mut string, n| {
				write!(string, "{n} ").unwrap();
				string
			});
	interaction
		.create_response(
			context,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.content(content)
					.add_file(CreateAttachment::bytes(image, "uvi.png")),
			),
		)
		.await?;
	Ok(())
}

pub fn create_uvi() -> CreateCommand {
	CreateCommand::new("uvi")
		.description("Hourly UVI forecast")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the UVI forecast of.",
			)
			.required(false),
		)
}
