use reqwest::Client;
use serde::Deserialize;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
	CreateInteractionResponse, CreateInteractionResponseMessage,
};

use crate::error::Error;

#[derive(Debug, Deserialize)]
pub struct GeocodingResultMinimal {
	pub name: String,
	pub country: Option<String>,
	pub latitude: f32,
	pub longitude: f32,
}

impl GeocodingResultMinimal {
	pub async fn get(place_name: &str, client: &Client) -> Result<Self, Error> {
		let mut results: GeocodingResultsMinimal = client
			.get("https://geocoding-api.open-meteo.com/v1/search")
			.query(&[("count", 1)])
			.query(&[("format", "json"), ("name", place_name)])
			.send()
			.await?
			.json()
			.await?;
		results
			.results
			.pop()
			.ok_or(Error::friendly("No geocoding results"))
	}
}

/// https://open-meteo.com/en/docs/geocoding-api
#[derive(Debug, Deserialize)]
pub struct GeocodingResult {
	pub id: u32,
	pub name: String,
	pub latitude: f32,
	pub longitude: f32,
	pub elevation: Option<f32>,
	pub feature_code: String,
	pub country_code: Option<String>,
	pub country: Option<String>,
	pub population: Option<u32>,
}

impl GeocodingResult {
	pub async fn get(place_name: &str, client: &Client) -> Result<Self, Error> {
		let mut results: GeocodingResults = client
			.get("https://geocoding-api.open-meteo.com/v1/search")
			.query(&[("count", "1"), ("format", "json"), ("name", place_name)])
			.send()
			.await?
			.json()
			.await?;
		results
			.results
			.pop()
			.ok_or_else(|| Error::friendly("No geocoding results"))
	}
}

#[derive(Debug, Deserialize)]
struct GeocodingResults {
	#[serde(default)]
	results: Vec<GeocodingResult>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResultsMinimal {
	#[serde(default)]
	results: Vec<GeocodingResultMinimal>,
}

pub async fn handle_find_coordinates(
	context: &Context,
	interaction: &CommandInteraction,
) -> Result<(), Error> {
	let Some(place) = interaction
		.data
		.options
		.first()
		.and_then(|option| option.value.as_str())
	else {
		return Err(Error::friendly("No argument"));
	};
	let client = Client::new();
	let result = GeocodingResult::get(place, &client).await?;
	let content = format!(
		"Name: {}, population: {}, latitude: {}, longitude: {}, feature code: {}, country: {}",
		result.name,
		result
			.population
			.map_or_else(|| String::from("unknown"), |n| format!("{n}")),
		result.latitude,
		result.longitude,
		result.feature_code,
		result
			.country
			.unwrap_or_else(|| String::from("unspecified")),
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

pub fn create_find_coordinates() -> CreateCommand {
	CreateCommand::new("find_coordinates")
		.description("Finds the coordinates of the specified place.")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to find the coordinates of.",
			)
			.required(true),
		)
}
