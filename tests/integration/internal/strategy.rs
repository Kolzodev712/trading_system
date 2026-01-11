//! Strategy monitoring and execution tests

use crate::helpers::internal::MockTradingClient;
use crate::helpers::internal::*;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::RwLock;
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
    let (exchange_tx, exchange_rx) = trading_core::channels::exchange::exchange_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let trading_client: Arc<
        tokio::sync::RwLock<
            dyn trading_core::traits::TradingClient<trading_core::types::exchange::OrderRequest>,
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
    let monitoring_service = runner::grpc::service::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_infos: vec![runner::grpc::service::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }],
        strategy_manager: None,
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
    let (exchange_tx, exchange_rx) = trading_core::channels::exchange::exchange_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let trading_client: Arc<
        tokio::sync::RwLock<
            dyn trading_core::traits::TradingClient<trading_core::types::exchange::OrderRequest>,
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
    let monitoring_service = runner::grpc::service::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_infos: vec![runner::grpc::service::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }],
        strategy_manager: None,
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
    let mut order = create_test_order("order1", dec!(50000), dec!(0.001));
    order.symbol = trading_core::types::common::Symbol::from("BTCUSDC");
    trading_state.add_order(order);

    // Create exchange state manager first
    let (exchange_tx, exchange_rx) = trading_core::channels::exchange::exchange_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let trading_client: Arc<
        tokio::sync::RwLock<
            dyn trading_core::traits::TradingClient<trading_core::types::exchange::OrderRequest>,
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
    let monitoring_service = runner::grpc::service::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_infos: vec![runner::grpc::service::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }],
        strategy_manager: None,
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status shows active orders
    assert!(!monitoring_data.strategies.is_empty());
    let strategy_status = &monitoring_data.strategies[0];

    assert_eq!(strategy_status.active_orders, 1);
    assert_eq!(strategy_status.max_active_orders, 2);
}
