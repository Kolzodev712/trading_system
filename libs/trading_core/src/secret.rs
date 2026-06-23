//! Secure wrappers for sensitive config values (API keys, private keys, connection strings).
//!
//! `Debug` and `Display` redact values; use `expose()` only at trust boundaries (signing, auth).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;
use std::fmt;

/// Redacted secret container. Prefer `Secret<SmolStr>` for compact config strings.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T> {
    inner: T,
}

/// SmolStr-backed secret (API keys, private keys, tokens).
pub type SecretString = Secret<SmolStr>;

impl<T> Secret<T> {
    pub fn wrap(value: T) -> Self {
        Self { inner: value }
    }

    /// Borrow the inner value (use only when required, e.g. signing).
    pub fn expose_inner(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl Secret<SmolStr> {
    pub fn expose(&self) -> &str {
        self.inner.as_str()
    }

    pub fn expose_smol_str(&self) -> &SmolStr {
        &self.inner
    }

    pub fn into_smol_str(self) -> SmolStr {
        self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Secret<String> {
    pub fn expose(&self) -> &str {
        &self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl SecretString {
    pub fn new<S: Into<SmolStr>>(value: S) -> Self {
        Secret::wrap(value.into())
    }
}

impl From<SmolStr> for Secret<SmolStr> {
    fn from(value: SmolStr) -> Self {
        Secret::wrap(value)
    }
}

impl From<String> for Secret<SmolStr> {
    fn from(value: String) -> Self {
        Secret::wrap(SmolStr::new(value))
    }
}

impl From<&str> for Secret<SmolStr> {
    fn from(value: &str) -> Self {
        Secret::wrap(SmolStr::new(value))
    }
}

impl From<String> for Secret<String> {
    fn from(value: String) -> Self {
        Secret::wrap(value)
    }
}

impl From<&str> for Secret<String> {
    fn from(value: &str) -> Self {
        Secret::wrap(value.to_string())
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***REDACTED***)")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***REDACTED***")
    }
}

impl<T: Serialize> Serialize for Secret<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Secret<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Secret::wrap(T::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_smol_str_expose() {
        let secret = SecretString::new("my_secret_key");
        assert_eq!(secret.expose(), "my_secret_key");
        assert_eq!(secret.len(), 13);
    }

    #[test]
    fn secret_smol_str_debug_redacts() {
        let secret = SecretString::new("my_secret_key");
        let debug_str = format!("{secret:?}");
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("my_secret_key"));
    }

    #[test]
    fn secret_smol_str_serde() {
        let secret = SecretString::new("my_secret_key");
        let serialized = serde_json::to_string(&secret).unwrap();
        assert!(serialized.contains("my_secret_key"));

        let deserialized: SecretString = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.expose(), "my_secret_key");
    }

    #[test]
    fn secret_string_redacts() {
        let secret = Secret::<String>::from("mongodb://user:pass@localhost");
        assert_eq!(secret.expose(), "mongodb://user:pass@localhost");
        assert!(format!("{secret:?}").contains("REDACTED"));
    }
}
