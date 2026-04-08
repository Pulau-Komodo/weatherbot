use std::{fmt::Display, sync::LazyLock};

use itertools::Itertools;
use regex::Regex;
use reqwest::Client;
use serenity::all::{CommandInteraction, GuildId, UserId};
use sqlx::{Pool, Sqlite, query};

use crate::{error::Error, geocoding::GeocodingResult};

/// Latitude or longitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeoAxis {
	Latitude,
	Longitude,
}

/// North, south, east and west stored in a way convenient for processing input.
struct Direction {
	geoaxis: GeoAxis,
	sign: f32,
}

impl Direction {
	fn get(char: char) -> Self {
		match char {
			'N' | 'n' => Self {
				geoaxis: GeoAxis::Latitude,
				sign: 1.0,
			},
			'S' | 's' => Self {
				geoaxis: GeoAxis::Latitude,
				sign: -1.0,
			},
			'E' | 'e' => Self {
				geoaxis: GeoAxis::Longitude,
				sign: 1.0,
			},
			'W' | 'w' => Self {
				geoaxis: GeoAxis::Longitude,
				sign: -1.0,
			},
			_ => unreachable!("Unexpected direction character"),
		}
	}
}

static SIMPLE_COORDS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r"^([+-]?\s*(?:\d+(?:\.\d+)?|\.\d+))(?:\s+|\s*,\s*)([+-]?\s*(?:\d+(?:\.\d+)?|\.\d+))$",
	)
	.unwrap()
});
static FANCIER_COORDS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"(?i)^(\d{1,3})°\s*(\d{1,2})[\u2032']\s*(\d{1,2})[″"]\s*([NESW])\s*,?\s*(\d{1,3})°\s*(\d{1,2})[\u2032']\s*(\d{1,2})[″"]\s*([NESW])$"#).unwrap()
});

/// Latitude and longitude are in degrees.
#[derive(Debug, Clone, Copy)]
pub struct Coordinates {
	/// How far above the equator
	pub latitude: f32,
	/// How far east of the IERS Reference Meridian, which goes through Greenwich
	pub longitude: f32,
}

impl Coordinates {
	/// Latitude and longitude are in degrees.
	pub fn new(latitude: f32, longitude: f32) -> Self {
		Self {
			latitude,
			longitude,
		}
	}
	/// Attempt to parse a string describing coordinates.
	///
	/// It currently supports two formats:
	///
	/// Decimal: `52.87619043426636, -118.0795914761888` (Google Maps gives this on right click) (comma optional)
	///
	/// Degrees, minutes, seconds: `52° 52′ 34″ N, 118° 4′ 46″ W` (does not support decimals, spaces and comma optional, `′` and `″` can be `'` and `"` instead)
	pub fn parse(input: &str) -> Option<Self> {
		if let Some(captures) = SIMPLE_COORDS_REGEX.captures(input)
			&& let Some((Ok(latitude), Ok(longitude))) = captures
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

		if let Some(captures) = FANCIER_COORDS_REGEX.captures(input)
			&& let Some((
				degrees_a,
				minutes_a,
				seconds_a,
				direction_a,
				degrees_b,
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
			let direction_a = Direction::get(direction_a.chars().next().unwrap());
			let direction_b = Direction::get(direction_b.chars().next().unwrap());
			let (hours_a, minutes_a, seconds_a, hours_b, minutes_b, seconds_b) = [
				degrees_a, minutes_a, seconds_a, degrees_b, minutes_b, seconds_b,
			]
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
		None
	}
	fn get_axis_mut(&mut self, geo_axis: GeoAxis) -> &mut f32 {
		match geo_axis {
			GeoAxis::Latitude => &mut self.latitude,
			GeoAxis::Longitude => &mut self.longitude,
		}
	}
	/// The distance of the shortest path over the surface of Earth, simplified to a sphere, from one point on it to another.
	///
	/// This uses the Haversine formula.
	pub fn distance_to(&self, other: Self) -> Distance {
		const EARTH_RADIUS: f32 = 6371e3; // Earth's radius in meters
		let [a_lat, a_lon, b_lat, b_lon] = [
			self.latitude,
			self.longitude,
			other.latitude,
			other.longitude,
		]
		.map(f32::to_radians);
		let delta_lat = a_lat - b_lat;
		let delta_lon = a_lon - b_lon;

		let a = (delta_lat / 2.0).sin().powi(2)
			+ a_lat.cos() * b_lat.cos() * (delta_lon / 2.0).sin().powi(2);
		let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

		Distance::meters(c * EARTH_RADIUS)
	}
}

impl Display for Coordinates {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("{}, {}", self.latitude, self.longitude))
	}
}

