use smol_str::SmolStr;
use thiserror::Error;
use trading_core::error::EngineError;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BinanceError {
    #[error("HTTP Error: {0}")]
    HttpError(SmolStr),

    #[error("API Error: {0}")]
    ApiError(SmolStr),

    #[error("Authentication error: {0}")]
    AuthenticationError(SmolStr),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(SmolStr),

    #[error("Order not found: {0}")]
    OrderNotFound(SmolStr),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(SmolStr),

    #[error("Invalid request: {0}")]
    InvalidRequest(SmolStr),

    #[error("Server error: {0}")]
    ServerError(SmolStr),
}

impl BinanceError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BinanceError::RateLimitExceeded(_) | BinanceError::ServerError(_)
        )
    }

    pub fn is_auth_error(&self) -> bool {
        matches!(self, BinanceError::AuthenticationError(_))
    }

    pub fn is_order_error(&self) -> bool {
        matches!(
            self,
            BinanceError::OrderNotFound(_) | BinanceError::InsufficientBalance(_)
        )
    }
}

impl From<BinanceError> for EngineError {
    fn from(err: BinanceError) -> Self {
        match &err {
            BinanceError::OrderNotFound(_) => EngineError::OrderNotFound(anyhow::anyhow!("{err}")),
            BinanceError::AuthenticationError(_) => {
                EngineError::AuthenticationError(anyhow::anyhow!("{err}"))
            }
            BinanceError::RateLimitExceeded(_) => {
                EngineError::RateLimitError(anyhow::anyhow!("{err}"))
            }
            _ => EngineError::CustomError(anyhow::anyhow!("{err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        assert!(BinanceError::RateLimitExceeded(SmolStr::new("test")).is_retryable());
        assert!(BinanceError::ServerError(SmolStr::new("test")).is_retryable());
        assert!(!BinanceError::OrderNotFound(SmolStr::new("test")).is_retryable());
        assert!(!BinanceError::AuthenticationError(SmolStr::new("test")).is_retryable());
    }

    #[test]
    fn test_is_auth_error() {
        assert!(BinanceError::AuthenticationError(SmolStr::new("test")).is_auth_error());
        assert!(!BinanceError::OrderNotFound(SmolStr::new("test")).is_auth_error());
    }

    #[test]
    fn test_is_order_error() {
        assert!(BinanceError::OrderNotFound(SmolStr::new("test")).is_order_error());
        assert!(BinanceError::InsufficientBalance(SmolStr::new("test")).is_order_error());
        assert!(!BinanceError::AuthenticationError(SmolStr::new("test")).is_order_error());
    }

    #[test]
    fn test_to_engine_error() {
        let err: EngineError = BinanceError::OrderNotFound(SmolStr::new("test")).into();
        match err {
            EngineError::OrderNotFound(_) => {}
            _ => panic!("Expected OrderNotFound"),
        }

        let err: EngineError = BinanceError::AuthenticationError(SmolStr::new("test")).into();
        match err {
            EngineError::AuthenticationError(_) => {}
            _ => panic!("Expected AuthenticationError"),
        }
    }
}
