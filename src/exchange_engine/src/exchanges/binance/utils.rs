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
            .map_err(EngineError::JsonError)
    }
}
