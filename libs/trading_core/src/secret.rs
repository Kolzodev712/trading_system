//! Secure wrapper for sensitive data (API keys, secrets, etc.)
//!
//! This module provides `SecretString` which prevents accidental exposure
//! of sensitive data in logs, Debug output, or Display formatting.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;
use std::fmt;

/// A secure wrapper around a string that prevents accidental exposure
/// in logs, Debug output, or Display formatting.
///
/// The value is still accessible via `expose()` when needed, but
/// Debug and Display will show `***REDACTED***` instead of the actual value.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString {
    inner: SmolStr,
}

impl SecretString {
    /// Create a new `SecretString` from a string
    pub fn new<S: Into<SmolStr>>(value: S) -> Self {
        Self {
            inner: value.into(),
        }
    }

    /// Expose the secret value (use with caution!)
    /// This should only be called when absolutely necessary (e.g., for signing requests)
    pub fn expose(&self) -> &str {
        self.inner.as_str()
    }

    /// Expose the secret value as SmolStr
    pub fn expose_smol_str(&self) -> &SmolStr {
        &self.inner
    }

    /// Convert to SmolStr (consumes self)
    pub fn into_smol_str(self) -> SmolStr {
        self.inner
    }

    /// Get the length of the secret (without exposing it)
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if the secret is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl From<SmolStr> for SecretString {
    fn from(value: SmolStr) -> Self {
        Self::new(value)
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***REDACTED***)")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***REDACTED***")
    }
}

// Serialization: serialize the actual value (needed for config files)
impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.serialize(serializer)
    }
}

// Deserialization: deserialize into SecretString
impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = SmolStr::deserialize(deserializer)?;
        Ok(SecretString::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_string_creation() {
        let secret = SecretString::new("my_secret_key");
        assert_eq!(secret.len(), 13);
        assert!(!secret.is_empty());
    }

    #[test]
    fn test_secret_string_expose() {
        let secret = SecretString::new("my_secret_key");
        assert_eq!(secret.expose(), "my_secret_key");
    }

    #[test]
    fn test_secret_string_debug() {
        let secret = SecretString::new("my_secret_key");
        let debug_str = format!("{:?}", secret);
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("my_secret_key"));
    }

    #[test]
    fn test_secret_string_display() {
        let secret = SecretString::new("my_secret_key");
        let display_str = format!("{}", secret);
        assert_eq!(display_str, "***REDACTED***");
    }

    #[test]
    fn test_secret_string_serde() {
        let secret = SecretString::new("my_secret_key");
        let serialized = serde_json::to_string(&secret).unwrap();
        assert!(serialized.contains("my_secret_key"));

        let deserialized: SecretString = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.expose(), "my_secret_key");
    }

    #[test]
    fn test_secret_string_from() {
        let from_str = SecretString::from("test");
        assert_eq!(from_str.expose(), "test");

        let from_string = SecretString::from(String::from("test"));
        assert_eq!(from_string.expose(), "test");

        let from_smol_str = SecretString::from(SmolStr::new("test"));
        assert_eq!(from_smol_str.expose(), "test");
    }
}
