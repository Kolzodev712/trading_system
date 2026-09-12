//! Strategy monitoring and execution tests

use crate::helpers::internal::MockTradingClient;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::channels::market_data::market_data_update_channel;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::Pair;
use trading_engine::state::{manager::TradingStateManager, TradingState};

#[tokio::test]
async fn test_strategy_monitoring_with_position() {
    // Create a mock trading state with a position
    let mut trading_state = TradingState::new();
    let pair = Pair::new("BTC", "USDC");
    trading_state.update_position(pair.clone(), dec!(0.0015), dec!(50000));

    // Create exchange state manager first (needed for trading state manager)
    let (exchange_tx, exchange_rx) =
        trading_core::venue::cex::channels::cex_venue_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let trading_client: Arc<
        tokio::sync::RwLock<
            dyn trading_core::venue::cex::traits::TradingClient<
                trading_core::types::exchange::OrderRequest,
            >,
        >,
    > = Arc::new(tokio::sync::RwLock::new(MockTradingClient::new()));

    let exchange_state_manager =
        Arc::new(exchange_engine::state::manager::ExchangeStateManager::new(
            exchange_tx,
            symbol_registry,
            trading_client,
            0, // Disabled reconciliation
            false,
        ));

    // Create trading state manager
    let trading_state_manager = Arc::new(TradingStateManager::new(
        exchange_rx,
        exchange_state_manager.clone(),
    ));

    // Update the state
    {
        let state_arc = trading_state_manager.get_state();
        let mut state = state_arc.write().await;
        *state = trading_state;
    }

    let indicator_calculator = Arc::new(indicator_engine::calculator::IndicatorCalculator::new());

    // Create market data state manager for orderbooks
    let (market_data_tx, _market_data_rx) = market_data_update_channel(100);
    let market_data_symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair.clone()]));
    let market_data_state_manager = Arc::new(
        market_data_engine::state::manager::MarketDataStateManager::new(
            market_data_tx,
            market_data_symbol_registry,
        ),
    );

    let monitoring_service = runner::grpc::service::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        market_data_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_infos: vec![runner::grpc::service::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }],
        strategy_manager: None,
        instrument_type: trading_core::types::common::InstrumentType::Spot,
        exchange: "Test".to_string(),
        dex_state_manager: None,
        dex_venue_label: None,
        dex_quote_manager: None,
        dex_strategy_manager: None,
        dex_indicator_engine: None,
        strategy_caps: None,
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status is present
    assert!(!monitoring_data.strategies.is_empty());
    let strategy_status = &monitoring_data.strategies[0];

    assert_eq!(strategy_status.strategy_name, "BuyLowSellHigh");
    assert_eq!(strategy_status.symbol, "BTC/USDC");
    assert_eq!(strategy_status.position_quantity, "0.0015");
    assert_eq!(strategy_status.position_avg_price, "50000");
    assert_eq!(strategy_status.active_orders, 0);
    assert_eq!(strategy_status.max_active_orders, 2);
}

