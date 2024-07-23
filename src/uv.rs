use std::{error::Error, fmt::Write};

use ab_glyph::FontRef;
use chrono::{FixedOffset, TimeZone, Timelike};
use graph::modules::make_png;
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateAttachment, CreateCommand,
	CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
};

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
	async fn get(latitude: f32, longitude: f32, client: &Client) -> Result<Self, Box<dyn Error>> {
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
	context: Context,
	interaction: CommandInteraction,
	font: &FontRef<'static>,
) -> Result<(), Box<dyn Error>> {
	let Some(input) = interaction
		.data
		.options
		.first()
		.and_then(|option| option.value.as_str())
	else {
		return Err("No argument")?;
	};
	let (latitude, longitude) = input.split_once(' ').ok_or("Huh")?;
	let latitude = latitude.parse()?;
	let longitude = longitude.parse()?;
	let client = Client::new();
	let result = UviResult::get(latitude, longitude, &client).await?;
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
			.required(true),
		)
}
