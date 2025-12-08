use super::{config::MarketDataConfig, messages::ChannelMessageHandler};
use crate::{config::ExchangeConfig, exchanges::binance::client::BinanceClient};
use tracing::error;
use trading_core::{
    channels::market_data::MarketDataUpdateSender, error::EngineError, traits::MarketDataClient,
};

#[derive(Debug, Clone)]
pub enum Method {
    Orderbooks,
    Trades,
}

pub struct MarketDataDriver {
    exchange_client: Box<dyn MarketDataClient>,
}

impl MarketDataDriver {
    /// Create a new MarketDataDriver with channel-based message handling
    ///
    /// # Arguments
    /// * `config` - Market data configuration
    /// * `market_data_tx` - Channel sender for publishing market data updates
    pub fn new(
        config: MarketDataConfig,
        market_data_tx: MarketDataUpdateSender,
    ) -> Result<Self, EngineError> {
        let exchange_client = Self::create_exchange_client(&config, market_data_tx)?;

        Ok(Self { exchange_client })
    }

    pub fn create_exchange_client(
        config: &MarketDataConfig,
        market_data_tx: MarketDataUpdateSender,
    ) -> Result<Box<dyn MarketDataClient>, EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let message_handler = Box::new(ChannelMessageHandler::new(market_data_tx));
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
                    error!("Error in fetching orderbooks: {e}");
                }
            }
            Method::Trades => {
                if let Err(e) = self.exchange_client.trades().await {
                    error!("Error in fetching trades: {e}");
                }
            }
        }
        Ok(())
    }
}
