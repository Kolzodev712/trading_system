//! Generic builders for external exchange clients and drivers
//!
//! These builders are exchange-agnostic and provide a unified interface
//! for creating exchange clients for testing external connections.

use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{
    channels::exchange::exchange_update_channel,
    config::ChannelConfig,
    registry::symbol::SymbolRegistry,
    secret::SecretString,
    traits::TradingClient,
    types::{
        common::{Exchange, Pair},
        exchange::OrderRequest,
    },
};

use exchange_engine::{
    exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient},
    state::manager::ExchangeStateManager,
};

/// Exchange-agnostic configuration for testing
#[derive(Debug, Clone)]
pub struct TestExchangeConfig {
    pub exchange: Exchange,
    pub pairs: Vec<Pair>,
    pub api_key: String,
    pub secret_key: String,
}

impl Default for TestExchangeConfig {
    fn default() -> Self {
        Self {
            exchange: Exchange::Binance,
            pairs: vec![Pair::new("BTC", "USDT")],
            api_key: "test_key".to_string(),
            secret_key: "test_secret".to_string(),
        }
    }
}

/// Builder for exchange clients (exchange-agnostic)
pub struct ExchangeClientBuilder {
    config: TestExchangeConfig,
    channels: ChannelConfig,
}

impl ExchangeClientBuilder {
    pub fn new(config: TestExchangeConfig) -> Self {
        Self {
            config,
            channels: ChannelConfig::default(),
        }
    }

    pub fn with_channels(mut self, channels: ChannelConfig) -> Self {
        self.channels = channels;
        self
    }

    /// Build a trading client for the configured exchange
    pub fn build_trading_client(&self) -> Arc<RwLock<dyn TradingClient<OrderRequest>>> {
        match self.config.exchange {
            Exchange::Binance => {
                let binance_config = self.create_binance_config();
                Arc::new(RwLock::new(BinanceClient::new(
                    binance_config,
                    self.channels.websocket_inbound_capacity,
                    self.channels.websocket_ping_capacity,
                    self.channels.websocket_outbound_rpc_capacity,
                    self.channels.shutdown_capacity,
                )))
            }
            Exchange::BinanceFutures | Exchange::Bybit => {
                todo!("Futures exchange clients not yet implemented in tests")
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
        }
    }

    /// Build an exchange state manager with the configured client
    pub async fn build_state_manager(
        &self,
    ) -> (
        ExchangeStateManager,
        trading_core::channels::exchange::ExchangeUpdateReceiver,
    ) {
        let (tx, rx) = exchange_update_channel(self.channels.exchange_update_capacity);
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(self.config.pairs.clone()));
        let trading_client = self.build_trading_client();

        let manager = ExchangeStateManager::new(
            tx,
            symbol_registry,
            trading_client,
            0, // Disabled reconciliation for tests
            false,
        );

        (manager, rx)
    }

    fn create_binance_config(&self) -> BinanceConfig {
        use exchange_engine::exchanges::binance::config::BinanceCredentials;

        BinanceConfig {
            pairs: self.config.pairs.clone(),
            rest: BinanceCredentials {
                api_key: SecretString::new(self.config.api_key.clone()),
                secret_key: SecretString::new(self.config.secret_key.clone()),
            },
            ws: BinanceCredentials {
                api_key: SecretString::new(self.config.api_key.clone()),
                secret_key: SecretString::new(self.config.secret_key.clone()),
            },
            limit: None,
            base_delay: None,
            max_delay: None,
        }
    }
}

/// Builder for market data clients (exchange-agnostic)
pub struct MarketDataClientBuilder {
    config: TestExchangeConfig,
    buffer_size: usize,
}

impl MarketDataClientBuilder {
    pub fn new(config: TestExchangeConfig) -> Self {
        Self {
            config,
            buffer_size: 100,
        }
    }

    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Build a market data client for the configured exchange
    pub fn build(&self) -> Box<dyn trading_core::traits::MarketDataClient> {
        match self.config.exchange {
            Exchange::Binance => {
                use market_data_engine::exchanges::binance::{
                    client::BinanceClient, config::BinanceConfig,
                };

                let config = BinanceConfig {
                    pairs: self.config.pairs.clone(),
                    limit: 100,
                    base_delay: None,
                    max_delay: None,
                };

                Box::new(BinanceClient::new(config, self.buffer_size, None, vec![]))
            }
            Exchange::BinanceFutures | Exchange::Bybit => {
                todo!("Futures market data clients not yet implemented in tests")
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
        }
    }

    /// Build a market data client with a message handler
    pub fn build_with_handler(
        &self,
        handler: Option<Box<dyn market_data_engine::messages::WSMessageHandler + Send + Sync>>,
    ) -> Box<dyn trading_core::traits::MarketDataClient> {
        match self.config.exchange {
            Exchange::Binance => {
                use market_data_engine::exchanges::binance::{
                    client::BinanceClient, config::BinanceConfig,
                };

                let config = BinanceConfig {
                    pairs: self.config.pairs.clone(),
                    limit: 100,
                    base_delay: None,
                    max_delay: None,
                };

                let client = BinanceClient::new(config, self.buffer_size, handler, vec![]);
                Box::new(client)
            }
            Exchange::BinanceFutures | Exchange::Bybit => {
                todo!("Futures market data clients not yet implemented in tests")
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
        }
    }
}

/// Builder for asset data clients (exchange-agnostic)
pub struct AssetDataClientBuilder {
    config: TestExchangeConfig,
}

impl AssetDataClientBuilder {
    pub fn new(config: TestExchangeConfig) -> Self {
        Self { config }
    }

    /// Build an asset data client for the configured exchange
    pub fn build(&self) -> Arc<dyn asset_engine::AssetDataClient> {
        match self.config.exchange {
            Exchange::Binance => {
                use asset_engine::exchanges::binance::public_client::BinancePublicClient;
                Arc::new(BinancePublicClient::new(
                    "https://api.binance.com".to_string(),
                ))
            }
            Exchange::BinanceFutures | Exchange::Bybit => {
                todo!("Futures asset clients not yet implemented in tests")
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
        }
    }
}
