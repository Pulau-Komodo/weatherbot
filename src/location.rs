use std::{cell::LazyCell, fmt::Display};

use itertools::Itertools;
use regex::Regex;
use reqwest::Client;
use serenity::all::{GuildId, UserId};
use sqlx::{query, Pool, Sqlite};

use crate::{error::Error, geocoding::GeocodingResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeoAxis {
	Latitude,
	Longitude,
}

struct Direction {
	letter: char,
	geoaxis: GeoAxis,
	sign: f32,
}

const DIRECTIONS: [Direction; 4] = [
	Direction {
		letter: 'N',
		geoaxis: GeoAxis::Latitude,
		sign: 1.0,
	},
	Direction {
		letter: 'S',
		geoaxis: GeoAxis::Latitude,
		sign: -1.0,
	},
	Direction {
		letter: 'E',
		geoaxis: GeoAxis::Longitude,
		sign: 1.0,
	},
	Direction {
		letter: 'W',
		geoaxis: GeoAxis::Longitude,
		sign: -1.0,
	},
];

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
	pub fn parse(input: &str) -> Option<Self> {
		let simple_regex = LazyCell::new(|| {
			Regex::new(
				r"^([+-]?\s*(?:\d+(?:\.\d+)?|\.\d+))(?:\s+|\s*,\s*)([+-]?\s*(?:\d+(?:\.\d+)?|\.\d+))$",
			)
			.unwrap()
		});
		let fancier_regex = LazyCell::new(|| {
			Regex::new(r#"(?i)^(\d{1,3})°(\d{1,2})[\u2032'](\d{1,2})[″"]\s*([NESW])\s*(\d{1,3})°(\d{1,2})[\u2032'](\d{1,2})[″"]\s*([NESW])$"#).unwrap()
		});
		if let Some(captures) = simple_regex.captures(input) {
			if let Some((Ok(latitude), Ok(longitude))) = captures
				.iter()
				.skip(1)
				.flatten()
				.map(|capture| capture.as_str().parse::<f32>())
				.collect_tuple()
			{
				return Some(Self {
					latitude,
					longitude,
				});
			}
		}

		if let Some(captures) = fancier_regex.captures(input) {
			if let Some((
				hours_a,
				minutes_a,
				seconds_a,
				direction_a,
				hours_b,
				minutes_b,
				seconds_b,
				direction_b,
			)) = captures
				.iter()
				.skip(1)
				.flatten()
				.map(|capture| capture.as_str())
				.collect_tuple()
			{
				let direction_a = DIRECTIONS
					.iter()
					.find(|dir| {
						dir.letter == direction_a.chars().next().unwrap().to_ascii_uppercase()
					})
					.unwrap();
				let direction_b = DIRECTIONS
					.iter()
					.find(|dir| {
						dir.letter == direction_b.chars().next().unwrap().to_ascii_uppercase()
					})
					.unwrap();
				let (hours_a, minutes_a, seconds_a, hours_b, minutes_b, seconds_b) =
					[hours_a, minutes_a, seconds_a, hours_b, minutes_b, seconds_b]
						.into_iter()
						.filter_map(|str| str.parse::<f32>().ok())
						.collect_tuple()?;
				if direction_a.geoaxis == direction_b.geoaxis {
					return None; // Invalid combination of directions
				}
				let magnitude_a = hours_a + minutes_a / 60.0 + seconds_a / 60.0 / 60.0;
				let magnitude_b = hours_b + minutes_b / 60.0 + seconds_b / 60.0 / 60.0;
				let mut coordinates = Self::new(0.0, 0.0);
				*coordinates.get_axis_mut(direction_a.geoaxis) = magnitude_a * direction_a.sign;
				*coordinates.get_axis_mut(direction_b.geoaxis) = magnitude_b * direction_b.sign;
				return Some(coordinates);
			}
		}
		None
	}
	fn get_axis_mut(&mut self, geo_axis: GeoAxis) -> &mut f32 {
		match geo_axis {
			GeoAxis::Latitude => &mut self.latitude,
			GeoAxis::Longitude => &mut self.longitude,
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
	pub fn from_coords(coordinates: Coordinates) -> Self {
		Self {
			name: None,
			coordinates,
			country: None,
			feature_code: None,
		}
	}
	pub async fn try_from_arg(arg: &str, client: &Client) -> Result<Self, Error> {
		if let Some(coords) = Coordinates::parse(arg) {
			return Ok(Self::from_coords(coords));
		}
		GeocodingResult::get(arg, client)
			.await
			.map(Self::from_geocoding_result)
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn coord_parsing_simple() {
		let coords = Coordinates::parse(r#"5.0, 5.0"#).unwrap();
		assert_eq!(coords.latitude, 5.0);
		assert_eq!(coords.longitude, 5.0);
	}
	#[test]
	fn coord_parsing_fancy() {
		let coords = Coordinates::parse(r#"1°2'3"N4°5'6"E"#).unwrap();
		let latitude = 1.0 + 2.0 / 60.0 + 3.0 / 60.0 / 60.0;
		let longitude = 4.0 + 5.0 / 60.0 + 6.0 / 60.0 / 60.0;
		let delta = 1.000001;
		assert!((latitude / delta..latitude * delta).contains(&coords.latitude));
		assert!((longitude / delta..longitude * delta).contains(&coords.longitude));
	}
}
