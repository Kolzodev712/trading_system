use trading_core::{error::EngineError, types::common::Exchange};

use super::exchanges::binance::config::BinanceConfig;
use serde::{Deserialize, Serialize};

pub trait ConfigGetter<T> {
    fn get_config(self) -> Result<T, EngineError>;
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct ExchangeEngineConfig {
    pub exchange: Exchange,
    pub exchange_config: ExchangeConfig,
}

impl ExchangeEngineConfig {
    pub fn get_exchange_config<T>(self) -> Result<T, EngineError>
    where
        ExchangeConfig: ConfigGetter<T>,
    {
        self.exchange_config.get_config()
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ExchangeConfig {
    Binance(BinanceConfig),
}

impl ConfigGetter<BinanceConfig> for ExchangeConfig {
    fn get_config(self) -> Result<BinanceConfig, EngineError> {
        match self {
            ExchangeConfig::Binance(config) => Ok(config),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::exchanges::binance::config::BinanceCredentials;

    use super::*;
    use serde_json;

    // Sample config JSON for deserialization
    const CONFIG_JSON: &str = r#"
        {
        "exchange": "Binance",
        "exchange_config": {
            "Binance": {
            "pairs": [
                {
                    "base": "BTC",
                    "quote": "USDT"
                }
            ],
            "rest": {
                "apiKey": "some_key",
                "secretKey": "some_secret"    
            },
            "ws": {
                "apiKey": "some_key",
                "secretKey": "some_secret"    
            },
            "limit": 100,
            "baseDelay": 2,
            "maxDelay": 60
            }
        }
    }
    "#;

    // Test for deserializing the entire ExchangeEngineConfig
    #[test]
    fn test_exchange_engine_config_deserialization() {
        let config: ExchangeEngineConfig = serde_json::from_str(CONFIG_JSON).unwrap();

        assert_eq!(config.exchange, Exchange::Binance);

        match config.exchange_config {
            ExchangeConfig::Binance(ref binance_config) => {
                assert_eq!(binance_config.rest.api_key, "some_key".into());
                assert_eq!(binance_config.rest.secret_key, "some_secret".into());
                assert_eq!(binance_config.ws.api_key, "some_key".into());
                assert_eq!(binance_config.ws.secret_key, "some_secret".into());
                assert_eq!(binance_config.limit, Some(100));
                assert_eq!(binance_config.base_delay, Some(2));
                assert_eq!(binance_config.max_delay, Some(60));
            }
        }
    }

    // Test for serializing the ExchangeEngineConfig
    #[test]
    fn test_exchange_engine_config_serialization() {
        let binance_config = BinanceConfig {
            pairs: vec![trading_core::types::common::Pair::new("BTC", "USDT")],
            rest: BinanceCredentials {
                api_key: trading_core::secret::SecretString::from("some_key"),
                secret_key: trading_core::secret::SecretString::from("some_secret"),
            },
            ws: BinanceCredentials {
                api_key: trading_core::secret::SecretString::from("some_key"),
                secret_key: trading_core::secret::SecretString::from("some_secret"),
            },
            limit: Some(100),
            base_delay: Some(2),
            max_delay: Some(60),
        };

        let exchange_engine_config = ExchangeEngineConfig {
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(binance_config),
        };

        let json = serde_json::to_string(&exchange_engine_config).unwrap();
        assert!(json.contains("\"exchange\":\"Binance\""));
        assert!(json.contains("\"apiKey\":\"some_key\""));
        assert!(json.contains("\"pairs\":[{\"base\":\"BTC\",\"quote\":\"USDT\"}]"));
    }

    // Test for retrieving the BinanceConfig from the ExchangeEngineConfig
    #[test]
    fn test_get_exchange_config() {
        let binance_config = BinanceConfig {
            pairs: vec![trading_core::types::common::Pair::new("BTC", "USDT")],
            rest: BinanceCredentials {
                api_key: trading_core::secret::SecretString::from("some_key"),
                secret_key: trading_core::secret::SecretString::from("some_secret"),
            },
            ws: BinanceCredentials {
                api_key: trading_core::secret::SecretString::from("some_key"),
                secret_key: trading_core::secret::SecretString::from("some_secret"),
            },
            limit: Some(100),
            base_delay: Some(2),
            max_delay: Some(60),
        };

        let exchange_engine_config = ExchangeEngineConfig {
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(binance_config),
        };

        let retrieved_config: BinanceConfig = exchange_engine_config.get_exchange_config().unwrap();

        assert_eq!(retrieved_config.rest.api_key, "some_key".into());
        assert_eq!(retrieved_config.rest.secret_key, "some_secret".into());
        assert_eq!(retrieved_config.ws.api_key, "some_key".into());
        assert_eq!(retrieved_config.ws.secret_key, "some_secret".into());
        assert_eq!(retrieved_config.limit, Some(100));
        assert_eq!(retrieved_config.base_delay, Some(2));
        assert_eq!(retrieved_config.max_delay, Some(60));
    }
}
