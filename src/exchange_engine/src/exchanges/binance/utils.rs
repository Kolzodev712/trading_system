use core_logic::error::EngineError;
use serde::Deserialize;

/// Deserializes a JSON string into an optional type.
///
/// # Parameters
/// - `text`: A JSON string to deserialize.
///
/// # Returns
/// - `Ok(Some(Res))` if the input is valid JSON for type `Res`.
/// - `Ok(None)` if the input is empty or `{}`.
/// - `Err(EngineError::JsonError)` if deserialization fails for non-empty input.
///
/// # Type Parameters
/// - `Res`: The type to deserialize into, requiring `serde::Deserialize`.
pub fn deserialize_or_none<Res>(text: &str) -> Result<Option<Res>, EngineError>
where
    Res: for<'de> Deserialize<'de>,
{
    if text.trim().is_empty() || text.trim() == "{}" {
        Ok(None)
    } else {
        serde_json::from_str::<Res>(text)
            .map(Some)
            .map_err(|e| EngineError::JsonError(e))
    }
}

use reqwest::Client;
#[derive(Deserialize)]
struct BinanceTimestampResponse {
    server_time: u64,
}

pub struct BinanceTimestampService {
    client: Client,
}

impl BinanceTimestampService {
    pub fn new() -> Self {
        BinanceTimestampService {
            client: Client::new(),
        }
    }

    /// Temporary workaround for the binance timestamp:
    pub async fn get_server_timestamp(&self) -> Result<u64, EngineError> {
        let response = self
            .client
            .get("https://api.binance.com/api/v3/time")
            .send()
            .await
            .map_err(|e| EngineError::CustomError(e.into()))?;

        let timestamp_response: BinanceTimestampResponse = response
            .json()
            .await
            .map_err(|e| EngineError::CustomError(e.into()))?;

        Ok(timestamp_response.server_time)
    }
}
