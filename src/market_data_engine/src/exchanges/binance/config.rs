use serde::{Deserialize, Serialize};
use trading_core::types::common::Pair;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceConfig {
    pub pairs: Vec<Pair>,
    pub limit: u64,
    pub base_delay: Option<u64>,
    pub max_delay: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_serialize_binance_config() {
        let config = BinanceConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            limit: 100,
            base_delay: Some(2),
            max_delay: Some(60),
        };

        let json = serde_json::to_string(&config).unwrap();
        let expected_json = r#"{"pairs":[{"base":"BTC","quote":"USDT"}],"limit":100,"base_delay":2,"max_delay":60}"#;
        assert_eq!(json, expected_json);
    }

    #[test]
    fn test_deserialize_binance_config() {
        let json = r#"
        {
            "pairs": [{ "base": "ETH", "quote": "USDT" }],
            "limit": 500,
            "base_delay": 5,
            "max_delay": 30
        }
        "#;

        let config: BinanceConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.pairs.len(), 1);
        assert_eq!(config.pairs[0].base, "ETH");
        assert_eq!(config.pairs[0].quote, "USDT");
        assert_eq!(config.limit, 500);
        assert_eq!(config.base_delay, Some(5));
        assert_eq!(config.max_delay, Some(30));
    }

    #[test]
    fn test_deserialize_binance_config_missing_optional_fields() {
        let json = r#"
        {
            "pairs": [{ "base": "SOL", "quote": "USDT" }],
            "limit": 100
        }
        "#;

        let config: BinanceConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.pairs.len(), 1);
        assert_eq!(config.pairs[0].base, "SOL");
        assert_eq!(config.pairs[0].quote, "USDT");
        assert_eq!(config.limit, 100);
        assert_eq!(config.base_delay, None);
        assert_eq!(config.max_delay, None);
    }
}
