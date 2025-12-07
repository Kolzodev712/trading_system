//! Configuration types for the trading system
use crate::types::common::Exchange;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Unified configuration for the entire trading system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSystemConfig {
    /// System-wide settings
    pub system: SystemConfig,

    /// Exchange configuration
    pub exchange: ExchangeConfig,

    /// Strategy configuration
    pub strategy: StrategyConfig,

    /// HTTP server configuration
    pub server: ServerConfig,

    /// Redis configuration
    pub redis: RedisConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Log level: "trace", "debug", "info", "warn", "error"
    pub log_level: String,

    /// Graceful shutdown timeout in seconds
    pub shutdown_timeout_secs: u64,
}

/// Generic exchange configuration that stores exchange-specific config as opaque JSON
/// This allows core to remain exchange-agnostic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    /// Exchange name
    pub exchange: Exchange,

    /// Exchange-specific configuration stored as JSON
    /// This will be deserialized by the exchange-specific engine
    /// The JSON should contain the exchange name as a key (e.g., "binance": {...})
    #[serde(flatten)]
    pub exchange_specific: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy type: "BuyLowSellHigh", "MeanReversion", etc.
    pub strategy_type: String,

    /// Position size per trade
    pub position_size: Decimal,

    /// Maximum total position size
    pub max_position: Decimal,

    /// Buy threshold percentage (e.g., 2.0 for 2%)
    pub buy_threshold_percent: Decimal,

    /// Sell threshold percentage (e.g., 2.0 for 2%)
    pub sell_threshold_percent: Decimal,

    /// Stop loss percentage (e.g., 5.0 for 5% loss)
    pub stop_loss_percent: Option<Decimal>,

    /// Take profit percentage (e.g., 10.0 for 10% profit)
    pub take_profit_percent: Option<Decimal>,

    /// Maximum active orders
    pub max_active_orders: usize,

    /// Price history size for moving average
    pub price_history_size: usize,

    /// Minimum price history before trading
    pub min_price_history: usize,

    /// Minimum order size (e.g., "0.00001" for BTC/USDC on Binance)
    /// This should match the exchange's minimum lot size requirement
    pub min_order_size: Option<Decimal>,

    /// Step size for order quantities (e.g., "0.00001" for BTC/USDC on Binance)
    /// Quantities must be rounded to a multiple of this step size
    pub step_size: Option<Decimal>,

    /// Tick size for order prices (e.g., "0.01" for BTC/USDC on Binance)
    /// Prices must be rounded to a multiple of this tick size
    pub tick_size: Option<Decimal>,

    /// Minimum notional value (price * quantity) in quote currency
    /// (e.g., "5.0" for BTC/USDC on Binance means minimum 5 USDC)
    pub min_notional: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP server bind address
    pub bind_address: String,

    /// HTTP server port
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,

    /// Whether Redis is enabled
    #[serde(default = "default_redis_enabled")]
    pub enabled: bool,
}

fn default_redis_enabled() -> bool {
    false
}

