//! Bybit exchange configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use trading_core::secret::SecretString;
use trading_core::types::common::{InstrumentType, Pair};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BybitConfig {
    /// Trading pairs to use (centralized pair configuration)
    pub pairs: Vec<Pair>,
    /// Instrument type for all pairs (Spot or Futures)
    /// Defaults to Spot if not specified
    #[serde(default)]
    pub instrument_type: InstrumentType,
    /// API credentials (shared for both spot and futures)
    pub credentials: BybitCredentials,
    /// Classic testnet (`api-testnet.bybit.com`). Mutually exclusive with `demo`.
    #[serde(default)]
    pub testnet: bool,
    /// Mainnet demo trading (`api-demo.bybit.com`). Requires a demo API key.
    #[serde(default)]
    pub demo: bool,
    /// Optional linear leverage applied at bootstrap (one-way). Ignored for spot.
    #[serde(default)]
    pub leverage: Option<u32>,
    /// Rate limiter configuration
    #[serde(default)]
    pub rate_limiter: BybitRateLimiterConfig,
}

const BYBIT_REST_MAINNET: &str = "https://api.bybit.com/v5/";
const BYBIT_REST_TESTNET: &str = "https://api-testnet.bybit.com/v5/";
const BYBIT_REST_DEMO: &str = "https://api-demo.bybit.com/v5/";
const BYBIT_WS_PRIVATE_MAINNET: &str = "wss://stream.bybit.com/v5/private";
const BYBIT_WS_PRIVATE_TESTNET: &str = "wss://stream-testnet.bybit.com/v5/private";
const BYBIT_WS_PRIVATE_DEMO: &str = "wss://stream-demo.bybit.com/v5/private";
const BYBIT_WS_TRADE_MAINNET: &str = "wss://stream.bybit.com/v5/trade";
const BYBIT_WS_TRADE_TESTNET: &str = "wss://stream-testnet.bybit.com/v5/trade";

impl BybitConfig {
    /// REST host for this environment. Demo wins over testnet if both are set.
    pub fn rest_base_url(&self) -> &'static str {
        if self.demo {
            BYBIT_REST_DEMO
        } else if self.testnet {
            BYBIT_REST_TESTNET
        } else {
            BYBIT_REST_MAINNET
        }
    }

    /// Private account stream. Public market data stays on mainnet when `demo` is set.
    pub fn private_ws_url(&self) -> &'static str {
        if self.demo {
            BYBIT_WS_PRIVATE_DEMO
        } else if self.testnet {
            BYBIT_WS_PRIVATE_TESTNET
        } else {
            BYBIT_WS_PRIVATE_MAINNET
        }
    }

    /// WS Trade host. `None` on demo — Bybit does not support that stream there.
    pub fn trade_ws_url(&self) -> Option<&'static str> {
        if self.demo {
            None
        } else if self.testnet {
            Some(BYBIT_WS_TRADE_TESTNET)
        } else {
            Some(BYBIT_WS_TRADE_MAINNET)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitRateLimiterConfig {
    #[serde(default)]
    pub endpoint_limits: HashMap<String, u64>,

    #[serde(default = "default_endpoint_limit")]
    pub default_endpoint_limit: u64,

    #[serde(default = "default_ip_limit")]
    pub ip_limit: u64,

    #[serde(default = "default_ip_window_seconds")]
    pub ip_window_seconds: u64,
}

fn default_endpoint_limit() -> u64 {
    10
}

fn default_ip_limit() -> u64 {
    600
}

fn default_ip_window_seconds() -> u64 {
    5
}

impl Default for BybitRateLimiterConfig {
    fn default() -> Self {
        Self {
            endpoint_limits: HashMap::new(),
            default_endpoint_limit: default_endpoint_limit(),
            ip_limit: default_ip_limit(),
            ip_window_seconds: default_ip_window_seconds(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BybitCredentials {
    pub api_key: SecretString,
    pub api_secret: SecretString,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_bybit_credentials_serde() {
        let creds = BybitCredentials {
            api_key: SecretString::new("test_api_key"),
            api_secret: SecretString::new("test_api_secret"),
        };

        let serialized = serde_json::to_string(&creds).expect("Serialization failed");
        assert!(serialized.contains("test_api_key"));
        assert!(serialized.contains("test_api_secret"));

        let deserialized: BybitCredentials =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.api_key.expose(), creds.api_key.expose());
        assert_eq!(deserialized.api_secret.expose(), creds.api_secret.expose());
    }

    #[test]
    fn test_bybit_config_serde() {
        let config = BybitConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            instrument_type: InstrumentType::Spot,
            credentials: BybitCredentials {
                api_key: SecretString::new("api_key"),
                api_secret: SecretString::new("api_secret"),
            },
            testnet: false,
            demo: false,
            leverage: None,
            rate_limiter: BybitRateLimiterConfig::default(),
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));

        let deserialized: BybitConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(
            deserialized.credentials.api_key.expose(),
            config.credentials.api_key.expose()
        );
        assert_eq!(deserialized.pairs, config.pairs);
        assert_eq!(deserialized.testnet, config.testnet);
        assert!(!deserialized.demo);
    }

    #[test]
    fn demo_urls_win_over_testnet() {
        let mut config = BybitConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            instrument_type: InstrumentType::Futures,
            credentials: BybitCredentials {
                api_key: SecretString::new("api_key"),
                api_secret: SecretString::new("api_secret"),
            },
            testnet: true,
            demo: true,
            leverage: None,
            rate_limiter: BybitRateLimiterConfig::default(),
        };
        assert_eq!(config.rest_base_url(), "https://api-demo.bybit.com/v5/");
        assert_eq!(
            config.private_ws_url(),
            "wss://stream-demo.bybit.com/v5/private"
        );
        assert!(config.trade_ws_url().is_none());

        config.demo = false;
        assert_eq!(config.rest_base_url(), "https://api-testnet.bybit.com/v5/");
        assert_eq!(
            config.trade_ws_url(),
            Some("wss://stream-testnet.bybit.com/v5/trade")
        );
    }
}
