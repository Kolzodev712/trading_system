//! Configuration types and loading for the trading system
//! This is in the runner crate since it's the orchestrator that loads and uses configs
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use trading_core::secret::Secret;
use trading_core::types::common::{Exchange, InstrumentType, Pair};
use trading_core::venue::VenueMode;

/// General bot configuration - system-wide settings, not trading-specific
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// System-wide settings
    pub system: SystemConfig,

    /// Primary venue mode (CEX vs DEX). Defaults to CEX when omitted.
    #[serde(default)]
    pub venue: VenueConfig,

    /// CEX exchange engine configuration (required when `venue.mode` is `cex`)
    #[serde(rename = "exchange_engine")]
    pub exchange_engine: ExchangeConfig,

    /// DEX engine configuration (required when `venue.mode` is `dex`)
    #[serde(default)]
    pub dex_engine: Option<dex_engine::DexEngineConfig>,

    /// HTTP server configuration
    pub server: ServerConfig,

    /// Redis configuration (optional, only needed if using Redis feature)
    #[serde(default)]
    pub redis: RedisConfig,

    /// Feature flags
    #[serde(default)]
    pub features: FeatureFlags,

    /// Reconciliation configuration
    #[serde(default)]
    pub reconciliation: ReconciliationConfig,

    /// MongoDB configuration (optional, for configuration persistence)
    #[serde(default)]
    pub mongo: MongoConfig,

    /// Channel configuration (buffer sizes for internal communication)
    #[serde(default)]
    pub channels: ChannelConfig,
}

/// Venue selection for this process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueConfig {
    #[serde(default = "default_venue_mode_cex")]
    pub mode: VenueMode,
}

fn default_venue_mode_cex() -> VenueMode {
    VenueMode::Cex
}

impl Default for VenueConfig {
    fn default() -> Self {
        Self {
            mode: VenueMode::Cex,
        }
    }
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
    pub config: JsonValue,
}

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP server bind address
    pub bind_address: String,

    /// HTTP server port
    pub port: u16,
}

/// Redis configuration (optional, only needed if using Redis feature for Web UI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL (optional; may contain credentials)
    #[serde(default = "default_redis_url")]
    pub url: Secret<String>,

    /// Whether Redis is enabled (defaults to false - channels are used for internal communication)
    #[serde(default = "default_redis_enabled")]
    pub enabled: bool,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            enabled: default_redis_enabled(),
        }
    }
}

fn default_redis_url() -> Secret<String> {
    Secret::from("")
}

fn default_redis_enabled() -> bool {
    false
}

/// Feature flags for enabling/disabling system features
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeatureFlags {
    /// Enable reconciliation
    #[serde(default = "default_true")]
    pub enable_reconciliation: bool,
}

/// Reconciliation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationConfig {
    /// Exchange-level reconciliation interval in milliseconds (WS vs REST)
    /// Default: 180000 milliseconds (3 minutes)
    #[serde(default = "default_exchange_reconciliation_interval_ms")]
    pub exchange_level_interval_ms: u64,

    /// Trading-level reconciliation interval in milliseconds (TradingState vs ExchangeStateManager)
    /// Default: 180000 milliseconds (3 minutes)
    #[serde(default = "default_trading_reconciliation_interval_ms")]
    pub trading_level_interval_ms: u64,

    /// Whether reconciliation is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_exchange_reconciliation_interval_ms() -> u64 {
    180_000 // 3 minutes in milliseconds
}

fn default_trading_reconciliation_interval_ms() -> u64 {
    180_000 // 3 minutes in milliseconds
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self {
            exchange_level_interval_ms: default_exchange_reconciliation_interval_ms(),
            trading_level_interval_ms: default_trading_reconciliation_interval_ms(),
            enabled: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// MongoDB configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoConfig {
    /// MongoDB connection string (e.g., "mongodb://localhost:27017"; may contain credentials)
    #[serde(default = "default_mongo_connection_string")]
    pub connection_string: Secret<String>,

    /// MongoDB database name
    #[serde(default = "default_mongo_database")]
    pub database: String,

    /// Whether MongoDB persistence is enabled
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// Bot ID for this instance (used as collection identifier)
    #[serde(
        default = "default_bot_id",
        serialize_with = "trading_core::utils::serialize_smol_str",
        deserialize_with = "trading_core::utils::deserialize_smol_str"
    )]
    pub bot_id: smol_str::SmolStr,
}

