// General level error abstraction for the trading system
use anyhow::Error;
use reqwest::{
    self,
    header::{InvalidHeaderName, InvalidHeaderValue},
};
use smol_str::SmolStr;
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;
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

    #[error("WebSocket connection error: {0}")]
    WsConnectionError(#[from] tungstenite::Error),

    #[error("TCP stream error: {0}")]
    TcpStreamError(#[from] tokio::io::Error),

    #[error("URL parsing error: {0}")]
    UrlParseError(#[from] ParseError),

    #[error("Authentication error: {0}")]
    AuthenticationError(Error),

    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("HTTP custom error: {0}")]
    HttpCustomError(String),

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

    #[error("Custom error: {0}")]
    CustomError(Error),
}

impl EngineError {
    pub fn new_custom_error(msg: SmolStr) -> Self {
        EngineError::CustomError(anyhow::anyhow!(msg))
    }
}
