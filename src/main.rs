use std::fs;

use database::init_database;
use discord_event_handler::DiscordEventHandler;
use location::Coordinates;
use serenity::all::GatewayIntents;

mod current;
mod daily_forecast;
mod database;
mod discord_event_handler;
mod error;
mod geocoding;
mod hourly_forecast;
mod location;
mod reply_shortcuts;
mod sunrise_sunset;
mod user_locations;
mod util;

#[tokio::main]
async fn main() {
	let db_pool = init_database("./data/db.db").await;

	let font_data: &[u8] = include_bytes!("../RobotoCondensed-Regular.ttf");
	let font = ab_glyph::FontRef::try_from_slice(font_data).expect("Failed to read font");
	let font_data: &[u8] = include_bytes!("../Roboto-Black.ttf");
	let header_font = ab_glyph::FontRef::try_from_slice(font_data).expect("Failed to read font");

	let discord_token = fs::read_to_string("./token.txt").expect("Could not read token file");

	let _init = Coordinates::parse(r#"1°2'3"N4°5'6"E"#).unwrap();

	let handler = DiscordEventHandler::new(db_pool, font, header_font);
	let mut client = serenity::Client::builder(&discord_token, GatewayIntents::empty())
		.event_handler(handler)
		.await
		.expect("Error creating Discord client");

	if let Err(why) = client.start().await {
		eprintln!("Error with client: {:?}", why);
	}
}
