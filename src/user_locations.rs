use reqwest::Client;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption, GuildId,
	UserId,
};
use sqlx::{query, Pool, Sqlite};

use crate::{error::Error, geocoding::GeocodingResult, reply_shortcuts::ReplyShortcuts};

pub struct Location {
	name: Option<String>,
	latitude: f32,
	longitude: f32,
	country: Option<String>,
	feature_code: Option<String>,
}

impl Location {
	pub fn from_geocoding_result(geocoding: GeocodingResult) -> Self {
		Self {
			name: Some(geocoding.name),
			latitude: geocoding.latitude,
			longitude: geocoding.longitude,
			country: geocoding.country,
			feature_code: Some(geocoding.feature_code),
		}
	}
	pub fn from_coords(latitude: f32, longitude: f32) -> Self {
		Self {
			name: None,
			latitude,
			longitude,
			country: None,
			feature_code: None,
		}
	}
	pub async fn get_for_user(
		database: &Pool<Sqlite>,
		user: UserId,
		domain: GuildId,
	) -> Result<Option<Self>, Error> {
		let user = user.get() as i64;
		let domain = domain.get() as i64;
		let Some(result) = query!(
			"
			SELECT place_name, latitude, longitude, country, feature_code
			FROM user_locations
			WHERE domain = ? AND user = ?
			",
			domain,
			user
		)
		.fetch_optional(database)
		.await?
		else {
			return Ok(None);
		};
		Ok(Some(Self {
			name: result.place_name,
			latitude: result.latitude as f32,
			longitude: result.longitude as f32,
			country: result.country,
			feature_code: result.feature_code,
		}))
	}
	pub async fn set_for_user(
		&self,
		database: &Pool<Sqlite>,
		user: UserId,
		domain: GuildId,
	) -> Result<(), Error> {
		let user = user.get() as i64;
		let domain = domain.get() as i64;
		query!(
			"
			INSERT INTO user_locations (domain, user, place_name, latitude, longitude, country, feature_code)
			VALUES (?, ?, ?, ?, ?, ?, ?)
		",
			domain,
			user,
			self.name,
			self.latitude,
			self.longitude,
			self.country,
			self.feature_code
		)
		.execute(database)
		.await?;
		Ok(())
	}
	pub fn name(&self) -> &str {
		self.name.as_deref().unwrap_or("unspecified")
	}
	pub fn latitude(&self) -> f32 {
		self.latitude
	}
	pub fn longitude(&self) -> f32 {
		self.longitude
	}
	pub fn country(&self) -> &str {
		self.country.as_deref().unwrap_or("unspecified")
	}
	pub fn feature_code(&self) -> &str {
		self.feature_code.as_deref().unwrap_or("unspecified")
	}
}

pub async fn handle_set_location(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
) -> Result<(), Error> {
	let location_arg = interaction
		.data
		.options
		.first()
		.and_then(|arg| arg.value.as_str())
		.ok_or_else(|| Error::custom("Missing argument"))?;
	let client = Client::new();
	let geocoding = GeocodingResult::get(location_arg, &client).await?;
	let location = Location::from_geocoding_result(geocoding);
	location
		.set_for_user(
			database,
			interaction.user.id,
			interaction
				.guild_id
				.ok_or_else(|| Error::custom("Somehow had no guild ID"))?,
		)
		.await?;
	interaction
		.ephemeral_reply(
			&context.http,
			format!(
				"Location set to {} ({}, {}), country: {}, feature code: {}",
				location.name(),
				location.latitude(),
				location.longitude(),
				location.country(),
				location.feature_code()
			),
		)
		.await?;
	Ok(())
}

pub fn create_set_location() -> CreateCommand {
	CreateCommand::new("set_location")
		.description("Set the location to use by default for weather commands.")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"location",
				"The location to use by default for weather commands",
			)
			.required(true),
		)
}

pub async fn handle_unset_location(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
) -> Result<(), Error> {
	let user = interaction.user.id.get() as i64;
	let domain = interaction
		.guild_id
		.ok_or_else(|| Error::custom("Somehow had no guild ID"))?
		.get() as i64;
	query!(
		"
		DELETE FROM user_locations
		WHERE domain = ? AND user = ?",
		domain,
		user
	)
	.execute(database)
	.await?;
	interaction
		.ephemeral_reply(&context.http, "Successfully unset location.")
		.await?;
	Ok(())
}

pub fn create_unset_location() -> CreateCommand {
	CreateCommand::new("unset_location")
		.description("Unset the location to use by default for weather commands.")
}
