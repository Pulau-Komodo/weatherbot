use itertools::Itertools;
use serenity::{
	all::{Context, EventHandler, Interaction, Ready},
	async_trait,
};
use sqlx::{Pool, Sqlite};

use crate::{
	error::Error,
	geocoding::{self, handle_find_coordinates},
	reply_shortcuts::ReplyShortcuts,
	user_locations::{self, handle_set_location, handle_unset_location},
	uv::{self, handle_uvi},
};

pub struct DiscordEventHandler {
	database: Pool<Sqlite>,
	font: ab_glyph::FontRef<'static>,
}

impl DiscordEventHandler {
	pub fn new(database: Pool<Sqlite>, font: ab_glyph::FontRef<'static>) -> Self {
		Self { database, font }
	}
}

#[async_trait]
impl EventHandler for DiscordEventHandler {
	async fn interaction_create(&self, context: Context, interaction: Interaction) {
		if let Interaction::Command(interaction) = interaction {
			let result = match interaction.data.name.as_str() {
				"find_coordinates" => handle_find_coordinates(&context, &interaction).await,
				"uvi" => handle_uvi(&context, &interaction, &self.database, &self.font).await,
				"set_location" => handle_set_location(&context, &interaction, &self.database).await,
				"unset_location" => {
					handle_unset_location(&context, &interaction, &self.database).await
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
				uv::create_uvi(),
				user_locations::create_set_location(),
				user_locations::create_unset_location(),
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
