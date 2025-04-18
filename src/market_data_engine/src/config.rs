use core_logic::{error::EngineError, Exchange};

use super::exchanges::binance::config::BinanceConfig;
use serde::{Deserialize, Serialize};

pub trait ConfigGetter<T> {
    fn get_config(self) -> Result<T, EngineError>;
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct MarketDataConfig {
    pub redis_url: String,
    pub exchange: Exchange,
    pub exchange_config: ExchangeConfig,
}

impl MarketDataConfig {
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
    use super::*;
    use core_logic::{data::base_data::Pair, error::EngineError, Exchange};
    use serde_json;

    // Test for deserialization of MarketDataConfig
    #[test]
    fn test_market_data_config_deserialization() {
        let json = r#"
        {
            "redis_url": "redis://localhost:6379",
            "exchange": "Binance",
            "exchange_config": {
                "Binance": {
                    "symbol": {
                        "base": "BTC",
                        "quote": "USDT"
                    },
                    "limit": 100,
                    "baseDelay": 2,
                    "maxDelay": 60
                }
            }
        }
        "#;

        let config: MarketDataConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.redis_url, "redis://localhost:6379");
        assert_eq!(config.exchange, Exchange::Binance);
        assert_eq!(
            config.exchange_config,
            ExchangeConfig::Binance(BinanceConfig {
                symbol: Pair {
                    base: "BTC".into(),
                    quote: "USDT".into()
                }
                .into(),
                limit: 100,
                base_delay: None,
                max_delay: None
            })
        );
    }

    // Test for successful retrieval of exchange config
    #[test]
    fn test_get_exchange_config_success() {
        let config = MarketDataConfig {
            redis_url: "redis://localhost:6379".into(),
            exchange: Exchange::Binance,
            exchange_config: ExchangeConfig::Binance(BinanceConfig {
                symbol: Pair {
                    base: "BTC".into(),
                    quote: "USDT".into(),
                }
                .into(),
                limit: 100,
                base_delay: Some(2),
                max_delay: Some(60),
            }),
        };

        let result: Result<BinanceConfig, EngineError> = config.get_exchange_config();
        assert!(result.is_ok());
        let binance_config = result.unwrap();

        assert_eq!(binance_config.symbol.base, "BTC");
        assert_eq!(binance_config.symbol.quote, "USDT");
        assert_eq!(binance_config.limit, 100);
        assert_eq!(binance_config.base_delay, Some(2));
        assert_eq!(binance_config.max_delay, Some(60));
    }
}
