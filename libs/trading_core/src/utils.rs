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