#[tokio::test]
async fn test_strategy_monitoring_without_position() {
    // Create exchange state manager first
    let (exchange_tx, exchange_rx) =
        trading_core::venue::cex::channels::cex_venue_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let trading_client: Arc<
        tokio::sync::RwLock<
            dyn trading_core::venue::cex::traits::TradingClient<
                trading_core::types::exchange::OrderRequest,
            >,
        >,
    > = Arc::new(tokio::sync::RwLock::new(MockTradingClient::new()));

    let exchange_state_manager =
        Arc::new(exchange_engine::state::manager::ExchangeStateManager::new(
            exchange_tx,
            symbol_registry,
            trading_client,
            0, // Disabled reconciliation
            false,
        ));

    // Create trading state manager with no position
    let trading_state_manager = Arc::new(TradingStateManager::new(
        exchange_rx,
        exchange_state_manager.clone(),
    ));

    let pair = Pair::new("BTC", "USDC");
    let indicator_calculator = Arc::new(indicator_engine::calculator::IndicatorCalculator::new());

    // Create market data state manager for orderbooks
    let (market_data_tx, _market_data_rx) = market_data_update_channel(100);
    let market_data_symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair.clone()]));
    let market_data_state_manager = Arc::new(
        market_data_engine::state::manager::MarketDataStateManager::new(
            market_data_tx,
            market_data_symbol_registry,
        ),
    );

    let monitoring_service = runner::grpc::service::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        market_data_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_infos: vec![runner::grpc::service::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }],
        strategy_manager: None,
        instrument_type: trading_core::types::common::InstrumentType::Spot,
        exchange: "Test".to_string(),
        dex_state_manager: None,
        dex_venue_label: None,
        dex_quote_manager: None,
        dex_strategy_manager: None,
        dex_indicator_engine: None,
        strategy_caps: None,
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status is present
    assert!(!monitoring_data.strategies.is_empty());
    let strategy_status = &monitoring_data.strategies[0];

    assert_eq!(strategy_status.strategy_name, "BuyLowSellHigh");
    assert_eq!(strategy_status.position_quantity, "0");
    assert_eq!(strategy_status.position_avg_price, "0");
    assert_eq!(strategy_status.active_orders, 0);
}

#[tokio::test]
async fn test_strategy_monitoring_with_active_orders() {
    // Create trading state with active orders
    let mut trading_state = TradingState::new();
    let pair = Pair::new("BTC", "USDC");

    // Add an active order with matching symbol (BTCUSDC to match the pair)
    // Note: create_test_order creates an order with BTC/USDT pair
    // We'll create a new order with the correct pair for this test
    use trading_core::types::common::{Instrument, InstrumentType, OrderId, OrderStatus, Side};
    use trading_core::types::exchange::Order;
    let order = Order {
        order_id: OrderId::new("order1".into()),
        client_order_id: None,
        instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
        side: Side::Buy,
        price: dec!(50000),
        amount: dec!(0.001),
        internal_created_at: 0,
        exchange_created_at: None,
        status: OrderStatus::New,
        filled_amount: dec!(0),
    };
    trading_state.add_order(order);

    // Create exchange state manager first
    let (exchange_tx, exchange_rx) =
        trading_core::venue::cex::channels::cex_venue_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let trading_client: Arc<
        tokio::sync::RwLock<
            dyn trading_core::venue::cex::traits::TradingClient<
                trading_core::types::exchange::OrderRequest,
            >,
        >,
    > = Arc::new(tokio::sync::RwLock::new(MockTradingClient::new()));

    let exchange_state_manager =
        Arc::new(exchange_engine::state::manager::ExchangeStateManager::new(
            exchange_tx,
            symbol_registry,
            trading_client,
            0, // Disabled reconciliation
            false,
        ));

    let trading_state_manager = Arc::new(TradingStateManager::new(
        exchange_rx,
        exchange_state_manager.clone(),
    ));

    {
        let state_arc = trading_state_manager.get_state();
        let mut state = state_arc.write().await;
        *state = trading_state;
    }

    let indicator_calculator = Arc::new(indicator_engine::calculator::IndicatorCalculator::new());

    // Create market data state manager for orderbooks
    let (market_data_tx, _market_data_rx) = market_data_update_channel(100);
    let market_data_symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair.clone()]));
    let market_data_state_manager = Arc::new(
        market_data_engine::state::manager::MarketDataStateManager::new(
            market_data_tx,
            market_data_symbol_registry,
        ),
    );

    let monitoring_service = runner::grpc::service::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        market_data_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_infos: vec![runner::grpc::service::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }],
        strategy_manager: None,
        instrument_type: trading_core::types::common::InstrumentType::Spot,
        exchange: "Test".to_string(),
        dex_state_manager: None,
        dex_venue_label: None,
        dex_quote_manager: None,
        dex_strategy_manager: None,
        dex_indicator_engine: None,
        strategy_caps: None,
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status shows active orders
    assert!(!monitoring_data.strategies.is_empty());
    let strategy_status = &monitoring_data.strategies[0];

    assert_eq!(strategy_status.active_orders, 1);
    assert_eq!(strategy_status.max_active_orders, 2);
}
