use crate::exchanges::binance::{client::BinanceClient, config::BinanceConfig};
use core_logic::{
    data::{
        base_data::Pair,
        public_data::{Orderbook, Trade},
    },
    error::EngineError,
    redis::{RedisClient, RedisMessage, ORDERBOOK_CHANNEL, TRADE_CHANNEL},
    traits::MarketDataClient,
    Exchange,
};

use super::config::MarketDataConfig;

use async_trait::async_trait;
use tracing::error;

#[derive(Debug, Clone)]
pub enum Method {
    Orderbooks,
    Trades,
}

#[derive(Debug, Clone)]
pub struct MarketDataDriver {
    config: MarketDataConfig,
    redis_client: RedisClient,
}

impl MarketDataDriver {
    pub fn new(config: MarketDataConfig) -> Self {
        let redis_client = RedisClient::new(&config.redis_url).unwrap();
        Self {
            config,
            redis_client,
        }
    }

    pub async fn run(&mut self, method: Method) -> Result<(), EngineError> {
        match method {
            Method::Orderbooks => {
                if let Err(e) = self.orderbooks().await {
                    error!("Error in fetching orderbooks: {}", e);
                }
            }
            Method::Trades => {
                if let Err(e) = self.trades().await {
                    error!("Error in fetching trades: {}", e);
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MarketDataClient for MarketDataDriver {
    const EXCHANGE_ID: Exchange = Exchange::Binance;

    /// Fetch and publish orderbooks
    async fn orderbooks(&mut self) -> Result<Vec<Orderbook>, EngineError> {
        let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
        let mut client = BinanceClient::new(binance_config, 100);
        let orderbooks = client.orderbook().await?;

        // Publish each orderbook to Redis
        for orderbook in &orderbooks {
            let message = RedisMessage::OrderbookMessage(orderbook.clone());
            self.redis_client
                .publish(ORDERBOOK_CHANNEL, message)
                .await?;
        }

        Ok(orderbooks)
    }

    /// Fetch and publish trades
    async fn trades(&mut self) -> Result<Vec<Trade>, EngineError> {
        let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
        let mut client = BinanceClient::new(binance_config, 100);
        let trades = client.trades().await?;

        // Publish each trade to Redis
        for trade in &trades {
            let message = RedisMessage::TradeMessage(trade.clone());
            self.redis_client.publish(TRADE_CHANNEL, message).await?;
        }

        Ok(trades)
    }

    async fn orderbook_snapshot(&self, _pair: Pair) -> Result<Orderbook, EngineError> {
        unimplemented!()
    }

    async fn trade_snapshot(&self, _pair: Pair) -> Result<Trade, EngineError> {
        unimplemented!()
    }
}
