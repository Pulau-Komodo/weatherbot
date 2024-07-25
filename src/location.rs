use std::fmt::Display;

use serenity::all::{GuildId, UserId};
use sqlx::{query, Pool, Sqlite};

use crate::{error::Error, geocoding::GeocodingResult};

#[derive(Debug, Clone, Copy)]
pub struct Coordinates {
	pub latitude: f32,
	pub longitude: f32,
}

impl Coordinates {
	pub fn new(latitude: f32, longitude: f32) -> Self {
		Self {
			latitude,
			longitude,
		}
	}
}

impl Display for Coordinates {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("{}, {}", self.latitude, self.longitude))
	}
}

pub struct Location {
	name: Option<String>,
	coordinates: Coordinates,
	country: Option<String>,
	feature_code: Option<String>,
}

impl Location {
	pub fn from_geocoding_result(geocoding: GeocodingResult) -> Self {
		Self {
			name: Some(geocoding.name),
			coordinates: Coordinates::new(geocoding.latitude, geocoding.longitude),
			country: geocoding.country,
			feature_code: Some(geocoding.feature_code),
		}
	}
	pub fn from_coords(latitude: f32, longitude: f32) -> Self {
		Self {
			name: None,
			coordinates: Coordinates::new(latitude, longitude),
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
			coordinates: Coordinates::new(result.latitude as f32, result.longitude as f32),
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
			self.coordinates.latitude,
			self.coordinates.longitude,
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
	pub fn coordinates(&self) -> Coordinates {
		self.coordinates
	}
	pub fn country(&self) -> &str {
		self.country.as_deref().unwrap_or("unspecified")
	}
	pub fn feature_code(&self) -> &str {
		self.feature_code.as_deref().unwrap_or("unspecified")
	}
}
