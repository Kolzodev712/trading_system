use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time is before the UNIX epoch")
        .as_millis() as u64
}

pub fn generate_custom_uuid() -> String {
    Uuid::new_v4().to_string()
}

// Serialization and deserialization function for SmolStr
pub fn serialize_smol_str<S>(value: &SmolStr, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    value.as_str().serialize(serializer)
}

pub fn deserialize_smol_str<'de, D>(deserializer: D) -> Result<SmolStr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    Ok(SmolStr::new(s))
}

// Custom serialization and deserialization for Vec<SmolStr>
pub fn serialize_vec_smol_str<S>(value: &[SmolStr], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let vec_as_str: Vec<&str> = value.iter().map(|s| s.as_str()).collect();
    vec_as_str.serialize(serializer)
}

pub fn deserialize_vec_smol_str<'de, D>(deserializer: D) -> Result<Vec<SmolStr>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let vec_of_strings: Vec<String> = Vec::deserialize(deserializer)?;
    Ok(vec_of_strings.into_iter().map(SmolStr::new).collect())
}

// Serialization and deserialization function for Option<SmolStr>
pub fn serialize_option_smol_str<S>(
    value: &Option<SmolStr>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(s) => serializer.serialize_some(s.as_str()),
        None => serializer.serialize_none(),
    }
}

pub fn deserialize_option_smol_str<'de, D>(deserializer: D) -> Result<Option<SmolStr>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.map(SmolStr::new))
}

use serde::ser::Serializer;

pub fn serialize_enum_as_display<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: std::fmt::Display,
{
    serializer.serialize_str(&value.to_string())
}

pub fn serialize_option_enum_as_display<S, T>(
    value: &Option<T>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: std::fmt::Display,
{
    match value {
        Some(v) => serializer.serialize_some(&v.to_string()),
        None => serializer.serialize_none(),
    }
}

pub fn generate_string_params(params: &mut HashMap<SmolStr, SmolStr>) -> HashMap<String, String> {
    params
        .iter_mut()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_current_timestamp() {
        let ts1 = current_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ts2 = current_timestamp();
        assert!(ts2 > ts1);
    }

    #[test]
    fn test_generate_custom_uuid() {
        let uuid1 = generate_custom_uuid();
        let uuid2 = generate_custom_uuid();
        assert_ne!(uuid1, uuid2);
        assert!(!uuid1.is_empty());
        assert!(!uuid2.is_empty());
    }

    #[test]
    fn test_serialize_deserialize_smol_str() {
        let original = SmolStr::new("test_string");
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: SmolStr = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_vec_smol_str() {
        let original = vec![
            SmolStr::new("one"),
            SmolStr::new("two"),
            SmolStr::new("three"),
        ];
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Vec<SmolStr> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_option_smol_str_some() {
        let original = Some(SmolStr::new("test"));
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Option<SmolStr> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_option_smol_str_none() {
        let original: Option<SmolStr> = None;
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Option<SmolStr> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_generate_string_params() {
        let mut params = HashMap::new();
        params.insert(SmolStr::new("key1"), SmolStr::new("value1"));
        params.insert(SmolStr::new("key2"), SmolStr::new("value2"));

        let string_params = generate_string_params(&mut params);
        assert_eq!(string_params.get("key1"), Some(&"value1".to_string()));
        assert_eq!(string_params.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_generate_string_params_empty() {
        let mut params = HashMap::new();
        let string_params = generate_string_params(&mut params);
        assert!(string_params.is_empty());
    }
}
