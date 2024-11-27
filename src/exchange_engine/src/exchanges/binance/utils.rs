use core_logic::error::EngineError;
use reqwest::Client;
use serde_json::Value;

pub async fn fetch_server_time() -> Result<u64, EngineError> {
    let client = Client::new();
    let url = "https://api.binance.com/api/v3/time";
    let response = client.get(url).send().await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        let json: Value = serde_json::from_str(&response_text)?;
        Ok(json["serverTime"].as_u64().unwrap_or_default())
    } else {
        Err(EngineError::HttpCustomError(format!(
            "Failed to fetch server time from Binance"
        )))
    }
}
