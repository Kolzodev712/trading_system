use reqwest::Error as ReqwestError;
use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BinanceError {
    #[error("HTTP Error: {0}")]
    HttpError(String),
    #[error("API Error: {0}")]
    ApiError(String),
}

impl From<ReqwestError> for BinanceError {
    fn from(error: ReqwestError) -> Self {
        BinanceError::HttpError(error.to_string())
    }
}
