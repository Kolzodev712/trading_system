//! Live config service tests

use live_config::service::LiveConfigService;
use live_config::types::{
    LevelField, LevelUpdateRequest, LevelValue, RiskLimitType, RiskLimitUpdateRequest,
};
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::{Pair, Side};
use trading_engine::levels::config::{
    LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
};
use trading_engine::levels::LevelConfig;
use trading_engine::risk::{RiskLimits, RiskManager};

#[tokio::test]
async fn test_live_config_service_creation() {
    let level_manager = Arc::new(RwLock::new(trading_engine::levels::LevelManager::new()));
    let limits = RiskLimits::default();
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let risk_manager = Arc::new(RwLock::new(RiskManager::new(limits, symbol_registry)));

    let _service = LiveConfigService::new(
        level_manager,
        risk_manager,
        None,
        "/tmp/test_audit.log".to_string(),
    );

    // Should not panic
    assert!(true);
}

#[tokio::test]
async fn test_live_config_update_level_value() {
    let mut level_manager = trading_engine::levels::LevelManager::new();
    let config = LevelConfig {
        id: SmolStr::new("test_level"),
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

    let level_manager = Arc::new(RwLock::new(level_manager));
    let limits = RiskLimits::default();
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let risk_manager = Arc::new(RwLock::new(RiskManager::new(limits, symbol_registry)));

    let service = LiveConfigService::new(
        level_manager.clone(),
        risk_manager,
        None,
        "/tmp/test_audit.log".to_string(),
    );

    let request = LevelUpdateRequest {
        level_id: SmolStr::new("test_level"),
        field: LevelField::Enabled,
        value: LevelValue::Boolean(false),
    };

    let result = service.update_level_value(request).await;
    assert!(result.is_ok());

    // Verify update
    let lm = level_manager.read().await;
    let level = lm.get_level(&SmolStr::new("test_level")).unwrap();
    assert!(!level.config.enabled);
}

#[tokio::test]
async fn test_live_config_update_risk_limit() {
    let level_manager = Arc::new(RwLock::new(trading_engine::levels::LevelManager::new()));
    let limits = RiskLimits::default();
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let risk_manager = Arc::new(RwLock::new(RiskManager::new(limits, symbol_registry)));

    let service = LiveConfigService::new(
        level_manager,
        risk_manager.clone(),
        None,
        "/tmp/test_audit.log".to_string(),
    );

    let request = RiskLimitUpdateRequest {
        limit_type: RiskLimitType::TotalMaxPosition,
        value: dec!(5000),
    };
    let result = service.update_risk_limit(request).await;
    assert!(result.is_ok());

    // Verify update
    let rm = risk_manager.read().await;
    assert_eq!(rm.get_limits().total_max_position, Some(dec!(5000)));
}
