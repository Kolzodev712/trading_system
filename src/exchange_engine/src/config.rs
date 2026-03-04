use trading_core::{error::EngineError, types::common::Exchange};

use super::exchanges::binance::config::BinanceConfig;
use super::exchanges::bybit::config::BybitConfig;
use super::exchanges::k_paper::config::PaperConfig;
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
    Bybit(BybitConfig),
    Paper(PaperConfig),
}

impl ConfigGetter<BinanceConfig> for ExchangeConfig {
    fn get_config(self) -> Result<BinanceConfig, EngineError> {
        match self {
            ExchangeConfig::Binance(config) => Ok(config),
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Expected Binance config"
            ))),
        }
    }
}

impl ConfigGetter<BybitConfig> for ExchangeConfig {
    fn get_config(self) -> Result<BybitConfig, EngineError> {
        match self {
            ExchangeConfig::Bybit(config) => Ok(config),
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Expected Bybit config"
            ))),
        }
    }
}

impl ConfigGetter<PaperConfig> for ExchangeConfig {
    fn get_config(self) -> Result<PaperConfig, EngineError> {
        match self {
            ExchangeConfig::Paper(config) => Ok(config),
            _ => Err(EngineError::CustomError(anyhow::anyhow!(
                "Expected Paper config"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchanges::binance::config::BinanceConfig;
    use crate::exchanges::bybit::config::{BybitConfig, BybitCredentials, BybitRateLimiterConfig};
    use serde_json;
    use trading_core::secret::SecretString;
    use trading_core::types::common::{InstrumentType, Pair};

    fn create_test_pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    // Test that ExchangeEngineConfig can be serialized and deserialized for any exchange
    #[test]
    fn test_exchange_engine_config_roundtrip_binance() {
        let binance_config = BinanceConfig {
            pairs: vec![create_test_pair()],
            api_key: SecretString::from("test_key"),
            secret: SecretString::from("test_secret"),
            limit: Some(100),
            base_delay: Some(2),
            max_delay: Some(60),
        };

        let original = ExchangeEngineConfig {
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(binance_config),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ExchangeEngineConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(original.exchange, deserialized.exchange);
        assert!(matches!(
            deserialized.exchange_config,
            ExchangeConfig::Binance(_)
        ));
    }

    #[test]
    fn test_exchange_engine_config_roundtrip_bybit() {
        let bybit_config = BybitConfig {
            pairs: vec![create_test_pair()],
            instrument_type: InstrumentType::Spot,
            credentials: BybitCredentials {
                api_key: SecretString::from("test_key"),
                api_secret: SecretString::from("test_secret"),
            },
            testnet: false,
            rate_limiter: BybitRateLimiterConfig::default(),
        };

        let original = ExchangeEngineConfig {
            exchange: Exchange::Bybit,
            exchange_config: ExchangeConfig::Bybit(bybit_config),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ExchangeEngineConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(original.exchange, deserialized.exchange);
        assert!(matches!(
            deserialized.exchange_config,
            ExchangeConfig::Bybit(_)
        ));
    }

    // Test that exchange field matches the config variant
    #[test]
    fn test_exchange_field_matches_config_binance() {
        let config = ExchangeEngineConfig {
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(BinanceConfig {
                pairs: vec![create_test_pair()],
                api_key: SecretString::from("key"),
                secret: SecretString::from("secret"),
                limit: None,
                base_delay: None,
                max_delay: None,
            }),
        };

        assert_eq!(config.exchange, Exchange::Binance);
        assert!(matches!(config.exchange_config, ExchangeConfig::Binance(_)));
    }

    #[test]
    fn test_exchange_field_matches_config_bybit() {
        let config = ExchangeEngineConfig {
            exchange: Exchange::Bybit,
            exchange_config: ExchangeConfig::Bybit(BybitConfig {
                pairs: vec![create_test_pair()],
                instrument_type: InstrumentType::Spot,
                credentials: BybitCredentials {
                    api_key: SecretString::from("key"),
                    api_secret: SecretString::from("secret"),
                },
                testnet: false,
                rate_limiter: BybitRateLimiterConfig::default(),
            }),
        };

        assert_eq!(config.exchange, Exchange::Bybit);
        assert!(matches!(config.exchange_config, ExchangeConfig::Bybit(_)));
    }

    // Test ConfigGetter trait for both exchanges
    #[test]
    fn test_get_exchange_config_binance() {
        let binance_config = BinanceConfig {
            pairs: vec![create_test_pair()],
            api_key: SecretString::from("test_key"),
            secret: SecretString::from("test_secret"),
            limit: Some(100),
            base_delay: Some(2),
            max_delay: Some(60),
        };

        let exchange_engine_config = ExchangeEngineConfig {
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(binance_config.clone()),
        };

        let retrieved: BinanceConfig = exchange_engine_config.get_exchange_config().unwrap();
        assert_eq!(retrieved.pairs, binance_config.pairs);
        assert_eq!(retrieved.limit, binance_config.limit);
    }

    #[test]
    fn test_get_exchange_config_bybit() {
        let bybit_config = BybitConfig {
            pairs: vec![create_test_pair()],
            instrument_type: InstrumentType::Futures,
            credentials: BybitCredentials {
                api_key: SecretString::from("test_key"),
                api_secret: SecretString::from("test_secret"),
            },
            testnet: true,
            rate_limiter: BybitRateLimiterConfig::default(),
        };

        let exchange_engine_config = ExchangeEngineConfig {
            exchange: Exchange::Bybit,
            exchange_config: ExchangeConfig::Bybit(bybit_config.clone()),
        };

        let retrieved: BybitConfig = exchange_engine_config.get_exchange_config().unwrap();
        assert_eq!(retrieved.pairs, bybit_config.pairs);
        assert_eq!(retrieved.instrument_type, bybit_config.instrument_type);
        assert_eq!(retrieved.testnet, bybit_config.testnet);
    }

    // Test error handling when requesting wrong exchange type
    #[test]
    fn test_get_exchange_config_wrong_type() {
        let exchange_engine_config = ExchangeEngineConfig {
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(BinanceConfig {
                pairs: vec![create_test_pair()],
                api_key: SecretString::from("key"),
                secret: SecretString::from("secret"),
                limit: None,
                base_delay: None,
                max_delay: None,
            }),
        };

        let result: Result<BybitConfig, _> = exchange_engine_config.clone().get_exchange_config();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Expected Bybit config"));
    }
}
