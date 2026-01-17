use serde::Deserialize;
use trading_core::error::EngineError;

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
