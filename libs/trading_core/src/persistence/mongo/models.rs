//! MongoDB document models
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Trading bot configuration document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfigDocument {
    #[serde(rename = "_id")]
    pub bot_id: String,

    /// Levels configuration
    pub levels: Vec<LevelConfigDocument>,

    /// Risk limits
    pub risk_limits: RiskLimitsDocument,

    /// Trading configuration
    pub trading_config: TradingConfigDocument,

    /// Last updated timestamp
    pub updated_at: u64,
}

/// Level configuration document (simplified for MongoDB)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelConfigDocument {
    pub id: String,
    pub name: Option<String>,
    pub symbol: PairDocument,
    pub side: String,                // "Buy" or "Sell"
    pub order_type: String,          // "Market", "Limit", etc.
    pub quantity: serde_json::Value, // QuantityConfig as JSON
    pub price: serde_json::Value,    // PriceConfig as JSON
    pub risk: LevelRiskConfigDocument,
    pub lifecycle: LifecycleConfigDocument,
    pub enabled: bool,
    pub group: Option<String>,
}

/// Pair document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairDocument {
    pub base: String,
    pub quote: String,
}

impl From<crate::types::common::Pair> for PairDocument {
    fn from(pair: crate::types::common::Pair) -> Self {
        Self {
            base: pair.base.to_string(),
            quote: pair.quote.to_string(),
        }
    }
}

impl From<PairDocument> for crate::types::common::Pair {
    fn from(doc: PairDocument) -> Self {
        Self::new(&doc.base, &doc.quote)
    }
}

/// Level risk configuration document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelRiskConfigDocument {
    pub max_position: Option<Decimal>,
    pub max_orders: Option<usize>,
    pub daily_loss_limit: Option<Decimal>,
}

/// Lifecycle configuration document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfigDocument {
    pub auto_repost: bool,
    pub auto_repost_on_cancel: bool,
    pub max_reposts: Option<usize>,
    pub repost_delay_secs: Option<u64>,
}

/// Risk limits document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimitsDocument {
    pub total_max_position: Option<Decimal>,
    pub max_total_orders: Option<usize>,
    pub daily_loss_limit_all_pairs: Option<Decimal>,
    pub daily_loss_limit_per_pair: std::collections::HashMap<String, Decimal>,
}

/// Trading configuration document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfigDocument {
    pub configured_pairs: Vec<String>,
}

// Note: State persistence (ExchangeStateDocument, MarketDataStateDocument, etc.)
// is not implemented at this time. Only configuration persistence is implemented.

/// Simple indicator document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleIndicatorDocument {
    #[serde(rename = "_id")]
    pub indicator_id: String,

    pub indicator_type: String, // "simple"
    pub name: String,
    pub pair: String,
    pub period: u32,
    pub source: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Composite indicator document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeIndicatorDocument {
    #[serde(rename = "_id")]
    pub indicator_id: String,

    pub indicator_type: String, // "composite"
    pub name: String,
    pub pair: String,
    pub expression: serde_json::Value, // Formula expression
    pub dependencies: Vec<String>,     // Dependent indicator IDs
    pub created_at: u64,
    pub updated_at: u64,
}

/// Strategy configuration document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfigDocument {
    #[serde(rename = "_id")]
    pub strategy_id: String,

    pub strategy_type: String, // "BuyLowSellHigh", etc.
    pub symbol: PairDocument,
    pub position_size: rust_decimal::Decimal,
    pub max_position: rust_decimal::Decimal,
    pub buy_threshold_percent: rust_decimal::Decimal,
    pub sell_threshold_percent: rust_decimal::Decimal,
    pub stop_loss_percent: Option<rust_decimal::Decimal>,
    pub take_profit_percent: Option<rust_decimal::Decimal>,
    pub max_active_orders: usize,
    pub price_history_size: usize,
    pub min_price_history: usize,
    pub min_order_size: Option<rust_decimal::Decimal>,
    pub step_size: Option<rust_decimal::Decimal>,
    pub tick_size: Option<rust_decimal::Decimal>,
    pub min_notional: Option<rust_decimal::Decimal>,
    pub created_at: u64,
    pub updated_at: u64,
}
