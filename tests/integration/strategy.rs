//! Strategy monitoring and execution tests

use super::helpers::*;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::Pair;
use trading_engine::state::{TradingState, TradingStateManager};

#[tokio::test]
async fn test_strategy_monitoring_with_position() {
    // Create a mock trading state with a position
    let mut trading_state = TradingState::new();
    let pair = Pair::new("BTC", "USDC");
    trading_state.update_position(pair.clone(), dec!(0.0015), dec!(50000));

    // Create trading state manager
    let trading_state_manager = Arc::new(TradingStateManager::new(
        trading_core::channels::exchange::exchange_update_channel(100).1,
        exchange_engine::state::manager::ExchangeStateManager::new(
            Arc::new(SymbolRegistry::new()),
            trading_core::channels::exchange::exchange_update_channel(100).0,
        ),
    ));

    // Update the state
    {
        let state_arc = trading_state_manager.get_state();
        let mut state = state_arc.write().await;
        *state = trading_state;
    }

    // Create monitoring service with strategy info
    let exchange_state_manager = Arc::new(
        exchange_engine::state::manager::ExchangeStateManager::new(
            Arc::new(SymbolRegistry::new()),
            trading_core::channels::exchange::exchange_update_channel(100).0,
        ),
    );

    let indicator_calculator = Arc::new(indicator_engine::calculator::IndicatorCalculator::new());
    let monitoring_service = runner::grpc::service::monitoring::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_info: Some(runner::grpc::service::monitoring::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }),
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status is present
    assert!(monitoring_data.strategy.is_some());
    let strategy_status = monitoring_data.strategy.unwrap();

    assert_eq!(strategy_status.strategy_name, "BuyLowSellHigh");
    assert_eq!(strategy_status.symbol, "BTCUSDC");
    assert_eq!(strategy_status.position_quantity, "0.0015");
    assert_eq!(strategy_status.position_avg_price, "50000");
    assert_eq!(strategy_status.active_orders, 0);
    assert_eq!(strategy_status.max_active_orders, 2);
}

#[tokio::test]
async fn test_strategy_monitoring_without_position() {
    // Create trading state manager with no position
    let trading_state_manager = Arc::new(TradingStateManager::new(
        trading_core::channels::exchange::exchange_update_channel(100).1,
        exchange_engine::state::manager::ExchangeStateManager::new(
            Arc::new(SymbolRegistry::new()),
            trading_core::channels::exchange::exchange_update_channel(100).0,
        ),
    ));

    let exchange_state_manager = Arc::new(
        exchange_engine::state::manager::ExchangeStateManager::new(
            Arc::new(SymbolRegistry::new()),
            trading_core::channels::exchange::exchange_update_channel(100).0,
        ),
    );

    let pair = Pair::new("BTC", "USDC");
    let indicator_calculator = Arc::new(indicator_engine::calculator::IndicatorCalculator::new());
    let monitoring_service = runner::grpc::service::monitoring::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_info: Some(runner::grpc::service::monitoring::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }),
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status is present
    assert!(monitoring_data.strategy.is_some());
    let strategy_status = monitoring_data.strategy.unwrap();

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
    
    // Add an active order
    let order = create_test_order("order1", dec!(50000), dec!(0.001));
    trading_state.add_order(order);

    let trading_state_manager = Arc::new(TradingStateManager::new(
        trading_core::channels::exchange::exchange_update_channel(100).1,
        exchange_engine::state::manager::ExchangeStateManager::new(
            Arc::new(SymbolRegistry::new()),
            trading_core::channels::exchange::exchange_update_channel(100).0,
        ),
    ));

    {
        let state_arc = trading_state_manager.get_state();
        let mut state = state_arc.write().await;
        *state = trading_state;
    }

    let exchange_state_manager = Arc::new(
        exchange_engine::state::manager::ExchangeStateManager::new(
            Arc::new(SymbolRegistry::new()),
            trading_core::channels::exchange::exchange_update_channel(100).0,
        ),
    );

    let indicator_calculator = Arc::new(indicator_engine::calculator::IndicatorCalculator::new());
    let monitoring_service = runner::grpc::service::monitoring::MonitoringService {
        level_manager: Arc::new(RwLock::new(trading_engine::levels::LevelManager::new())),
        trading_state_manager: trading_state_manager.clone(),
        exchange_state_manager,
        indicator_calculator: indicator_calculator.clone(),
        strategy_info: Some(runner::grpc::service::monitoring::StrategyInfo {
            strategy_name: "BuyLowSellHigh".to_string(),
            symbol: pair.clone(),
            max_active_orders: 2,
        }),
    };

    // Collect monitoring data
    let monitoring_data = monitoring_service.collect_monitoring_data().await.unwrap();

    // Verify strategy status shows active orders
    assert!(monitoring_data.strategy.is_some());
    let strategy_status = monitoring_data.strategy.unwrap();

    assert_eq!(strategy_status.active_orders, 1);
    assert_eq!(strategy_status.max_active_orders, 2);
}

