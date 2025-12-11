//! Level management system tests

use super::helpers::create_test_level_config;
use smol_str::SmolStr;
use trading_core::types::common::OrderId;
use trading_engine::levels::LevelManager;

#[tokio::test]
async fn test_level_manager_crud() {
    let mut manager = LevelManager::new();

    // Create
    let config = create_test_level_config("test_level_1");
    let result = manager.add_level(config).await;
    assert!(result.is_ok());

    // Read
    let level = manager.get_level(&SmolStr::new("test_level_1"));
    assert!(level.is_some());
    assert_eq!(level.unwrap().config.id, SmolStr::new("test_level_1"));

    // Update
    let mut updated_config = create_test_level_config("test_level_1");
    updated_config.enabled = false;
    let result = manager
        .update_level_config(&SmolStr::new("test_level_1"), updated_config)
        .await;
    assert!(result.is_ok());

    let level = manager.get_level(&SmolStr::new("test_level_1")).unwrap();
    assert!(!level.config.enabled);

    // Delete
    let result = manager.remove_level(&SmolStr::new("test_level_1")).await;
    assert!(result.is_ok());
    assert!(manager.get_level(&SmolStr::new("test_level_1")).is_none());
}

#[tokio::test]
async fn test_level_manager_enable_disable() {
    let mut manager = LevelManager::new();
    let config = create_test_level_config("test_level_2");
    manager.add_level(config).await.unwrap();

    // Disable
    manager
        .disable_level(&SmolStr::new("test_level_2"))
        .await
        .unwrap();
    let level = manager.get_level(&SmolStr::new("test_level_2")).unwrap();
    assert!(!level.config.enabled);

    // Enable
    manager
        .enable_level(&SmolStr::new("test_level_2"))
        .await
        .unwrap();
    let level = manager.get_level(&SmolStr::new("test_level_2")).unwrap();
    assert!(level.config.enabled);
}

#[tokio::test]
async fn test_level_manager_get_ready_levels() {
    let mut manager = LevelManager::new();

    // Add enabled level
    let config1 = create_test_level_config("level_1");
    manager.add_level(config1).await.unwrap();

    // Add disabled level
    let mut config2 = create_test_level_config("level_2");
    config2.enabled = false;
    manager.add_level(config2).await.unwrap();

    let ready = manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].config.id, SmolStr::new("level_1"));
}

#[tokio::test]
async fn test_level_manager_state_transitions() {
    let mut manager = LevelManager::new();
    let config = create_test_level_config("state_test");
    manager.add_level(config).await.unwrap();

    // Initial state should be Created
    let level = manager.get_level(&SmolStr::new("state_test")).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Created);

    // Transition to Posted
    manager
        .update_level_state(
            &SmolStr::new("state_test"),
            trading_engine::levels::LevelState::Posted,
        )
        .unwrap();
    let level = manager.get_level(&SmolStr::new("state_test")).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Posted);

    // Transition to Filled
    manager
        .update_level_state(
            &SmolStr::new("state_test"),
            trading_engine::levels::LevelState::Filled,
        )
        .unwrap();
    let level = manager.get_level(&SmolStr::new("state_test")).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Filled);
}

#[tokio::test]
async fn test_level_manager_order_tracking() {
    let mut manager = LevelManager::new();
    let config = create_test_level_config("order_test");
    manager.add_level(config).await.unwrap();

    let order_id = OrderId::new("order123".into());
    manager
        .handle_order_filled(&SmolStr::new("order_test"), &order_id)
        .await
        .unwrap();

    let level = manager.get_level(&SmolStr::new("order_test")).unwrap();
    assert_eq!(level.state, trading_engine::levels::LevelState::Filled);
    assert!(level.last_filled_at.is_some());
}

#[tokio::test]
async fn test_level_manager_repost_logic() {
    let mut manager = LevelManager::new();
    let mut config = create_test_level_config("repost_test");
    config.lifecycle.auto_repost = true;
    config.lifecycle.auto_repost_on_cancel = false;
    manager.add_level(config).await.unwrap();

    // Filled should trigger repost
    manager
        .update_level_state(
            &SmolStr::new("repost_test"),
            trading_engine::levels::LevelState::Filled,
        )
        .unwrap();
    assert!(manager.should_repost(&SmolStr::new("repost_test")).unwrap());

    // Cancelled should not trigger repost
    manager
        .update_level_state(
            &SmolStr::new("repost_test"),
            trading_engine::levels::LevelState::Cancelled,
        )
        .unwrap();
    assert!(!manager.should_repost(&SmolStr::new("repost_test")).unwrap());
}

#[tokio::test]
async fn test_level_manager_multiple_levels() {
    let mut manager = LevelManager::new();

    for i in 0..5 {
        let config = create_test_level_config(&format!("level_{}", i));
        manager.add_level(config).await.unwrap();
    }

    assert_eq!(manager.get_all_levels().len(), 5);

    // Get ready levels
    let ready = manager.get_levels_ready_to_post();
    assert_eq!(ready.len(), 5);
}
