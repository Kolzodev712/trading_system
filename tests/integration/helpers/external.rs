//! Generic builders for external exchange clients and drivers
//!
//! These builders are **not** a Paper process harness and must not construct a
//! Bybit trading client that targets mainnet. Mutating Bybit Demo workflows live
//! in `workflow_harness` behind an explicit command.

use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{
    config::ChannelConfig,
    registry::symbol::SymbolRegistry,
    secret::SecretString,
    types::{
        common::{Exchange, Pair},
        exchange::OrderRequest,
    },
    venue::cex::channels::cex_venue_update_channel,
    venue::cex::traits::TradingClient,
};

use exchange_engine::{
    exchanges::{
        binance::{client::BinanceClient, config::BinanceConfig},
        bybit::{client::BybitClient, config::BybitConfig},
    },
    state::manager::ExchangeStateManager,
};

/// Explicit Bybit environment for a trading client. Default refuses construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BybitTradingTarget {
    #[default]
    Refused,
    Demo,
}

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
    bybit_trading: BybitTradingTarget,
}

impl ExchangeClientBuilder {
    pub fn new(config: TestExchangeConfig) -> Self {
        Self {
            config,
            channels: ChannelConfig::default(),
            bybit_trading: BybitTradingTarget::Refused,
        }
    }

    pub fn with_channels(mut self, channels: ChannelConfig) -> Self {
        self.channels = channels;
        self
    }

    pub fn with_bybit_trading(mut self, target: BybitTradingTarget) -> Self {
        self.bybit_trading = target;
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
            Exchange::Bybit => match self.bybit_trading {
                BybitTradingTarget::Demo => {
                    let bybit_config = self.create_bybit_demo_config();
                    Arc::new(RwLock::new(BybitClient::new(
                        bybit_config,
                        self.channels.websocket_inbound_capacity,
                        self.channels.websocket_ping_capacity,
                        self.channels.websocket_outbound_rpc_capacity,
                        self.channels.shutdown_capacity,
                    )))
                }
                BybitTradingTarget::Refused => {
                    panic!(
                        "refusing Bybit trading client: demo:false targets mainnet. \
                         Use workflow_harness Demo adapter (`just test-demo-readonly` / \
                         `just test-demo-mutating`). This helper must not place Demo or live orders."
                    )
                }
            },
            Exchange::Paper => {
                panic!(
                    "Paper trading clients are not built by tests/integration/helpers. \
                     Use the workflow_harness Paper adapter (`just test-paper`)."
                )
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
            Exchange::DeepBook => {
                panic!("DeepBook is configured via dex_engine, not exchange_engine tests")
            }
        }
    }

    /// Build an exchange state manager with the configured client
    pub async fn build_state_manager(
        &self,
    ) -> (
        ExchangeStateManager,
        trading_core::venue::cex::channels::CexVenueUpdateReceiver,
    ) {
        let (tx, rx) = cex_venue_update_channel(self.channels.cex_venue_update_capacity);
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
        BinanceConfig {
            pairs: self.config.pairs.clone(),
            api_key: SecretString::from("key"),
            secret: SecretString::from("secret"),
            limit: None,
            base_delay: None,
            max_delay: None,
        }
    }

    fn create_bybit_demo_config(&self) -> BybitConfig {
        use exchange_engine::exchanges::bybit::config::{BybitCredentials, BybitRateLimiterConfig};
        BybitConfig {
            pairs: self.config.pairs.clone(),
            instrument_type: trading_core::types::common::InstrumentType::Spot,
            credentials: BybitCredentials {
                api_key: SecretString::from(self.config.api_key.as_str()),
                api_secret: SecretString::from(self.config.secret_key.as_str()),
            },
            testnet: false,
            demo: true,
            leverage: None,
            rate_limiter: BybitRateLimiterConfig::default(),
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
    pub fn build(&self) -> Box<dyn trading_core::venue::cex::traits::MarketDataClient> {
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
            Exchange::Bybit => {
                use market_data_engine::exchanges::bybit::{
                    client::BybitClient, config::BybitCategory, config::BybitConfig,
                };

                let config = BybitConfig {
                    pairs: self.config.pairs.clone(),
                    limit: 1,
                    base_delay: None,
                    max_delay: None,
                    category: BybitCategory::Spot,
                };

                Box::new(BybitClient::new(config, self.buffer_size, None, vec![]))
            }
            Exchange::Paper => {
                panic!(
                    "Paper market-data clients are not built by tests/integration/helpers. \
                     Use the workflow_harness Paper adapter (`just test-paper`)."
                )
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
            Exchange::DeepBook => {
                panic!("DeepBook is configured via dex_engine, not exchange_engine tests")
            }
        }
    }

    /// Build a market data client with a message handler
    pub fn build_with_handler(
        &self,
        handler: Option<Box<dyn market_data_engine::messages::WSMessageHandler + Send + Sync>>,
    ) -> Box<dyn trading_core::venue::cex::traits::MarketDataClient> {
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
            Exchange::Bybit => {
                use market_data_engine::exchanges::bybit::{
                    client::BybitClient, config::BybitCategory, config::BybitConfig,
                };

                let config = BybitConfig {
                    pairs: self.config.pairs.clone(),
                    limit: 1,
                    base_delay: None,
                    max_delay: None,
                    category: BybitCategory::Spot,
                };

                let client = BybitClient::new(config, self.buffer_size, handler, vec![]);
                Box::new(client)
            }
            Exchange::Paper => {
                panic!(
                    "Paper market-data clients are not built by tests/integration/helpers. \
                     Use the workflow_harness Paper adapter (`just test-paper`)."
                )
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
            Exchange::DeepBook => {
                panic!("DeepBook is configured via dex_engine, not exchange_engine tests")
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
                use asset_engine::BinancePublicClient;
                Arc::new(BinancePublicClient::new(
                    "https://api.binance.com".to_string(),
                ))
            }
            Exchange::Bybit => {
                use asset_engine::exchanges::bybit::BybitPublicClient;
                Arc::new(BybitPublicClient::new("https://api.bybit.com".to_string()))
            }
            Exchange::Paper => {
                let base_url = std::env::var("PAPER_REST_BASE")
                    .unwrap_or_else(|_| "http://127.0.0.1:19999".to_string());
                Arc::new(asset_engine::exchanges::KPaperPublicClient::new(base_url))
            }
            Exchange::Unsupported => {
                panic!("Unsupported exchange cannot be used in tests")
            }
            Exchange::DeepBook => {
                panic!("DeepBook is configured via dex_engine, not exchange_engine tests")
            }
        }
    }
}

#[test]
fn paper_trading_builder_does_not_todo() {
    let builder = ExchangeClientBuilder::new(TestExchangeConfig {
        exchange: Exchange::Paper,
        ..TestExchangeConfig::default()
    });
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = builder.build_trading_client();
    }));
    assert!(err.is_err(), "Paper helper must fail closed, not todo!()");
}

#[test]
fn bybit_trading_builder_refuses_mainnet_default() {
    let builder = ExchangeClientBuilder::new(TestExchangeConfig {
        exchange: Exchange::Bybit,
        ..TestExchangeConfig::default()
    });
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = builder.build_trading_client();
    }));
    let payload = err.expect_err("Bybit helper must refuse demo:false mainnet");
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .map(str::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        text.contains("refusing Bybit") || text.contains("mainnet"),
        "unexpected panic: {text}"
    );
}
