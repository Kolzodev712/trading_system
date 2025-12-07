//! Common error types for the trading system
use anyhow::Error;
use reqwest::header::{InvalidHeaderName, InvalidHeaderValue};
use smol_str::SmolStr;
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinError;
use tokio_tungstenite::tungstenite;
use url::ParseError;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Channel send error: {0}")]
    ChannelSendError(SmolStr),

    #[error("Channel receive error: {0}")]
    ChannelReceiveError(#[from] TryRecvError),

    #[error("Channel Closed error.")]
    ChannelClosedError,

    #[error("Connection Closed")]
    ConnectionClosed,

    #[error("Task join error: {0}")]
    JoinError(#[from] JoinError),

    #[error("Missing Message Handler")]
    NoMessageHandler,

    #[error("WebSocket connection error: {0}")]
    WsConnectionError(#[from] Box<tungstenite::Error>),

    #[error("TCP stream error: {0}")]
    TcpStreamError(#[from] tokio::io::Error),

    #[error("URL parsing error: {0}")]
    UrlParseError(#[from] ParseError),

    #[error("Authentication error: {0}")]
    AuthenticationError(Error),

    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("HTTP custom error: {0}")]
    HttpCustomError(SmolStr),

    #[error("Invalid header name: {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),

    #[error("Invalid header value: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error("JSON serialization/deserialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("JSON QS error: {0}")]
    JsonQueryStringError(#[from] serde_qs::Error),

    #[error("JSON encoded error: {0}")]
    JsonUrlEncodedDeserError(#[from] serde_urlencoded::de::Error),

    #[error("JSON encoded serialization error: {0}")]
    JsonUrlEncodedSerError(#[from] serde_urlencoded::ser::Error),

    #[error("Order not found error: {0}")]
    OrderNotFound(Error),

    #[error("Order already filled error: {0}")]
    OrderAlreadyFilled(Error),

    #[error("Order already cancelled error: {0}")]
    OrderAlreadyCanceled(Error),

    #[error("Invalid config")]
    InvalidConfig,

    #[error("Unsupported Exchange")]
    UnsupportedExchange,

    #[error("Custom error: {0}")]
    CustomError(Error),
}

impl EngineError {
    pub fn new_custom_error(msg: SmolStr) -> Self {
        EngineError::CustomError(anyhow::anyhow!(msg))
    }
}

impl From<tungstenite::Error> for EngineError {
    fn from(err: tungstenite::Error) -> Self {
        EngineError::WsConnectionError(Box::new(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_error_new_custom_error() {
        let error = EngineError::new_custom_error("test error".into());
        match error {
            EngineError::CustomError(_) => {}
            _ => panic!("Expected CustomError"),
        }
    }

    #[test]
    fn test_engine_error_display() {
        let error = EngineError::NoMessageHandler;
        let display = format!("{}", error);
        assert!(display.contains("Missing Message Handler"));
    }

    #[test]
    fn test_engine_error_channel_send_error() {
        let error = EngineError::ChannelSendError("test".into());
        let display = format!("{}", error);
        assert!(display.contains("Channel send error"));
        assert!(display.contains("test"));
    }

    #[test]
    fn test_engine_error_unsupported_exchange() {
        let error = EngineError::UnsupportedExchange;
        let display = format!("{}", error);
        assert!(display.contains("Unsupported Exchange"));
    }

    #[test]
    fn test_engine_error_invalid_config() {
        let error = EngineError::InvalidConfig;
        let display = format!("{}", error);
        assert!(display.contains("Invalid config"));
    }
}
