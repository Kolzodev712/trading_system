use smol_str::SmolStr;
use thiserror::Error;
use trading_core::error::EngineError;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
/// See: <https://bybit-exchange.github.io/docs/v5/error>
pub enum BybitError {
    #[error("Bad request (400): {0}")]
    BadRequest(SmolStr),

    #[error("Unauthorized (401): API key invalid or missing")]
    Unauthorized,

    #[error("Forbidden (403): IP rate limit breached or U.S. IP")]
    Forbidden,

    #[error("Not found (404): Invalid path or category mismatch")]
    NotFound,

    #[error("Too many requests (429): Rate limit exceeded")]
    TooManyRequests,

    #[error("Request parameter error (10001)")]
    RequestParameterError,

    #[error("Request time exceeds window range (10002)")]
    RequestTimeExceeded,

    #[error("API key invalid (10003): Check key and domain match")]
    ApiKeyInvalid,

    #[error("Signature error (10004): Check signature generation")]
    SignatureError,

    #[error("Permission denied (10005): Check API key permissions")]
    PermissionDenied,

    #[error("Rate limit exceeded (10006): Too many visits")]
    RateLimitExceeded,

    #[error("User authentication failed (10007)")]
    AuthenticationFailed,

    #[error("Account banned (10008): Check account mode")]
    AccountBanned,

    #[error("IP banned (10009)")]
    IpBanned,

    #[error("IP mismatch (10010): Check API key's bound IP addresses")]
    IpMismatch,

    #[error("Invalid duplicate request (10014)")]
    DuplicateRequest,

    #[error("Server error (10016)")]
    ServerError,

    #[error("Route not found (10017)")]
    RouteNotFound,

    #[error("API key expired - Spot (-2015)")]
    ApiKeyExpiredSpot,

    #[error("API key expired - Derivatives (33004)")]
    ApiKeyExpiredDerivatives,

    #[error("Server timeout (10000)")]
    ServerTimeout,

    #[error("Request expired (-1)")]
    RequestExpired,

    #[error("High server load (429): Trading service experiencing high load")]
    HighServerLoad,

    #[error("Order does not exist (110001)")]
    OrderNotFound,

    #[error("Order price exceeds allowable range (110003)")]
    OrderPriceExceededRange,

    #[error("Wallet balance insufficient (110004)")]
    WalletBalanceInsufficient,

    #[error("Position status error (110005)")]
    PositionStatusError,

    #[error("Available balance insufficient (110007)")]
    AvailableBalanceInsufficient,

    #[error("Order completed or cancelled (110008)")]
    OrderCompletedOrCancelled,

    #[error("Too many stop orders (110009): Exceeds maximum limit")]
    TooManyStopOrders,

    #[error("Order cancelled (110010)")]
    OrderCancelled,

    #[error("Cannot set leverage due to risk limit (110013)")]
    CannotSetLeverageRiskLimit,

    #[error("Not allowed more than 500 active orders (110020)")]
    TooManyActiveOrders,

    #[error("Cannot switch position mode with existing position (110024)")]
    CannotSwitchPositionMode,

    /// Already in the requested mode. Bybit returns this instead of retCode=0.
    #[error("Position mode is not modified (110025)")]
    PositionModeUnchanged,

    /// Already at the requested leverage. Bybit returns this instead of retCode=0.
    #[error("Leverage not modified (110043)")]
    LeverageUnchanged,

    #[error("WebSocket op type not found (10404): Category not correct/supported")]
    WsOpTypeNotFound,

    #[error("WebSocket rate limit (10429): System level frequency protection")]
    WsRateLimit,

    #[error("WebSocket too many sessions (10003): Too many sessions under same UID")]
    WsTooManySessions,

    #[error("WebSocket internal error (10016): Server restarting")]
    WsInternalError,

    #[error("WebSocket service restarting (10019)")]
    WsServiceRestarting,

    #[error("WebSocket too frequent requests (20003): Under same session")]
    WsTooFrequentRequests,

    #[error("WebSocket duplicate reqId (20006)")]
    WsDuplicateReqId,

    #[error("WebSocket IP rate limit exceeded (10403): 3000 requests per second per IP")]
    WsIpRateLimitExceeded,

    #[error("Unknown error: retCode={ret_code}, retMsg={ret_msg}")]
    Unknown { ret_code: i32, ret_msg: SmolStr },
}

impl BybitError {
    pub fn from_ret_code(ret_code: i32, ret_msg: &str) -> Self {
        match ret_code {
            -2015 => BybitError::ApiKeyExpiredSpot,
            -1 => BybitError::RequestExpired,
            0 => BybitError::Unknown {
                ret_code: 0,
                ret_msg: SmolStr::new(ret_msg),
            },
            10000 => BybitError::ServerTimeout,
            10001 => BybitError::RequestParameterError,
            10002 => BybitError::RequestTimeExceeded,
            10003 => BybitError::ApiKeyInvalid,
            10004 => BybitError::SignatureError,
            10005 => BybitError::PermissionDenied,
            10006 => BybitError::RateLimitExceeded,
            10007 => BybitError::AuthenticationFailed,
            10008 => BybitError::AccountBanned,
            10009 => BybitError::IpBanned,
            10010 => BybitError::IpMismatch,
            10014 => BybitError::DuplicateRequest,
            10016 => BybitError::ServerError,
            10017 => BybitError::RouteNotFound,
            110001 => BybitError::OrderNotFound,
            110003 => BybitError::OrderPriceExceededRange,
            110004 => BybitError::WalletBalanceInsufficient,
            110005 => BybitError::PositionStatusError,
            110007 => BybitError::AvailableBalanceInsufficient,
            110008 => BybitError::OrderCompletedOrCancelled,
            110009 => BybitError::TooManyStopOrders,
            110010 => BybitError::OrderCancelled,
            110013 => BybitError::CannotSetLeverageRiskLimit,
            110020 => BybitError::TooManyActiveOrders,
            110024 => BybitError::CannotSwitchPositionMode,
            110025 => BybitError::PositionModeUnchanged,
            110043 => BybitError::LeverageUnchanged,
            10403 => BybitError::WsIpRateLimitExceeded,
            10404 => BybitError::WsOpTypeNotFound,
            10429 => BybitError::WsRateLimit,
            20003 => BybitError::WsTooFrequentRequests,
            20006 => BybitError::WsDuplicateReqId,
            33004 => BybitError::ApiKeyExpiredDerivatives,
            _ => BybitError::Unknown {
                ret_code,
                ret_msg: SmolStr::new(ret_msg),
            },
        }
    }

