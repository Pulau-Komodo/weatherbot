use ab_glyph::{FontRef, PxScale};
use graph::{
	common_types::Range,
	drawing::{MarkIntervals, Padding, Spacing},
	generic_graph::{AxisGridLabels, Chart, Line, Rgb},
	text_box::{TextBox, TextSegment},
	util::make_png,
};
use reqwest::Client;
use serde::Deserialize;
use serenity::all::*;
use sqlx::{Pool, Sqlite};

use crate::{
	error::Error,
	hourly_forecast::hour_from_timestamp,
	location::{Coordinates, Location},
	util::convert_num,
};

#[derive(Debug, Deserialize)]
struct HourlySoilMoisture {
	time: Vec<i64>,
	soil_moisture_0_to_1cm: Vec<f32>,
	soil_moisture_1_to_3cm: Vec<f32>,
	soil_moisture_3_to_9cm: Vec<f32>,
	soil_moisture_9_to_27cm: Vec<f32>,
	soil_moisture_27_to_81cm: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct HourlySoilMoistureResult {
	#[serde(rename = "latitude")]
	_latitude: f32,
	#[serde(rename = "longitude")]
	_longitude: f32,
	utc_offset_seconds: i32,
	hourly: HourlySoilMoisture,
}

impl HourlySoilMoistureResult {
	async fn get(coordinates: Coordinates, client: &Client) -> Result<Self, Error> {
		Ok(client
			.get("https://api.open-meteo.com/v1/forecast")
			.query(&[("hourly", "soil_moisture_0_to_1cm")])
			.query(&[("hourly", "soil_moisture_1_to_3cm")])
			.query(&[("hourly", "soil_moisture_3_to_9cm")])
			.query(&[("hourly", "soil_moisture_9_to_27cm")])
			.query(&[("hourly", "soil_moisture_27_to_81cm")])
			.query(&[("timeformat", "unixtime"), ("timezone", "auto")])
			.query(&[("forecast_hours", 72)])
			.query(&[
				("latitude", coordinates.latitude),
				("longitude", coordinates.longitude),
			])
			.send()
			.await?
			.json::<HourlySoilMoistureResult>()
			.await?)
	}
}

const LABEL_SIZE: PxScale = PxScale { x: 18.0, y: 18.0 };
const AXIS_LABEL_SIZE: PxScale = PxScale { x: 14.0, y: 14.0 };

pub async fn handle_hourly_soil(
	context: &Context,
	interaction: &CommandInteraction,
	database: &Pool<Sqlite>,
	font: &FontRef<'static>,
	header_font: &FontRef<'static>,
) -> Result<(), Error> {
	let client = Client::new();
	let location = Location::get_from_argument_or_for_user(interaction, &client, database).await?;

	let result = HourlySoilMoistureResult::get(location.coordinates(), &client).await?;
	let times = result
		.hourly
		.time
		.into_iter()
		.map(|time| hour_from_timestamp(time, result.utc_offset_seconds))
		.collect::<Vec<_>>();

	let padding = Padding {
		above: 3,
		below: 19,
		left: 21,
		right: 3,
	};

	let spacing = Spacing {
		horizontal: 8,
		vertical: 3,
	};

	const COLOURS: [Rgb<u8>; 5] = [
		Rgb([255, 200, 200]),
		Rgb([255, 150, 150]),
		Rgb([255, 100, 100]),
		Rgb([200, 50, 50]),
		Rgb([150, 0, 0]),
	];

	let soil_moistures = [
		result.hourly.soil_moisture_0_to_1cm,
		result.hourly.soil_moisture_1_to_3cm,
		result.hourly.soil_moisture_3_to_9cm,
		result.hourly.soil_moisture_9_to_27cm,
		result.hourly.soil_moisture_27_to_81cm,
	];
	let data_len = soil_moistures.iter().map(|v| v.len()).max().unwrap();
	let highest_moisture = soil_moistures
		.iter()
		.flatten()
		.fold(0.0f32, |acc, m| acc.max(*m));
	let moisture_range = Range::new(0, convert_num(highest_moisture.max(0.5) * 100.0));

	let label = TextBox::new(
		&[
			TextSegment::white("Soil moisture at "),
			TextSegment::new("0 to 1", COLOURS[0]),
			TextSegment::white(", "),
			TextSegment::new("1 to 3", COLOURS[1]),
			TextSegment::white(", "),
			TextSegment::new("3 to 9", COLOURS[2]),
			TextSegment::white(", "),
			TextSegment::new("9 to 27", COLOURS[3]),
			TextSegment::white(" and "),
			TextSegment::new("27 to 81", COLOURS[4]),
			TextSegment::white(" cm"),
		],
		header_font.clone(),
		LABEL_SIZE,
		(data_len - 1) as u32 * spacing.horizontal,
		2,
	);
	let mut chart = Chart::new(
		data_len,
		moisture_range.len() as u32,
		spacing,
		Padding {
			above: padding.above + label.height(),
			..padding
		},
	);
	chart.draw(label);
	chart.draw(AxisGridLabels {
		vertical_intervals: MarkIntervals::new(5, 5),
		horizontal_intervals: MarkIntervals::new(1, 2),
		vertical_label_range: moisture_range,
		horizontal_labels: times.iter().copied(),
		horizontal_labels_centered: false,
		font: font.clone(),
		font_scale: AXIS_LABEL_SIZE,
	});

	for (index, soil_moisture) in soil_moistures.into_iter().enumerate().rev() {
		chart.draw(Line {
			colour: COLOURS[index],
			data: soil_moisture.into_iter().map(|n| convert_num(n * 100.0)),
			max: moisture_range.end(),
		});
	}

	let soil_moisture_image = chart.into_canvas();

	let image = make_png(soil_moisture_image);

	interaction
		.create_response(
			context,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.add_file(CreateAttachment::bytes(image, "hourly_soil.png")),
			),
		)
		.await?;
	Ok(())
}

pub fn create_hourly_soil() -> CreateCommand {
	CreateCommand::new("soil_moisture")
		.description("Hourly soil moisture forecast")
		.add_option(
			CreateCommandOption::new(
				CommandOptionType::String,
				"place",
				"The place to get the weather forecast of.",
			)
			.required(false),
		)
}