impl TradingSystemConfig {
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_data = std::fs::read_to_string(path)?;
        let config: TradingSystemConfig = serde_json::from_str(&config_data)?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.strategy.position_size <= Decimal::ZERO {
            return Err("Position size must be greater than zero".to_string());
        }
        if self.strategy.max_position <= Decimal::ZERO {
            return Err("Max position must be greater than zero".to_string());
        }
        if self.strategy.position_size > self.strategy.max_position {
            return Err("Position size cannot exceed max position".to_string());
        }
        if self.strategy.buy_threshold_percent <= Decimal::ZERO {
            return Err("Buy threshold must be greater than zero".to_string());
        }
        if self.strategy.sell_threshold_percent <= Decimal::ZERO {
            return Err("Sell threshold must be greater than zero".to_string());
        }
        if let Some(stop_loss) = self.strategy.stop_loss_percent {
            if stop_loss <= Decimal::ZERO {
                return Err("Stop loss must be greater than zero".to_string());
            }
        }
        if let Some(take_profit) = self.strategy.take_profit_percent {
            if take_profit <= Decimal::ZERO {
                return Err("Take profit must be greater than zero".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_trading_system_config_validation_success() {
        let config = TradingSystemConfig {
            system: SystemConfig {
                log_level: "info".to_string(),
                shutdown_timeout_secs: 30,
            },
            exchange: ExchangeConfig {
                exchange: Exchange::Binance,
                exchange_specific: serde_json::json!({}),
            },
            strategy: StrategyConfig {
                strategy_type: "BuyLowSellHigh".to_string(),
                position_size: Decimal::new(100, 0),
                max_position: Decimal::new(1000, 0),
                buy_threshold_percent: Decimal::new(2, 0),
                sell_threshold_percent: Decimal::new(2, 0),
                stop_loss_percent: Some(Decimal::new(5, 0)),
                take_profit_percent: Some(Decimal::new(10, 0)),
                max_active_orders: 10,
                price_history_size: 100,
                min_price_history: 10,
                min_order_size: Some(Decimal::new(1, 5)),
                step_size: Some(Decimal::new(1, 5)),
                tick_size: Some(Decimal::new(1, 2)),
                min_notional: Some(Decimal::new(5, 0)),
            },
            server: ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                port: 8080,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".to_string(),
                enabled: false,
            },
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_trading_system_config_validation_position_size_zero() {
        let mut config = create_valid_config();
        config.strategy.position_size = Decimal::ZERO;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_system_config_validation_max_position_zero() {
        let mut config = create_valid_config();
        config.strategy.max_position = Decimal::ZERO;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_system_config_validation_position_size_exceeds_max() {
        let mut config = create_valid_config();
        config.strategy.position_size = Decimal::new(2000, 0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_system_config_validation_buy_threshold_zero() {
        let mut config = create_valid_config();
        config.strategy.buy_threshold_percent = Decimal::ZERO;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_system_config_validation_sell_threshold_zero() {
        let mut config = create_valid_config();
        config.strategy.sell_threshold_percent = Decimal::ZERO;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_system_config_validation_stop_loss_zero() {
        let mut config = create_valid_config();
        config.strategy.stop_loss_percent = Some(Decimal::ZERO);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trading_system_config_validation_take_profit_zero() {
        let mut config = create_valid_config();
        config.strategy.take_profit_percent = Some(Decimal::ZERO);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_redis_config_default_enabled() {
        let json = r#"{"url":"redis://localhost:6379"}"#;
        let redis_config: RedisConfig = serde_json::from_str(json).unwrap();
        assert_eq!(redis_config.enabled, false);
    }

    #[test]
    fn test_redis_config_explicit_enabled() {
        let json = r#"{"url":"redis://localhost:6379","enabled":true}"#;
        let redis_config: RedisConfig = serde_json::from_str(json).unwrap();
        assert_eq!(redis_config.enabled, true);
    }

    #[test]
    fn test_config_serialization() {
        let config = create_valid_config();
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("Binance"));
        assert!(serialized.contains("BuyLowSellHigh"));
    }

    #[test]
    fn test_config_deserialization() {
        let json = r#"
        {
            "system": {
                "log_level": "info",
                "shutdown_timeout_secs": 30
            },
            "exchange": {
                "exchange": "Binance",
                "binance": {}
            },
            "strategy": {
                "strategy_type": "BuyLowSellHigh",
                "position_size": "100",
                "max_position": "1000",
                "buy_threshold_percent": "2",
                "sell_threshold_percent": "2",
                "stop_loss_percent": "5",
                "take_profit_percent": "10",
                "max_active_orders": 10,
                "price_history_size": 100,
                "min_price_history": 10
            },
            "server": {
                "bind_address": "0.0.0.0",
                "port": 8080
            },
            "redis": {
                "url": "redis://localhost:6379",
                "enabled": false
            }
        }
        "#;

        let config: TradingSystemConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.system.log_level, "info");
        assert_eq!(config.exchange.exchange, Exchange::Binance);
    }

    fn create_valid_config() -> TradingSystemConfig {
        TradingSystemConfig {
            system: SystemConfig {
                log_level: "info".to_string(),
                shutdown_timeout_secs: 30,
            },
            exchange: ExchangeConfig {
                exchange: Exchange::Binance,
                exchange_specific: serde_json::json!({}),
            },
            strategy: StrategyConfig {
                strategy_type: "BuyLowSellHigh".to_string(),
                position_size: Decimal::new(100, 0),
                max_position: Decimal::new(1000, 0),
                buy_threshold_percent: Decimal::new(2, 0),
                sell_threshold_percent: Decimal::new(2, 0),
                stop_loss_percent: Some(Decimal::new(5, 0)),
                take_profit_percent: Some(Decimal::new(10, 0)),
                max_active_orders: 10,
                price_history_size: 100,
                min_price_history: 10,
                min_order_size: Some(Decimal::new(1, 5)),
                step_size: Some(Decimal::new(1, 5)),
                tick_size: Some(Decimal::new(1, 2)),
                min_notional: Some(Decimal::new(5, 0)),
            },
            server: ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                port: 8080,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".to_string(),
                enabled: false,
            },
        }
    }
}