#[derive(Debug, Clone, Copy)]
/// A distance that implements `Display` to show in Mm, km, m or mm, with 3 significant digits.
pub struct Distance {
	meters: f32,
}

impl Distance {
	pub fn meters(meters: f32) -> Self {
		Self { meters }
	}
	pub fn is_zero(&self) -> bool {
		self.meters == 0.0
	}
}

impl Display for Distance {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let abs = self.meters.abs();
		let (value, unit) = if abs >= 1_000_000.0 {
			(abs / 1_000_000.0, "Mm")
		} else if abs >= 1_000.0 {
			(abs / 1_000.0, "km")
		} else if abs >= 1.0 {
			(abs, "m")
		} else {
			(abs * 1_000.0, "mm")
		};

		let digits = if abs == 0.0 {
			0
		} else {
			3 - value.log10().floor() as i32 - 1
		}
		.max(0);

		f.write_fmt(format_args!("{:.*}{}", digits as usize, value, unit))
	}
}

/// A location, consisting of coordinates and optional information about it.
///
/// The `Display` implementation formats it like "Paris, France (48.85341, 2.3488)" or, if name is not available, like "48.85341, 2.3488".
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
	pub async fn get_from_argument_or_for_user(
		interaction: &CommandInteraction,
		client: &Client,
		database: &Pool<Sqlite>,
	) -> Result<Self, Error> {
		let location = match interaction
			.data
			.options
			.first()
			.and_then(|option| option.value.as_str())
		{
			Some(arg) => Location::try_from_arg(arg, client).await?,
			None => Location::get_for_user(
				database,
				interaction.user.id,
				interaction
					.guild_id
					.ok_or_else(|| Error::custom_unfriendly("Somehow could not get guild ID"))?,
			)
			.await?
			.ok_or_else(|| Error::friendly("No location set, and no location provided"))?,
		};
		Ok(location)
	}
	pub fn name(&self) -> Option<&str> {
		self.name.as_deref()
	}
	pub fn coordinates(&self) -> Coordinates {
		self.coordinates
	}
	pub fn country(&self) -> Option<&str> {
		self.country.as_deref()
	}
	pub fn feature_code(&self) -> Option<&str> {
		self.feature_code.as_deref()
	}
}

impl Display for Location {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if let Some(name) = &self.name {
			f.write_fmt(format_args!(
				"{}, {} ({})",
				name,
				self.country().unwrap_or("unspecified"),
				self.coordinates()
			))
		} else {
			f.write_fmt(format_args!("{}", self.coordinates))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn is_close_enough(num_one: f32, num_two: f32, precision: i32) -> bool {
		let delta = num_one.abs() * 1.0 / 10.0f32.powi(precision);
		let start = num_one - delta;
		let end = num_one + delta;
		let is_close_enough = (start..end).contains(&num_two);
		if !is_close_enough {
			println!(
				"Num {} is not close enough to {} (precision: {}, range: {:?})",
				num_two,
				num_one,
				precision,
				start..end
			);
		}
		is_close_enough
	}

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

		assert!(is_close_enough(latitude, coords.latitude, 6));
		assert!(is_close_enough(longitude, coords.longitude, 6));
	}
	#[test]
	fn coord_parsing_doc_example() {
		let coords_a = dbg!(Coordinates::parse("52.87619043426636, -118.0795914761888").unwrap());
		let coords_b = dbg!(Coordinates::parse("52° 52′ 34″ N, 118° 4′ 46″ W").unwrap());

		assert!(is_close_enough(coords_a.latitude, coords_b.latitude, 5));
		assert!(is_close_enough(coords_a.longitude, coords_b.longitude, 5));
	}
}
