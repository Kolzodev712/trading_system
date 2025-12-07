use reqwest::Error as ReqwestError;
use smol_str::SmolStr;
use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BinanceError {
    #[error("HTTP Error: {0}")]
    HttpError(SmolStr),
    #[error("API Error: {0}")]
    ApiError(SmolStr),
}

impl From<ReqwestError> for BinanceError {
    fn from(error: ReqwestError) -> Self {
        BinanceError::HttpError(SmolStr::new(error.to_string()))
    }
}
