use reqwest::Client;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
};
use sqlx::{query, Pool, Sqlite};

use crate::{
	error::Error, geocoding::GeocodingResult, location::Location, reply_shortcuts::ReplyShortcuts,
};

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
		.ok_or_else(|| Error::custom_unfriendly("Missing argument"))?;
	let client = Client::new();
	let geocoding = GeocodingResult::get(location_arg, &client).await?;
	let location = Location::from_geocoding_result(geocoding);
	location
		.set_for_user(
			database,
			interaction.user.id,
			interaction
				.guild_id
				.ok_or_else(|| Error::custom_unfriendly("Somehow had no guild ID"))?,
		)
		.await?;
	interaction
		.ephemeral_reply(
			&context.http,
			format!(
				"Location set to {} ({}), country: {}, feature code: {}",
				location.name(),
				location.coordinates(),
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

pub fn create_set_coords() -> CreateCommand {
	CreateCommand::new("set_coords")
		.description("Set the coordinates to use by default for weather commands.")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"coordinates",
				"The coordinates to use by default for weather commands",
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
		.ok_or_else(|| Error::custom_unfriendly("Somehow had no guild ID"))?
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
