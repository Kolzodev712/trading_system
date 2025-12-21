//! Test helpers and utilities for integration tests

use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::{Pair, Side, Symbol};
use trading_core::types::exchange::{Balance, Order, OrderRequest};
use trading_engine::levels::config::{
    LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
};
use trading_engine::levels::LevelConfig;
use trading_engine::risk::{RiskLimits, RiskManager};

/// Create a test level configuration
pub fn create_test_level_config(id: &str) -> LevelConfig {
    LevelConfig {
        id: SmolStr::new(id),
        name: None,
        side: Side::Buy,
        order_type: OrderTypeConfig::Limit,
        quantity: QuantityConfig::Fixed(dec!(0.001)),
        price: PriceConfig::Fixed(dec!(50000)),
        risk: LevelRiskConfig::default(),
        lifecycle: LifecycleConfig::default(),
        enabled: true,
        group: None,
    }
}

/// Create a test risk manager with default limits
pub fn create_test_risk_manager() -> RiskManager {
    let limits = RiskLimits::default();
    let symbol_registry = Arc::new(SymbolRegistry::new());
    RiskManager::new(limits, symbol_registry)
}

/// Create a test order
pub fn create_test_order(
    order_id: &str,
    price: rust_decimal::Decimal,
    amount: rust_decimal::Decimal,
) -> Order {
    Order {
        order_id: trading_core::types::common::OrderId::new(order_id.into()),
        client_order_id: None,
        symbol: Symbol::from("BTCUSDT"),
        side: Side::Buy,
        price,
        amount,
        internal_created_at: 0,
        exchange_created_at: None,
        status: trading_core::types::common::OrderStatus::New,
        filled_amount: dec!(0),
    }
}

/// Create a test order request
pub fn create_test_order_request(
    price: rust_decimal::Decimal,
    amount: rust_decimal::Decimal,
) -> OrderRequest {
    OrderRequest {
        symbol: Symbol::from("BTCUSDT"),
        side: Side::Buy,
        order_type: trading_core::types::common::OrderType::Limit,
        price,
        amount,
        timestamp: None,
        exchange_params: std::collections::HashMap::new(),
    }
}

/// Create a test balance
pub fn create_test_balance(
    currency: &str,
    free: rust_decimal::Decimal,
    used: rust_decimal::Decimal,
) -> Balance {
    Balance {
        symbol: Symbol::from(currency),
        free,
        used,
        total: free + used,
    }
}

/// Create Binance config for testing
pub fn create_test_binance_config() -> exchange_engine::exchanges::binance::config::BinanceConfig {
    use exchange_engine::exchanges::binance::config::BinanceCredentials;
    use trading_core::secret::SecretString;

    exchange_engine::exchanges::binance::config::BinanceConfig {
        pairs: vec![Pair::new("BTC", "USDT")],
        rest: BinanceCredentials {
            api_key: SecretString::new("test_key"),
            secret_key: SecretString::new("test_secret"),
        },
        ws: BinanceCredentials {
            api_key: SecretString::new("test_key"),
            secret_key: SecretString::new("test_secret"),
        },
        limit: None,
        base_delay: None,
        max_delay: None,
    }
}

/// Create exchange state manager for testing
pub async fn create_test_exchange_state_manager() -> (
    exchange_engine::state::manager::ExchangeStateManager,
    trading_core::channels::exchange::ExchangeUpdateReceiver,
) {
    use exchange_engine::exchanges::binance::ws_client::BinanceClient;
    use trading_core::channels::exchange::exchange_update_channel;
    use trading_core::config::ChannelConfig;
    use trading_core::registry::symbol::SymbolRegistry;

    let (tx, rx) = exchange_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let channels = ChannelConfig::default();
    let binance_config = create_test_binance_config();
    let trading_client: Arc<RwLock<dyn trading_core::traits::TradingClient<OrderRequest>>> =
        Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));
    let manager = exchange_engine::state::manager::ExchangeStateManager::new(
        tx,
        symbol_registry,
        trading_client,
        0,
        false,
    );
    (manager, rx)
}

/// Create market data state manager for testing
pub fn create_test_market_data_manager() -> (
    market_data_engine::state::manager::MarketDataStateManager,
    trading_core::channels::market_data::MarketDataUpdateReceiver,
) {
    use trading_core::channels::market_data::market_data_update_channel;
    use trading_core::registry::symbol::SymbolRegistry;
    use trading_core::types::common::Pair;

    let (tx, rx) = market_data_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
    let manager =
        market_data_engine::state::manager::MarketDataStateManager::new(tx, symbol_registry);
    (manager, rx)
}
