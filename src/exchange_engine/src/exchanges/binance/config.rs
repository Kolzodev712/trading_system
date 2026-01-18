use serde::{Deserialize, Serialize};
use trading_core::secret::SecretString;
use trading_core::types::common::Pair;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceConfig {
    /// Trading pairs to use (centralized pair configuration)
    pub pairs: Vec<Pair>,
    pub api_key: SecretString,
    pub secret: SecretString,
    pub limit: Option<u64>,
    pub base_delay: Option<u64>,
    pub max_delay: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use trading_core::types::common::Pair;

    #[test]
    fn test_binance_config_serde() {
        let config = BinanceConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            api_key: SecretString::new("test_api_key"),
            secret: SecretString::new("test_secret_key"),
            limit: Some(500),
            base_delay: Some(1000),
            max_delay: Some(60000),
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
        assert!(serialized.contains("test_api_key"));
        assert!(serialized.contains("test_secret_key"));
        assert!(serialized.contains("500"));
        assert!(serialized.contains("1000"));
        assert!(serialized.contains("60000"));

        let deserialized: BinanceConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.api_key.expose(), config.api_key.expose());
        assert_eq!(deserialized.secret.expose(), config.secret.expose());
        assert_eq!(deserialized.pairs, config.pairs);
        assert_eq!(deserialized.limit, config.limit);
    }

    #[test]
    fn test_binance_config_optional_fields_none() {
        let config = BinanceConfig {
            pairs: vec![Pair::new("ETH", "USDT")],
            api_key: SecretString::new("test_api_key"),
            secret: SecretString::new("test_secret_key"),
            limit: None,
            base_delay: None,
            max_delay: None,
        };

        let serialized = serde_json::to_string(&config).expect("Serialization failed");

        let deserialized: BinanceConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.api_key.expose(), config.api_key.expose());
        assert_eq!(deserialized.secret.expose(), config.secret.expose());
        assert_eq!(deserialized.pairs, config.pairs);
    }
}
