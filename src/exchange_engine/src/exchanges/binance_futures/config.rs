//! Binance Futures Configuration
//!
//! Configuration structure for Binance USD Futures integration.
//! Mirrors the Binance Spot configuration but may include futures-specific options.

use serde::{Deserialize, Serialize};
use trading_core::secret::SecretString;
use trading_core::types::common::Pair;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesConfig {
    /// Trading pairs to use (centralized pair configuration)
    pub pairs: Vec<Pair>,

    /// REST API credentials
    pub rest: BinanceFuturesCredentials,

    /// WebSocket credentials (may be same as REST)
    pub ws: BinanceFuturesCredentials,

    /// Orderbook depth limit
    pub limit: Option<u64>,

    /// Base delay for rate limiting (milliseconds)
    pub base_delay: Option<u64>,

    /// Maximum delay for rate limiting (milliseconds)
    pub max_delay: Option<u64>,

    /// Testnet mode
    #[serde(default)]
    pub testnet: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesCredentials {
    pub api_key: SecretString,
    pub secret_key: SecretString,
}
