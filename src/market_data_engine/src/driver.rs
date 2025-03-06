use super::{config::MarketDataConfig, messages::RedisMessageHandler};
use crate::{config::ExchangeConfig, exchanges::binance::client::BinanceClient};
use core_logic::{error::EngineError, redis::RedisClient, traits::MarketDataClient};
use tracing::error;

#[derive(Debug, Clone)]
pub enum Method {
    Orderbooks,
    Trades,
}

pub struct MarketDataDriver {
    exchange_client: Box<dyn MarketDataClient>,
}

impl MarketDataDriver {
    pub fn new(config: MarketDataConfig) -> Result<Self, EngineError> {
        let redis_client = RedisClient::new(&config.redis_url)?;
        let exchange_client = Self::create_exchange_client(&config, redis_client.clone())?;

        Ok(Self { exchange_client })
    }

    pub fn create_exchange_client(
        config: &MarketDataConfig,
        redis_client: RedisClient,
    ) -> Result<Box<dyn MarketDataClient>, EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let message_handler = Box::new(RedisMessageHandler::new(redis_client));
                Ok(Box::new(BinanceClient::new(
                    binance_config.clone(),
                    100,
                    Some(message_handler),
                )))
            } // _ => Err(EngineError::UnsupportedExchange), => add this when more exchanges are supported
        }
    }

    pub async fn run(&mut self, method: Method) -> Result<(), EngineError> {
        match method {
            Method::Orderbooks => {
                if let Err(e) = self.exchange_client.orderbooks().await {
                    error!("Error in fetching orderbooks: {}", e);
                }
            }
            Method::Trades => {
                if let Err(e) = self.exchange_client.trades().await {
                    error!("Error in fetching trades: {}", e);
                }
            }
        }
        Ok(())
    }
}