fn default_mongo_connection_string() -> Secret<String> {
    Secret::from("mongodb://localhost:27017")
}

fn default_mongo_database() -> String {
    "trading_system".to_string()
}

fn default_false() -> bool {
    false
}

fn default_bot_id() -> smol_str::SmolStr {
    smol_str::SmolStr::new("binance_bot")
}

impl Default for MongoConfig {
    fn default() -> Self {
        Self {
            connection_string: default_mongo_connection_string(),
            database: default_mongo_database(),
            enabled: default_false(),
            bot_id: default_bot_id(),
        }
    }
}

// Re-export ChannelConfig from trading_core for convenience
pub use trading_core::config::ChannelConfig;

/// Strategy-specific configuration - trading parameters for strategies mode
/// Supports both single strategy (legacy) and multiple strategies (array)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StrategiesConfig {
    /// Single strategy format (for backward compatibility)
    Single(Box<StrategyConfig>),
    /// Multiple strategies format (array)
    Multiple { strategies: Vec<StrategyConfig> },
}

/// Individual strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy type: "BuyLowSellHigh", "MeanReversion", etc.
    #[serde(
        serialize_with = "trading_core::utils::serialize_smol_str",
        deserialize_with = "trading_core::utils::deserialize_smol_str"
    )]
    pub strategy_type: smol_str::SmolStr,

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

impl StrategiesConfig {
    /// Get all strategy configs as a vector
    pub fn into_vec(self) -> Vec<StrategyConfig> {
        match self {
            StrategiesConfig::Single(config) => vec![*config],
            StrategiesConfig::Multiple { strategies } => strategies,
        }
    }

    /// Get all strategy configs as a reference vector
    pub fn as_vec(&self) -> Vec<&StrategyConfig> {
        match self {
            StrategiesConfig::Single(config) => vec![config],
            StrategiesConfig::Multiple { strategies } => strategies.iter().collect(),
        }
    }
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
        if self.server.port == 0 {
            return Err("Server port must be greater than zero".to_string());
        }
        match self.venue.mode {
            VenueMode::Cex => {}
            VenueMode::Dex | VenueMode::Both => {
                if self.dex_engine.is_none() {
                    return Err(
                        "dex_engine config is required when venue.mode is \"dex\" or \"both\""
                            .to_string(),
                    );
                }
                if let Some(ref dex) = self.dex_engine {
                    if dex.chain == dex_engine::config::DexChain::Sui {
                        if dex.venue != trading_core::venue::VenueId::DeepBook {
                            return Err(
                                "dex_engine.venue must be \"deepbook\" when chain is \"sui\""
                                    .to_string(),
                            );
                        }
                        dex.deepbook_config()
                            .map_err(|e| format!("invalid dex_engine DeepBook config: {e}"))?;
                    }
                }
            }
        }
        if matches!(self.venue.mode, VenueMode::Cex | VenueMode::Both)
            && self.get_pairs().map_err(|e| e.to_string())?.is_empty()
        {
            return Err(
                "at least one trading pair is required in exchange_engine config when venue.mode is \"cex\" or \"both\"".to_string(),
            );
        }

