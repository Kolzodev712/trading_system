//! Level configuration types
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::types::common::{OrderType, Side};

/// Level configuration - exchange-agnostic and pair-agnostic
/// Levels are applied to pairs from the bot configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LevelConfig {
    /// Unique identifier for this level (template, will be expanded per pair)
    pub id: SmolStr,

    /// Level name/description
    pub name: Option<SmolStr>,

    /// Side: Buy (bid) or Sell (ask)
    pub side: Side,

    /// Order type configuration
    pub order_type: OrderTypeConfig,

    /// Quantity configuration (static or indicator-based)
    pub quantity: QuantityConfig,

    /// Price configuration (static or indicator-based)
    pub price: PriceConfig,

    /// Risk parameters for this level
    pub risk: LevelRiskConfig,

    /// Lifecycle rules
    pub lifecycle: LifecycleConfig,

    /// Whether level is enabled
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// Level group (for management)
    pub group: Option<SmolStr>,
}

/// Order type configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderTypeConfig {
    Market,
    Limit,
    PostOnly,
    IoC, // Immediate or Cancel
    FoK, // Fill or Kill
}

impl From<OrderTypeConfig> for OrderType {
    fn from(config: OrderTypeConfig) -> Self {
        match config {
            OrderTypeConfig::Market => OrderType::Market,
            OrderTypeConfig::Limit => OrderType::Limit,
            OrderTypeConfig::PostOnly => OrderType::PostOnly,
            OrderTypeConfig::IoC => OrderType::Ioc,
            OrderTypeConfig::FoK => OrderType::Fok,
        }
    }
}

/// Quantity configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum QuantityConfig {
    /// Fixed quantity
    Fixed(Decimal),

    /// Percentage of available balance
    BalancePercent(Decimal),

    /// Fixed notional value (price * quantity = notional)
    Notional(Decimal),
}

/// Price configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PriceConfig {
    /// Fixed price
    Fixed(Decimal),

    /// Price from indicator config binding for this level template id (`config.id`).
    /// See `config/indicators.json` → `level_pricing` or MongoDB `indicators` collection.
    Binding,

    /// Best bid with offset
    BestBid { offset: Decimal },

    /// Best ask with offset
    BestAsk { offset: Decimal },

    /// Mid price with offset
    MidPrice { offset_percent: Decimal },
}

/// Price offset type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceOffsetType {
    /// Absolute offset (add/subtract absolute value)
    Absolute,

    /// Percentage offset (add/subtract percentage)
    Percent,

    /// X% below indicator
    PercentBelow,

    /// X% above indicator
    PercentAbove,
}

/// Level risk configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LevelRiskConfig {
    /// Maximum position size for this level
    pub max_position: Option<Decimal>,

    /// Maximum number of orders for this level
    pub max_orders: Option<usize>,

    /// Daily loss limit for this level
    pub daily_loss_limit: Option<Decimal>,
}

/// Lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifecycleConfig {
    /// Auto-repost after fill (default: true)
    #[serde(default = "default_true")]
    pub auto_repost: bool,

    /// Auto-repost after cancellation (default: true)
    #[serde(default = "default_true")]
    pub auto_repost_on_cancel: bool,

    /// Maximum reposts before stopping
    pub max_reposts: Option<usize>,

    /// Delay before reposting (seconds)
    pub repost_delay_secs: Option<u64>,

    /// Conditions that prevent reposting
    #[serde(default)]
    pub repost_conditions: Vec<RepostCondition>,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            auto_repost: true,
            auto_repost_on_cancel: true,
            max_reposts: None,
            repost_delay_secs: None,
            repost_conditions: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

/// Repost condition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RepostCondition {
    /// Don't repost if daily loss limit reached
    DailyLossLimitReached,

    /// Don't repost if position limit reached
    PositionLimitReached,

    /// Don't repost if balance insufficient
    InsufficientBalance,

    /// Don't repost if P&L below threshold
    PnLBelowThreshold(Decimal),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_level_config_serialization() {
        let config = LevelConfig {
            id: SmolStr::new("test_level"),
            name: Some(SmolStr::new("Test Level")),
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Binding,
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: Some(SmolStr::new("main")),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LevelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }
}