    pub fn from_http_status(status: u16) -> Option<Self> {
        match status {
            400 => Some(BybitError::BadRequest(SmolStr::new("Bad request"))),
            401 => Some(BybitError::Unauthorized),
            403 => Some(BybitError::Forbidden),
            404 => Some(BybitError::NotFound),
            429 => Some(BybitError::TooManyRequests),
            _ => None,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BybitError::TooManyRequests
                | BybitError::RateLimitExceeded
                | BybitError::ServerError
                | BybitError::ServerTimeout
                | BybitError::HighServerLoad
                | BybitError::WsRateLimit
                | BybitError::WsIpRateLimitExceeded
                | BybitError::WsTooFrequentRequests
                | BybitError::WsInternalError
                | BybitError::WsServiceRestarting
        )
    }

    pub fn is_auth_error(&self) -> bool {
        matches!(
            self,
            BybitError::Unauthorized
                | BybitError::ApiKeyInvalid
                | BybitError::SignatureError
                | BybitError::AuthenticationFailed
                | BybitError::ApiKeyExpiredSpot
                | BybitError::ApiKeyExpiredDerivatives
        )
    }

    pub fn is_order_error(&self) -> bool {
        matches!(
            self,
            BybitError::OrderNotFound
                | BybitError::OrderPriceExceededRange
                | BybitError::OrderCompletedOrCancelled
                | BybitError::OrderCancelled
                | BybitError::TooManyActiveOrders
        )
    }
}

impl From<BybitError> for EngineError {
    fn from(err: BybitError) -> Self {
        match &err {
            BybitError::OrderNotFound => {
                EngineError::OrderNotFound(smol_str::format_smolstr!("{err}"))
            }
            BybitError::OrderCompletedOrCancelled => {
                EngineError::OrderAlreadyFilled(smol_str::format_smolstr!("{err}"))
            }
            BybitError::OrderCancelled => {
                EngineError::OrderAlreadyCanceled(smol_str::format_smolstr!("{err}"))
            }
            BybitError::Unauthorized
            | BybitError::ApiKeyInvalid
            | BybitError::SignatureError
            | BybitError::AuthenticationFailed => {
                EngineError::AuthenticationError(smol_str::format_smolstr!("{err}"))
            }
            BybitError::TooManyRequests
            | BybitError::RateLimitExceeded
            | BybitError::WsRateLimit
            | BybitError::WsIpRateLimitExceeded => {
                EngineError::RateLimitError(smol_str::format_smolstr!("{err}"))
            }
            _ => EngineError::CustomError(smol_str::format_smolstr!("{err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_ret_code() {
        assert_eq!(
            BybitError::from_ret_code(10001, "test"),
            BybitError::RequestParameterError
        );
        assert_eq!(
            BybitError::from_ret_code(110001, "test"),
            BybitError::OrderNotFound
        );
        assert_eq!(
            BybitError::from_ret_code(110025, "Position mode is not modified"),
            BybitError::PositionModeUnchanged
        );
        assert_eq!(
            BybitError::from_ret_code(110043, "leverage not modified"),
            BybitError::LeverageUnchanged
        );
        assert_eq!(
            BybitError::from_ret_code(99999, "unknown"),
            BybitError::Unknown {
                ret_code: 99999,
                ret_msg: SmolStr::new("unknown")
            }
        );
    }

    #[test]
    fn test_from_http_status() {
        assert_eq!(
            BybitError::from_http_status(401),
            Some(BybitError::Unauthorized)
        );
        assert_eq!(BybitError::from_http_status(500), None);
    }

    #[test]
    fn test_is_retryable() {
        assert!(BybitError::TooManyRequests.is_retryable());
        assert!(BybitError::ServerError.is_retryable());
        assert!(!BybitError::OrderNotFound.is_retryable());
        assert!(!BybitError::Unauthorized.is_retryable());
    }

    #[test]
    fn test_is_auth_error() {
        assert!(BybitError::Unauthorized.is_auth_error());
        assert!(BybitError::ApiKeyInvalid.is_auth_error());
        assert!(!BybitError::OrderNotFound.is_auth_error());
    }

    #[test]
    fn test_is_order_error() {
        assert!(BybitError::OrderNotFound.is_order_error());
        assert!(BybitError::OrderCancelled.is_order_error());
        assert!(!BybitError::Unauthorized.is_order_error());
    }

    #[test]
    fn test_to_engine_error() {
        let err: EngineError = BybitError::OrderNotFound.into();
        match err {
            EngineError::OrderNotFound(_) => {}
            _ => panic!("Expected OrderNotFound"),
        }

        let err: EngineError = BybitError::Unauthorized.into();
        match err {
            EngineError::AuthenticationError(_) => {}
            _ => panic!("Expected AuthenticationError"),
        }
    }
}
