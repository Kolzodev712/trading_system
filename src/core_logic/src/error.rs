// General level error abstraction for the trading system
use anyhow::Error;
use reqwest::{
    self,
    header::{InvalidHeaderName, InvalidHeaderValue},
};
use smol_str::SmolStr;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio_tungstenite::tungstenite;
use url::ParseError;

#[derive(Error, Debug)]
pub enum KolzoError {
    #[error("Channel send error: {0}")]
    ChannelSendError(#[from] SendError<SmolStr>),

    #[error("WebSocket connection error: {0}")]
    WsConnectionError(#[from] tungstenite::Error),

    #[error("TCP stream error: {0}")]
    TcpStreamError(#[from] tokio::io::Error),

    #[error("URL parsing error: {0}")]
    UrlParseError(#[from] ParseError),

    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("Invalid header name: {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),

    #[error("Invalid header value: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error("JSON serialization/deserialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Order not found error: {0}")]
    OrderNotFound(Error),

    #[error("Order already filled error: {0}")]
    OrderAlreadyFilled(Error),

    #[error("Order already cancelled error: {0}")]
    OrderAlreadyCanceled(Error),

    #[error("Custom error: {0}")]
    CustomError(Error),
}

impl KolzoError {
    pub fn new_custom_error(msg: SmolStr) -> Self {
        KolzoError::CustomError(anyhow::anyhow!(msg))
    }
}
