//! Configuration types and loading for the trading system
//! This is in the runner crate since it's the orchestrator that loads and uses configs
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use trading_core::types::common::{Exchange, Pair};

/// General bot configuration - system-wide settings, not trading-specific
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// System-wide settings
    pub system: SystemConfig,

    /// Exchange configuration
    pub exchange: ExchangeConfig,

    /// HTTP server configuration
    pub server: ServerConfig,

    /// Redis configuration
    pub redis: RedisConfig,

    /// Feature flags
    #[serde(default)]
    pub features: FeatureFlags,
}

/// System-wide settings
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

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP server bind address
    pub bind_address: String,

    /// HTTP server port
    pub port: u16,
}

/// Redis configuration
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

/// Feature flags for enabling/disabling system features
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeatureFlags {
    /// Enable strategies mode
    #[serde(default = "default_true")]
    pub enable_strategies: bool,

    /// Enable levels mode
    #[serde(default = "default_true")]
    pub enable_levels: bool,

    /// Enable indicator engine
    #[serde(default = "default_true")]
    pub enable_indicators: bool,

    /// Enable reconciliation
    #[serde(default = "default_true")]
    pub enable_reconciliation: bool,
}

fn default_true() -> bool {
    true
}

/// Strategy-specific configuration - trading parameters for strategy mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategiesConfig {
    /// Strategy type: "BuyLowSellHigh", "MeanReversion", etc.
    pub strategy_type: String,

    /// Trading pair for this strategy
    pub symbol: Pair,

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

impl BotConfig {
    /// Load bot configuration from a JSON file
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_data = std::fs::read_to_string(path)?;
        let config: BotConfig = serde_json::from_str(&config_data)?;
        Ok(config)
    }

    /// Validate bot configuration
    pub fn validate(&self) -> Result<(), String> {
        // Basic validation - can be extended
        if self.server.port == 0 {
            return Err("Server port must be greater than zero".to_string());
        }
        if self.redis.url.is_empty() && self.redis.enabled {
            return Err("Redis URL cannot be empty when Redis is enabled".to_string());
        }
        Ok(())
    }
}

impl StrategiesConfig {
    /// Load strategies configuration from a JSON file
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_data = std::fs::read_to_string(path)?;
        let config: StrategiesConfig = serde_json::from_str(&config_data)?;
        Ok(config)
    }

    /// Validate strategies configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.position_size <= Decimal::ZERO {
            return Err("Position size must be greater than zero".to_string());
        }
        if self.max_position <= Decimal::ZERO {
            return Err("Max position must be greater than zero".to_string());
        }
        if self.position_size > self.max_position {
            return Err("Position size cannot exceed max position".to_string());
        }
        if self.buy_threshold_percent <= Decimal::ZERO {
            return Err("Buy threshold must be greater than zero".to_string());
        }
        if self.sell_threshold_percent <= Decimal::ZERO {
            return Err("Sell threshold must be greater than zero".to_string());
        }
        if let Some(stop_loss) = self.stop_loss_percent {
            if stop_loss <= Decimal::ZERO {
                return Err("Stop loss must be greater than zero".to_string());
            }
        }
        if let Some(take_profit) = self.take_profit_percent {
            if take_profit <= Decimal::ZERO {
                return Err("Take profit must be greater than zero".to_string());
            }
        }
        Ok(())
    }
}

// Legacy type alias for backward compatibility during migration
/// @deprecated: Use BotConfig instead. This will be removed.
pub type TradingSystemConfig = BotConfig;
