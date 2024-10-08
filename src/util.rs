/// Convert a `f32` into a `i32` and multiply it by 100, because the graph drawing library uses them this way often.
pub fn convert_num(n: f32) -> i32 {
	(n * 100.0).round() as i32
}

pub fn weather_code_to_str(weather_code: u8) -> Option<&'static str> {
	let str = match weather_code {
		0 => "clear sky",
		1 => "mainly clear",
		2 => "partly cloudy",
		3 => "overcast",
		45 => "fog",
		48 => "rime-depositing fog",
		51 => "light drizzle",
		53 => "moderate drizzle",
		55 => "dense drizzle",
		56 => "light, freezing drizzle",
		57 => "dense, freezing drizzle",
		61 => "slight rain",
		63 => "moderate rain",
		65 => "heavy rain",
		66 => "light, freezing rain",
		67 => "heavy, freezing rain",
		71 => "slight snowfall",
		73 => "moderate snowfall",
		75 => "heavy snowfall",
		77 => "snow grains",
		80 => "slight rain showers",
		81 => "moderate rain showers",
		82 => "violent rains howers",
		85 => "slight snow showers",
		86 => "heavy snow showers",
		95 => "thunderstorm",
		96 => "thunderstorm with slight hail",
		99 => "thunderstorm with heavy hail",
		num => {
			println!("Unknown weather code: {num}");
			return None;
		}
	};
	Some(str)
}
