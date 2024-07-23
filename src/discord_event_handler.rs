use itertools::Itertools;
use serenity::{
	all::{Context, EventHandler, Interaction, Ready},
	async_trait,
};
use sqlx::{Pool, Sqlite};

use crate::{
	geocoding::{self, handle_find_coordinates},
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
				"find_coordinates" => handle_find_coordinates(context, interaction).await,
				"uvi" => handle_uvi(context, interaction, &self.font).await,
				name => return println!("Unknown command: {name}"),
			};
			if let Err(error) = result {
				println!("{}", error);
			}
		}
	}
	async fn ready(&self, context: Context, _ready: Ready) {
		println!("Ready");
		let arg = std::env::args().nth(1);
		if Some("register") == arg.as_deref() {
			let mut commands = Vec::new();
			commands.push(geocoding::create_find_coordinates());
			commands.push(uv::create_uvi());
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
