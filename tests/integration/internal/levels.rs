//! Level management system tests

use crate::helpers::internal::create_test_level_config;
use smol_str::SmolStr;
use trading_core::types::common::OrderId;
use trading_engine::levels::LevelManager;

#[tokio::test]
async fn test_level_manager_crud() {
    let mut manager = LevelManager::new();

    // Create
    let config = create_test_level_config("test_level_1");
    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    let result = manager.add_level(config, pair.clone()).await;
    assert!(result.is_ok());

    // Read
    let level = manager.get_level(&SmolStr::new("test_level_1_BTCUSDT"));
    assert!(level.is_some());
    assert_eq!(level.unwrap().config.id, SmolStr::new("test_level_1"));

    // Update
    let mut updated_config = create_test_level_config("test_level_1");
    updated_config.enabled = false;
    let level_id = SmolStr::new("test_level_1_BTCUSDT");
    let result = manager.update_level_config(&level_id, updated_config).await;
    assert!(result.is_ok());

    let level = manager.get_level(&level_id).unwrap();
    assert!(!level.config.enabled);

    // Delete
    let result = manager.remove_level(&level_id).await;
    assert!(result.is_ok());
    assert!(manager.get_level(&level_id).is_none());
}

#[tokio::test]
async fn test_level_manager_enable_disable() {
    let mut manager = LevelManager::new();
    let config = create_test_level_config("test_level_2");
    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    let level_id = SmolStr::new("test_level_2_BTCUSDT");
    manager.add_level(config, pair).await.unwrap();

    // Disable
    manager.disable_level(&level_id).await.unwrap();
    let level = manager.get_level(&level_id).unwrap();
    assert!(!level.config.enabled);

    // Enable
    manager.enable_level(&level_id).await.unwrap();
    let level = manager.get_level(&level_id).unwrap();
    assert!(level.config.enabled);
}

#[tokio::test]
async fn test_level_manager_get_ready_levels() {
    let mut manager = LevelManager::new();

    // Add enabled level
    let config1 = create_test_level_config("level_1");
    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    manager.add_level(config1, pair.clone()).await.unwrap();

    // Add disabled level
    let mut config2 = create_test_level_config("level_2");
    config2.enabled = false;
    manager.add_level(config2, pair).await.unwrap();

    let ready = manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].config.id, SmolStr::new("level_1"));
}

#[tokio::test]
async fn test_level_manager_state_transitions() {
    let mut manager = LevelManager::new();
    let config = create_test_level_config("state_test");
    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    let level_id = SmolStr::new("state_test_BTCUSDT");
    manager.add_level(config, pair).await.unwrap();

    // Initial state should be Created
    let level = manager.get_level(&level_id).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Created);

    // Transition to Posted
    manager
        .update_level_state(&level_id, trading_engine::levels::LevelState::Posted)
        .unwrap();
    let level = manager.get_level(&level_id).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Posted);

    // Transition to Filled
    manager
        .update_level_state(&level_id, trading_engine::levels::LevelState::Filled)
        .unwrap();
    let level = manager.get_level(&level_id).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Filled);
}

#[tokio::test]
async fn test_level_manager_order_tracking() {
    let mut manager = LevelManager::new();
    let config = create_test_level_config("order_test");
    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    let level_id = SmolStr::new("order_test_BTCUSDT");
    manager.add_level(config, pair).await.unwrap();

    let order_id = OrderId::new("order123".into());
    manager
        .handle_order_filled(&level_id, &order_id, rust_decimal::Decimal::ZERO)
        .await
        .unwrap();

    let level = manager.get_level(&level_id).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Filled);
    assert!(level.last_filled_at.is_some());
}

#[tokio::test]
async fn test_level_manager_repost_logic() {
    let mut manager = LevelManager::new();
    let mut config = create_test_level_config("repost_test");
    config.lifecycle.auto_repost = true;
    config.lifecycle.auto_repost_on_cancel = false;
    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    let level_id = SmolStr::new("repost_test_BTCUSDT");
    manager.add_level(config, pair).await.unwrap();

    // Filled should trigger repost
    manager
        .update_level_state(&level_id, trading_engine::levels::LevelState::Filled)
        .unwrap();
    assert!(manager.should_repost(&level_id).unwrap());

    // Cancelled should not trigger repost
    manager
        .update_level_state(&level_id, trading_engine::levels::LevelState::Cancelled)
        .unwrap();
    assert!(!manager.should_repost(&level_id).unwrap());
}

#[tokio::test]
async fn test_level_manager_multiple_levels() {
    let mut manager = LevelManager::new();

    let pair = trading_core::types::common::Pair::new("BTC", "USDT");
    for i in 0..5 {
        let config = create_test_level_config(&format!("level_{}", i));
        manager.add_level(config, pair.clone()).await.unwrap();
    }

    assert_eq!(manager.get_all_levels().len(), 5);

    // Get ready levels
    let ready = manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 5);
}
