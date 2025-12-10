use super::{config::MarketDataConfig, messages::StateManagerMessageHandler};
use crate::{
    config::ExchangeConfig, exchanges::binance::client::BinanceClient,
    state::manager::MarketDataStateManager,
};
use std::sync::Arc;
use tracing::error;
use trading_core::config::ChannelConfig;
use trading_core::{
    channels::market_data::MarketDataUpdateSender, error::EngineError,
    registry::symbol::SymbolRegistry, traits::MarketDataClient, types::common::Pair,
};

#[derive(Debug, Clone)]
pub enum Method {
    Orderbooks,
    Trades,
}

pub struct MarketDataDriver {
    exchange_client: Box<dyn MarketDataClient>,
    state_manager: Arc<MarketDataStateManager>,
}

impl MarketDataDriver {
    /// Create a new MarketDataDriver with state manager-based message handling
    ///
    /// # Arguments
    /// * `config` - Market data configuration
    /// * `market_data_tx` - Channel sender for publishing market data updates
    /// * `channels` - Channel configuration for buffer sizes
    ///
    /// The driver creates its own symbol registry from the config's pair information.
    pub fn new(
        config: MarketDataConfig,
        market_data_tx: MarketDataUpdateSender,
        channels: ChannelConfig,
    ) -> Result<Self, EngineError> {
        // Extract pair from config and create symbol registry
        let pair = match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => Pair::new(
                binance_config.symbol.base.as_str(),
                binance_config.symbol.quote.as_str(),
            ),
        };
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair]));

        // Create state manager with symbol registry
        let state_manager = Arc::new(MarketDataStateManager::new(
            market_data_tx.clone(),
            symbol_registry,
        ));

        // Create exchange client with state manager handler
        let exchange_client =
            Self::create_exchange_client(&config, state_manager.clone(), &channels)?;

        Ok(Self {
            exchange_client,
            state_manager,
        })
    }

    /// Get the state manager
    pub fn get_state_manager(&self) -> Arc<MarketDataStateManager> {
        self.state_manager.clone()
    }

    fn create_exchange_client(
        config: &MarketDataConfig,
        state_manager: Arc<MarketDataStateManager>,
        channels: &ChannelConfig,
    ) -> Result<Box<dyn MarketDataClient>, EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let message_handler = Box::new(StateManagerMessageHandler::new(state_manager));
                Ok(Box::new(BinanceClient::new(
                    binance_config.clone(),
                    channels.market_data_engine_buffer,
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
