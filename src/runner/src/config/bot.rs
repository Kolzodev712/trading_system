//! Configuration types and loading for the trading system
//! This is in the runner crate since it's the orchestrator that loads and uses configs
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use trading_core::secret::Secret;
use trading_core::types::common::{Exchange, InstrumentType, Pair};
use trading_core::venue::VenueMode;

/// Error text when a DEX venue mode is requested in a build without `feature = "dex"`.
#[cfg(not(feature = "dex"))]
pub const DEX_NOT_ENABLED_MSG: &str = "DEX support not enabled; build with --features dex";

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

    /// DEX engine configuration (required when `venue.mode` is `dex`).
    /// Only present with `feature = "dex"`; otherwise a `dex_engine` JSON key is ignored.
    #[cfg(feature = "dex")]
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

    /// Shadow vs exchange order permission. Default exchange keeps existing bots unchanged.
    #[serde(default)]
    pub execution_mode: trading_core::types::ExecutionMode,

    /// Append-only shadow journal. Runtime data; not committed.
    #[serde(default = "default_shadow_journal_path")]
    pub shadow_journal_path: String,

    /// Append-only unknown-submission journal. Runtime data; not committed.
    #[serde(default = "default_submission_journal_path")]
    pub submission_journal_path: String,

    /// Append-only execution ledger for fills, fees, and funding. Runtime data.
    #[serde(default = "default_execution_ledger_path")]
    pub execution_ledger_path: String,

    /// Process-local enablement snapshot. Runtime data; not position truth.
    #[serde(default = "default_runtime_snapshot_path")]
    pub runtime_snapshot_path: String,

    /// RequestRestart flag file. Runtime data.
    #[serde(default = "default_restart_flag_path")]
    pub restart_flag_path: String,
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

fn default_shadow_journal_path() -> String {
    "runtime-data/shadow-journal.jsonl".to_string()
}

fn default_submission_journal_path() -> String {
    "runtime-data/submission-journal.jsonl".to_string()
}

fn default_execution_ledger_path() -> String {
    "runtime-data/execution-ledger.jsonl".to_string()
}

fn default_runtime_snapshot_path() -> String {
    crate::config::runtime_snapshot::SNAPSHOT_FILE.to_string()
}

fn default_restart_flag_path() -> String {
    crate::config::runtime_snapshot::RESTART_FLAG_FILE.to_string()
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

    /// gRPC listen address. Default preserves historical `[::]:50051`.
    #[serde(default = "default_grpc_bind")]
    pub grpc_bind: String,
}

fn default_grpc_bind() -> String {
    "[::]:50051".to_string()
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
    Multiple {
        strategies: Vec<StrategyConfig>,
        #[serde(default)]
        risk_limits: Option<trading_engine::risk::RiskLimits>,
    },
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

    /// Optional lookback override for strategies that use a window.
    #[serde(default)]
    pub lookback: Option<usize>,

    /// Fresh-entry window in seconds. Required for a flat account to enter.
    #[serde(default)]
    pub max_entry_age_secs: Option<u64>,

    /// Enable after construction when no runtime snapshot entry exists for this strategy.
    /// A persisted snapshot `enabled` value overrides this on restore.
    #[serde(default)]
    pub initially_enabled: bool,

    /// Max age of the last orderbook update before exposure increases are blocked.
    /// Required for Exchange + futures. No mainnet default is invented.
    #[serde(default)]
    pub max_market_data_age_ms: Option<u64>,
}

impl StrategiesConfig {
    /// Account-level limits for strategy mode. Absent means those checks are not configured.
    pub fn risk_limits(&self) -> trading_engine::risk::RiskLimits {
        match self {
            StrategiesConfig::Multiple {
                risk_limits: Some(limits),
                ..
            } => limits.clone(),
            _ => trading_engine::risk::RiskLimits::default(),
        }
    }

    /// Get all strategy configs as a vector
    pub fn into_vec(self) -> Vec<StrategyConfig> {
        match self {
            StrategiesConfig::Single(config) => vec![*config],
            StrategiesConfig::Multiple { strategies, .. } => strategies,
        }
    }

