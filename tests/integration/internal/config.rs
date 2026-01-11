//! Configuration loading and validation tests

use runner::config::{BotConfig, StrategiesConfig};

#[tokio::test]
#[ignore] // Requires config files that may not exist in CI
async fn test_bot_config_loading() {
    let bot_config = BotConfig::load_from_file("config/bot.json");
    assert!(bot_config.is_ok(), "Should load bot config file");

    let bot_config = bot_config.unwrap();
    assert_eq!(
        bot_config.exchange.exchange,
        trading_core::types::common::Exchange::Binance
    );
}

#[tokio::test]
#[ignore] // Requires config files that may not exist in CI
async fn test_bot_config_validation() {
    let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
    let result = bot_config.validate();
    assert!(
        result.is_ok(),
        "Bot config should be valid: {:?}",
        result.err()
    );
}

#[tokio::test]
#[ignore] // Requires config files that may not exist in CI
async fn test_bot_config_all_fields() {
    let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
    assert!(!bot_config.system.log_level.is_empty());
    assert!(bot_config.channels.market_data_capacity > 0);
}

#[tokio::test]
#[ignore] // Requires config files that may not exist in CI
async fn test_strategies_config_loading() {
    let strategies_config = StrategiesConfig::load_from_file("config/strategies.json");
    assert!(
        strategies_config.is_ok(),
        "Should load strategies config file"
    );
}

#[tokio::test]
#[ignore] // Requires config files that may not exist in CI
async fn test_strategies_config_validation() {
    let strategies_config = StrategiesConfig::load_from_file("config/strategies.json").unwrap();
    let result = strategies_config.validate();
    assert!(result.is_ok(), "Strategies config should be valid");
}
