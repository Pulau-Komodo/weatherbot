use reqwest::Client;
use serenity::all::{
	CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
	CreateInteractionResponse, CreateInteractionResponseMessage,
};

use crate::{error::Error, location::Location};

pub async fn handle_distance(
	context: &Context,
	interaction: &CommandInteraction,
) -> Result<(), Error> {
	let mut args = interaction
		.data
		.options
		.iter()
		.filter_map(|option| option.value.as_str());

	let place_a = args
		.next()
		.ok_or_else(|| Error::custom_unfriendly("Missing first location argument"))?;
	let place_b = args
		.next()
		.ok_or_else(|| Error::custom_unfriendly("Missing second location argument"))?;

	let client = Client::new();
	let (place_a, place_b) = tokio::try_join!(
		Location::try_from_arg(place_a, &client),
		Location::try_from_arg(place_b, &client),
	)?;
	let distance = place_a.coordinates().distance_to(place_b.coordinates());
	if distance.is_zero() {
		return Err(Error::friendly("Those places are the same."));
	}

	let content = format!("The distance between {place_a} and {place_b} is {distance}.");
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

pub fn create_distance() -> CreateCommand {
	CreateCommand::new("distance")
		.description("Get the distance between two places.")
		.add_option(
			CreateCommandOption::new(CommandOptionType::String, "a", "The first place")
				.required(true),
		)
		.add_option(
			CreateCommandOption::new(CommandOptionType::String, "b", "The second place")
				.required(true),
		)
}
