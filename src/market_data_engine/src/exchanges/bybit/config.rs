use serde::{Deserialize, Serialize};
use trading_core::types::common::Pair;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BybitConfig {
    pub pairs: Vec<Pair>,
    pub limit: u64,
    pub base_delay: Option<u64>,
    pub max_delay: Option<u64>,
    pub category: BybitCategory,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BybitCategory {
    #[serde(rename = "spot")]
    Spot,
    #[serde(rename = "linear")]
    Linear,
    #[serde(rename = "inverse")]
    Inverse,
}

impl Default for BybitCategory {
    fn default() -> Self {
        BybitCategory::Spot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_bybit_config() {
        let config = BybitConfig {
            pairs: vec![Pair::new("BTC", "USDT")],
            limit: 1,
            base_delay: Some(2),
            max_delay: Some(60),
            category: BybitCategory::Spot,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("BTC"));
        assert!(json.contains("USDT"));
        assert!(json.contains("spot"));
    }

    #[test]
    fn test_deserialize_bybit_config() {
        let json = r#"
        {
            "pairs": [{ "base": "ETH", "quote": "USDT" }],
            "limit": 1,
            "base_delay": 5,
            "max_delay": 30,
            "category": "linear"
        }
        "#;

        let config: BybitConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.pairs.len(), 1);
        assert_eq!(config.pairs[0].base, "ETH");
        assert_eq!(config.pairs[0].quote, "USDT");
        assert_eq!(config.limit, 1);
        assert_eq!(config.base_delay, Some(5));
        assert_eq!(config.max_delay, Some(30));
        match config.category {
            BybitCategory::Linear => {}
            _ => panic!("Expected Linear category"),
        }
    }

    #[test]
    fn test_deserialize_bybit_config_inverse() {
        let json = r#"
        {
            "pairs": [{ "base": "BTC", "quote": "USD" }],
            "limit": 1,
            "base_delay": 2,
            "max_delay": 60,
            "category": "inverse"
        }
        "#;

        let config: BybitConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.pairs.len(), 1);
        assert_eq!(config.pairs[0].base, "BTC");
        assert_eq!(config.pairs[0].quote, "USD");
        match config.category {
            BybitCategory::Inverse => {}
            _ => panic!("Expected Inverse category"),
        }
    }
}
