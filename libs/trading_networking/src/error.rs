//! Errors for the networking layer.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("serialization failed: {0}")]
    SerializationFailed(String),

    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "zenoh")]
    #[error("zenoh error: {0}")]
    Zenoh(#[from] zenoh::Error),
}

/// Alias for backward compatibility.
pub type DtpError = NetworkError;