        Ok(())
    }

    /// Validate `venue.mode` against runner `--mode`.
    pub fn validate_for_runner(&self, runner_mode: &str) -> Result<(), String> {
        match runner_mode {
            "levels" if self.venue.mode != VenueMode::Cex => {
                return Err(format!(
                    "runner --mode levels requires venue.mode \"cex\" (got {:?})",
                    self.venue.mode
                ));
            }
            "strategies" if self.venue.mode != VenueMode::Cex => {
                return Err(format!(
                    "runner --mode strategies requires venue.mode \"cex\" (got {:?})",
                    self.venue.mode
                ));
            }
            "dex" | "dex-strategies"
                if !matches!(self.venue.mode, VenueMode::Dex | VenueMode::Both) =>
            {
                return Err(format!(
                    "runner --mode {runner_mode} requires venue.mode \"dex\" or \"both\" (got {:?})",
                    self.venue.mode
                ));
            }
            "dex" => {
                if let Some(ref dex) = self.dex_engine {
                    dex.deepbook_config()
                        .map_err(|e| format!("invalid dex_engine DeepBook config: {e}"))?
                        .validate_for_trading()
                        .map_err(|e| format!("invalid dex_engine DeepBook config: {e}"))?;
                }
            }
            "dex-strategies" => {
                if self.dex_engine.is_none() {
                    return Err("dex-strategies mode requires dex_engine in bot config".into());
                }
            }
            "levels" | "strategies" => {}
            other => return Err(format!("unknown runner mode: {other}")),
        }
        Ok(())
    }

    /// Get pairs from exchange-specific config
    pub fn get_pairs(&self) -> Result<Vec<Pair>, Box<dyn std::error::Error>> {
        match self.exchange_engine.exchange {
            Exchange::Binance => {
                let binance_config: exchange_engine::exchanges::binance::config::BinanceConfig =
                    serde_json::from_value(self.exchange_engine.config.clone())
                        .map_err(|e| format!("Failed to deserialize Binance config: {e}"))?;
                Ok(binance_config.pairs)
            }
            Exchange::Bybit => {
                let bybit_config: exchange_engine::exchanges::bybit::config::BybitConfig =
                    serde_json::from_value(self.exchange_engine.config.clone())
                        .map_err(|e| format!("Failed to deserialize Bybit config: {e}"))?;
                Ok(bybit_config.pairs)
            }
            Exchange::Paper => {
                let paper_config: exchange_engine::exchanges::k_paper::PaperConfig =
                    serde_json::from_value(self.exchange_engine.config.clone())
                        .map_err(|e| format!("Failed to deserialize Paper config: {e}"))?;
                Ok(paper_config.pairs)
            }
            Exchange::Unsupported => Err("Unsupported exchange".into()),
            Exchange::DeepBook => Err("DeepBook pairs come from dex_engine.config.pools".into()),
        }
    }

    /// Get instrument type from exchange-specific config
    /// Defaults to Spot if not specified
    pub fn get_instrument_type(&self) -> Result<InstrumentType, Box<dyn std::error::Error>> {
        match self.exchange_engine.exchange {
            Exchange::Binance => Ok(InstrumentType::Spot), // Binance doesn't have instrument_type in config yet
            Exchange::Bybit => {
                let bybit_config: exchange_engine::exchanges::bybit::config::BybitConfig =
                    serde_json::from_value(self.exchange_engine.config.clone())
                        .map_err(|e| format!("Failed to deserialize Bybit config: {e}"))?;
                Ok(bybit_config.instrument_type)
            }
            Exchange::Paper => {
                let paper_config: exchange_engine::exchanges::k_paper::PaperConfig =
                    serde_json::from_value(self.exchange_engine.config.clone())
                        .map_err(|e| format!("Failed to deserialize Paper config: {e}"))?;
                Ok(paper_config.instrument_type)
            }
            Exchange::Unsupported => Ok(InstrumentType::Spot), // Default to Spot
            Exchange::DeepBook => Ok(InstrumentType::Spot),
        }
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
        let configs = self.as_vec();
        if configs.is_empty() {
            return Err("At least one strategy must be configured".to_string());
        }
        for (idx, config) in configs.iter().enumerate() {
            config
                .validate()
                .map_err(|e| format!("Strategy {}: {}", idx, e))?;
        }
        Ok(())
    }
}

impl StrategyConfig {
    /// Validate individual strategy configuration
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

#[cfg(test)]
mod config_smoke_tests {
    use super::*;

    fn example_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../config")
            .join(name)
    }

    #[test]
    fn bot_cex_example_validates() {
        let cfg = BotConfig::load_from_file(example_path("bot.json").to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        cfg.validate_for_runner("levels").unwrap();
    }

    #[test]
    fn bot_dex_example_validates() {
        let path = example_path("bot.dex.example.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        cfg.validate_for_runner("dex").unwrap();
        assert!(cfg.validate_for_runner("levels").is_err());
    }

    #[test]
    fn bot_both_example_validates() {
        let path = example_path("bot.both.example.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        cfg.validate_for_runner("dex").unwrap();
    }
}