    /// Get all strategy configs as a reference vector
    pub fn as_vec(&self) -> Vec<&StrategyConfig> {
        match self {
            StrategiesConfig::Single(config) => vec![config],
            StrategiesConfig::Multiple { strategies, .. } => strategies.iter().collect(),
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

    /// gRPC listen address from `server.grpc_bind` (allocated by the workflow harness).
    pub fn grpc_listen_addr(&self) -> Result<std::net::SocketAddr, String> {
        self.server
            .grpc_bind
            .parse()
            .map_err(|e| format!("invalid server.grpc_bind '{}': {e}", self.server.grpc_bind))
    }

    /// Validate bot configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.server.port == 0 {
            return Err("Server port must be greater than zero".to_string());
        }
        match self.venue.mode {
            VenueMode::Cex => {}
            #[cfg(not(feature = "dex"))]
            VenueMode::Dex | VenueMode::Both => {
                return Err(DEX_NOT_ENABLED_MSG.to_string());
            }
            #[cfg(feature = "dex")]
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
            #[cfg(not(feature = "dex"))]
            "dex" | "dex-strategies" => {
                return Err(DEX_NOT_ENABLED_MSG.to_string());
            }
            #[cfg(feature = "dex")]
            "dex" | "dex-strategies"
                if !matches!(self.venue.mode, VenueMode::Dex | VenueMode::Both) =>
            {
                return Err(format!(
                    "runner --mode {runner_mode} requires venue.mode \"dex\" or \"both\" (got {:?})",
                    self.venue.mode
                ));
            }
            #[cfg(feature = "dex")]
            "dex" => {
                if let Some(ref dex) = self.dex_engine {
                    dex.deepbook_config()
                        .map_err(|e| format!("invalid dex_engine DeepBook config: {e}"))?
                        .validate_for_trading()
                        .map_err(|e| format!("invalid dex_engine DeepBook config: {e}"))?;
                }
            }
            #[cfg(feature = "dex")]
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

    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    pub fn find_mut(&mut self, strategy_type: &str, symbol: &Pair) -> Option<&mut StrategyConfig> {
        self.as_vec_mut().into_iter().find(|c| {
            c.strategy_type.as_str() == strategy_type
                && c.symbol.base.as_str() == symbol.base.as_str()
                && c.symbol.quote.as_str() == symbol.quote.as_str()
        })
    }

    /// Prefer type+symbol, then symbol only (type swap from the UI).
    pub fn find_mut_for_operator(
        &mut self,
        strategy_type: &str,
        symbol: &Pair,
    ) -> Option<&mut StrategyConfig> {
        let pos = {
            let rows = self.as_vec();
            let exact = rows.iter().position(|c| {
                c.strategy_type.as_str() == strategy_type
                    && c.symbol.base.as_str() == symbol.base.as_str()
                    && c.symbol.quote.as_str() == symbol.quote.as_str()
            });
            exact.or_else(|| {
                rows.iter().position(|c| {
                    c.symbol.base.as_str() == symbol.base.as_str()
                        && c.symbol.quote.as_str() == symbol.quote.as_str()
                })
            })
        }?;
        self.as_vec_mut().into_iter().nth(pos)
    }

    fn as_vec_mut(&mut self) -> Vec<&mut StrategyConfig> {
        match self {
            StrategiesConfig::Single(config) => vec![config.as_mut()],
            StrategiesConfig::Multiple { strategies, .. } => strategies.iter_mut().collect(),
        }
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
        let mut seen = std::collections::HashSet::new();
        for config in configs {
            let key = (
                config.symbol.base.to_string(),
                config.symbol.quote.to_string(),
            );
            if !seen.insert(key) {
                return Err(format!(
                    "second executing strategy on {} rejected",
                    config.symbol
                ));
            }
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

    #[cfg(not(feature = "dex"))]
    #[test]
    fn dex_venue_modes_rejected_without_dex_feature() {
        for example in ["bot.dex.example.json", "bot.both.example.json"] {
            let cfg = BotConfig::load_from_file(example_path(example).to_str().unwrap()).unwrap();
            assert_eq!(cfg.validate().unwrap_err(), DEX_NOT_ENABLED_MSG);
            assert_eq!(
                cfg.validate_for_runner("dex").unwrap_err(),
                DEX_NOT_ENABLED_MSG
            );
        }
    }

    #[cfg(feature = "dex")]
    #[test]
    fn bot_dex_example_validates() {
        let path = example_path("bot.dex.example.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        cfg.validate_for_runner("dex").unwrap();
        assert!(cfg.validate_for_runner("levels").is_err());
    }

    #[cfg(feature = "dex")]
    #[test]
    fn bot_both_example_validates() {
        let path = example_path("bot.both.example.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        cfg.validate_for_runner("dex").unwrap();
    }

    #[test]
    fn bot_bybit_shadow_sample_is_shadow_mode() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample_config/bybit-shadow/bot.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        assert_eq!(
            cfg.execution_mode,
            trading_core::types::ExecutionMode::Shadow
        );
        let strategies = StrategiesConfig::load_from_file(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../sample_config/bybit-shadow/strategies.json")
                .to_str()
                .unwrap(),
        )
        .unwrap();
        strategies.validate().unwrap();
        let configs = strategies.as_vec();
        assert_eq!(configs[0].strategy_type.as_str(), "BTCPriceMALongFlatV1");
        assert_eq!(configs[0].lookback, Some(28));
        assert!(configs[0].initially_enabled);
        let (mode, venue, environment) = crate::config::execution_status_from_bot(&cfg);
        assert_eq!(mode, trading_core::types::ExecutionMode::Shadow);
        assert_eq!(venue, "BYBIT");
        assert_eq!(environment, "MAINNET");
    }

    #[test]
    fn bot_bybit_demo_sample_is_demo_exchange_btc_usdt() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample_config/bybit-demo/bot.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        assert_eq!(
            cfg.execution_mode,
            trading_core::types::ExecutionMode::Exchange
        );
        assert!(cfg.submission_journal_path.contains("bybit-demo"));
        assert!(cfg.execution_ledger_path.contains("bybit-demo"));
        assert!(cfg.runtime_snapshot_path.contains("bybit-demo"));
        let demo = cfg
            .exchange_engine
            .config
            .get("demo")
            .and_then(|v| v.as_bool());
        let testnet = cfg
            .exchange_engine
            .config
            .get("testnet")
            .and_then(|v| v.as_bool());
        assert_eq!(demo, Some(true));
        assert_eq!(testnet, Some(false));
        let strategies = StrategiesConfig::load_from_file(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../sample_config/bybit-demo/strategies.json")
                .to_str()
                .unwrap(),
        )
        .unwrap();
        strategies.validate().unwrap();
        strategies
            .risk_limits()
            .require_explicit_for_exchange(cfg.execution_mode)
            .unwrap();
        let configs = strategies.as_vec();
        assert_eq!(configs[0].strategy_type.as_str(), "BTCPriceMALongFlatV1");
        assert_eq!(configs[0].lookback, Some(28));
        assert_eq!(configs[0].max_entry_age_secs, Some(86_400));
        assert_eq!(configs[0].max_market_data_age_ms, Some(15_000));
        assert_eq!(configs[0].symbol, Pair::new("BTC", "USDT"));
        let (mode, venue, environment) = crate::config::execution_status_from_bot(&cfg);
        crate::config::operator_labels::exchange_live_is_authorized(mode, &venue, &environment)
            .unwrap();
        assert_eq!(mode, trading_core::types::ExecutionMode::Exchange);
        assert_eq!(venue, "BYBIT");
        assert_eq!(environment, "DEMO");
    }

    #[test]
    fn kpaper_sample_is_paper_local_exchange() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample_config/k-paper/bot.json");
        let cfg = BotConfig::load_from_file(path.to_str().unwrap()).unwrap();
        cfg.validate().unwrap();
        let (mode, venue, environment) = crate::config::execution_status_from_bot(&cfg);
        assert_eq!(mode, trading_core::types::ExecutionMode::Exchange);
        assert_eq!(venue, "PAPER");
        assert_eq!(environment, "LOCAL");
    }

    #[test]
    fn second_executing_strategy_on_same_pair_is_rejected() {
        let json = r#"{
            "strategies": [
                {
                    "strategy_type": "A",
                    "symbol": {"base": "BTC", "quote": "USDT"},
                    "position_size": "0.001",
                    "max_position": "0.001",
                    "buy_threshold_percent": "0.01",
                    "sell_threshold_percent": "0.01",
                    "max_active_orders": 1,
                    "price_history_size": 2,
                    "min_price_history": 1
                },
                {
                    "strategy_type": "B",
                    "symbol": {"base": "BTC", "quote": "USDT"},
                    "position_size": "0.001",
                    "max_position": "0.001",
                    "buy_threshold_percent": "0.01",
                    "sell_threshold_percent": "0.01",
                    "max_active_orders": 1,
                    "price_history_size": 2,
                    "min_price_history": 1
                }
            ]
        }"#;
        let cfg: StrategiesConfig = serde_json::from_str(json).unwrap();
        let err = cfg.validate().expect_err("duplicate pair must fail");
        assert!(err.contains("second executing strategy"));
    }

    #[cfg(feature = "dex")]
    #[test]
    fn dex_example_labels_deepbook_testnet() {
        let cfg = BotConfig::load_from_file(example_path("bot.dex.example.json").to_str().unwrap())
            .unwrap();
        let (mode, venue, environment) = crate::config::execution_status_from_bot(&cfg);
        assert_eq!(mode, trading_core::types::ExecutionMode::Exchange);
        assert_eq!(venue, "DEEPBOOK");
        assert_eq!(environment, "TESTNET");
    }
}
