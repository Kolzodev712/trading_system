//! End-to-end workflow tests

use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::{Pair, Side};
use trading_engine::levels::config::{
    LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
};
use trading_engine::levels::{LevelConfig, LevelManager};
use trading_engine::risk::{RiskLimits, RiskManager};

#[tokio::test]
async fn test_level_workflow_with_risk_checks() {
    // Setup
    let mut level_manager = LevelManager::new();
    let limits = RiskLimits::default();
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let risk_manager = RiskManager::new(limits, symbol_registry);
    let trading_state = trading_engine::state::TradingState::new();

    // Create level
    let config = LevelConfig {
        id: SmolStr::new("workflow_level"),
        name: None,
        symbol: Pair::new("BTC", "USDT"),
        side: Side::Buy,
        order_type: OrderTypeConfig::Limit,
        quantity: QuantityConfig::Fixed(dec!(0.001)),
        price: PriceConfig::Fixed(dec!(50000)),
        risk: LevelRiskConfig::default(),
        lifecycle: LifecycleConfig::default(),
        enabled: true,
        group: None,
    };

    level_manager.add_level(config).await.unwrap();

    // Get ready levels
    let ready = level_manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 1);

    // Check risk
    let risk_check = risk_manager.check_total_position_limits(&trading_state);
    assert!(risk_check.allowed);
}

#[tokio::test]
async fn test_level_lifecycle_states() {
    let mut manager = LevelManager::new();

    let config = LevelConfig {
        id: SmolStr::new("lifecycle_test"),
        name: None,
        symbol: Pair::new("BTC", "USDT"),
        side: Side::Buy,
        order_type: OrderTypeConfig::Limit,
        quantity: QuantityConfig::Fixed(dec!(0.001)),
        price: PriceConfig::Fixed(dec!(50000)),
        risk: LevelRiskConfig::default(),
        lifecycle: LifecycleConfig::default(),
        enabled: true,
        group: None,
    };

    manager.add_level(config).await.unwrap();

    // Initial state should be Created
    let level = manager.get_level(&SmolStr::new("lifecycle_test")).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Created);

    // Transition to Posted
    manager
        .update_level_state(
            &SmolStr::new("lifecycle_test"),
            trading_engine::levels::LevelState::Posted,
        )
        .unwrap();
    let level = manager.get_level(&SmolStr::new("lifecycle_test")).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Posted);
}

#[tokio::test]
async fn test_multiple_levels_concurrent_operations() {
    let mut manager = LevelManager::new();

    // Create multiple levels
    for i in 0..10 {
        let config = LevelConfig {
            id: SmolStr::new(&format!("level_{}", i)),
            name: None,
            symbol: Pair::new("BTC", "USDT"),
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000) + rust_decimal::Decimal::new(i, 0)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: i % 2 == 0, // Alternate enabled/disabled
            group: None,
        };
        manager.add_level(config).await.unwrap();
    }

    // Check counts
    assert_eq!(manager.get_all_levels().len(), 10);
    let ready = manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 5); // Half enabled

    // Disable all
    for i in 0..10 {
        manager
            .disable_level(&SmolStr::new(&format!("level_{}", i)))
            .await
            .unwrap();
    }

    let ready = manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 0);
}
