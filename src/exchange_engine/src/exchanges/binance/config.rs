use crate::exchanges::binance::common_types::BinanceSymbol;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceConfig {
    pub symbol: BinanceSymbol,
    pub rest: BinanceCredentials,
    pub ws: BinanceCredentials,
    pub limit: Option<u64>,
    pub base_delay: Option<u64>,
    pub max_delay: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCredentials {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub api_key: SmolStr,

    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub secret_key: SmolStr,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_binance_credentials_serde() {
        let creds = BinanceCredentials {
            api_key: SmolStr::new("test_api_key"),
            secret_key: SmolStr::new("test_secret_key"),
        };

        let serialized = serde_json::to_string(&creds).expect("Serialization failed");
        assert!(serialized.contains("test_api_key"));
        assert!(serialized.contains("test_secret_key"));

        let deserialized: BinanceCredentials =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.api_key, creds.api_key);
        assert_eq!(deserialized.secret_key, creds.secret_key);
    }

    #[test]
    fn test_binance_config_serde() {
        let config = BinanceConfig {
            symbol: BinanceSymbol::new("BTC", "USDT"),
            rest: BinanceCredentials {
                api_key: SmolStr::new("rest_api_key"),
                secret_key: SmolStr::new("rest_secret_key"),
            },
            ws: BinanceCredentials {
                api_key: SmolStr::new("ws_api_key"),
                secret_key: SmolStr::new("ws_secret_key"),
            },
            limit: Some(500),
            base_delay: Some(1000),
            max_delay: Some(60000),
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
        assert!(serialized.contains("rest_api_key"));
        assert!(serialized.contains("ws_api_key"));
        assert!(serialized.contains("500"));
        assert!(serialized.contains("1000"));
        assert!(serialized.contains("60000"));

        let deserialized: BinanceConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_binance_config_optional_fields_none() {
        let config = BinanceConfig {
            symbol: BinanceSymbol::new("ETH", "USDT"),
            rest: BinanceCredentials {
                api_key: SmolStr::new("rest_api_key"),
                secret_key: SmolStr::new("rest_secret_key"),
            },
            ws: BinanceCredentials {
                api_key: SmolStr::new("ws_api_key"),
                secret_key: SmolStr::new("ws_secret_key"),
            },
            limit: None,
            base_delay: None,
            max_delay: None,
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");

        let deserialized: BinanceConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized, config);
    }
}
