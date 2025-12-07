use serde::Deserialize;
use trading_core::error::EngineError;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestStruct {
        value: u32,
    }

    #[test]
    fn test_deserialize_or_none_valid_json() {
        let json = r#"{"value": 42}"#;
        let result: Option<TestStruct> = deserialize_or_none(json).unwrap();
        assert_eq!(result, Some(TestStruct { value: 42 }));
    }

    #[test]
    fn test_deserialize_or_none_empty_string() {
        let json = "";
        let result: Option<TestStruct> = deserialize_or_none(json).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_or_none_whitespace_only() {
        let json = "   ";
        let result: Option<TestStruct> = deserialize_or_none(json).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_or_none_empty_object() {
        let json = "{}";
        let result: Option<TestStruct> = deserialize_or_none(json).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_or_none_empty_object_with_whitespace() {
        let json = "  {}  ";
        let result: Option<TestStruct> = deserialize_or_none(json).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_or_none_invalid_json() {
        let json = "{invalid json}";
        let result: Result<Option<TestStruct>, EngineError> = deserialize_or_none(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_or_none_missing_field() {
        let json = r#"{"other": 42}"#;
        let result: Result<Option<TestStruct>, EngineError> = deserialize_or_none(json);
        assert!(result.is_err());
    }
}
