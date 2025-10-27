use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AccuweatherDaily {
	daily_forecasts: Vec<DailyForecast>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DailyForecast {
	air_and_pollen: Vec<AirAndPollen>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AirAndPollen {
	name: String,
	value: u8,
	category_value: u8,
}

#[cfg(test)]
mod tests {
	use reqwest::Client;

	use crate::util::ResponseExt;

	use super::*;

	#[tokio::test]
	async fn test_name() {
		let client = Client::new();
		let response = client
			.get("http://dataservice.accuweather.com/forecasts/v1/daily/5day/230204")
			.query(&[
				("apikey", "iAhiwa9bbxUv1gJSXHpMSlGq58dwq6NQ"),
				("details", "true"),
				("metric", "true"),
			])
			.header("Accept-Encoding", "gzip")
			.send()
			.await
			.unwrap();
		let info: AccuweatherDaily = response.json_or_raw().await.unwrap();
		println!("{info:?}");
	}
}
