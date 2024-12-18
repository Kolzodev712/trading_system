use core_logic::{error::EngineError, Exchange};

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
