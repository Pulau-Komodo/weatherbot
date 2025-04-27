use itertools::Itertools;
use serenity::{
	all::{Context, EventHandler, Interaction, Ready},
	async_trait,
};
use sqlx::{Pool, Sqlite};

use crate::{
	current,
	error::Error,
	forecasts::{daily, hourly, hourly_absolute_humidity, hourly_soil},
	geocoding,
	reply_shortcuts::ReplyShortcuts,
	sunrise_sunset, user_locations,
};

pub struct DiscordEventHandler {
	database: Pool<Sqlite>,
	font: ab_glyph::FontRef<'static>,
	header_font: ab_glyph::FontRef<'static>,
}

impl DiscordEventHandler {
	pub fn new(
		database: Pool<Sqlite>,
		font: ab_glyph::FontRef<'static>,
		header_font: ab_glyph::FontRef<'static>,
	) -> Self {
		Self {
			database,
			font,
			header_font,
		}
	}
}

#[async_trait]
impl EventHandler for DiscordEventHandler {
	async fn interaction_create(&self, context: Context, interaction: Interaction) {
		if let Interaction::Command(interaction) = interaction {
			let result = match interaction.data.name.as_str() {
				"find_coordinates" => {
					geocoding::handle_find_coordinates(&context, &interaction).await
				}
				"current" => {
					current::handle_current(&context, &interaction, &self.database, &self.font)
						.await
				}
				"hourly" => {
					hourly::handle_hourly(
						&context,
						&interaction,
						&self.database,
						&self.font,
						&self.header_font,
					)
					.await
				}
				"soil_moisture" => {
					hourly_soil::handle_hourly_soil(
						&context,
						&interaction,
						&self.database,
						&self.font,
						&self.header_font,
					)
					.await
				}
				"absolute_humidity" => {
					hourly_absolute_humidity::handle_hourly_absolute_humidity(
						&context,
						&interaction,
						&self.database,
						&self.font,
						&self.header_font,
					)
					.await
				}
				"daily" => {
					daily::handle_daily(
						&context,
						&interaction,
						&self.database,
						&self.font,
						&self.header_font,
					)
					.await
				}
				"sun" => sunrise_sunset::handle_sun(&context, &interaction, &self.database).await,
				"set_location" => {
					user_locations::handle_set_location(&context, &interaction, &self.database)
						.await
				}
				"unset_location" => {
					user_locations::handle_unset_location(&context, &interaction, &self.database)
						.await
				}
				name => return println!("Unknown command: {name}"),
			};
			match result {
				Err(Error::Friendly(text)) => {
					let _ = interaction.ephemeral_reply(&context.http, text).await;
				}
				Err(Error::Unfriendly(error)) => {
					println!("{}", error);
					let _ = interaction.ephemeral_reply(&context.http, "Error").await;
				}
				Ok(_) => (),
			};
		}
	}
	async fn ready(&self, context: Context, _ready: Ready) {
		println!("Ready");
		let arg = std::env::args().nth(1);
		if Some("register") == arg.as_deref() {
			let commands = Vec::from([
				geocoding::create_find_coordinates(),
				current::create_current(),
				hourly::create_hourly(),
				hourly_soil::create_hourly_soil(),
				hourly_absolute_humidity::create_hourly_absolute_humidity(),
				daily::create_daily(),
				user_locations::create_set_location(),
				user_locations::create_unset_location(),
				sunrise_sunset::create_sun(),
			]);
			for guild in context.cache.guilds() {
				let commands = guild
					.set_commands(&context.http, commands.clone())
					.await
					.unwrap();
				let command_names = commands.into_iter().map(|command| command.name).join(", ");
				println!(
					"I now have the following guild slash commands in guild {}: {}",
					guild.get(),
					command_names
				);
			}
		}
	}
}
