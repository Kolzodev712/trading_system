//! Level field types that can be updated
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::utils::{deserialize_smol_str, serialize_smol_str};

/// Fields that can be updated in a level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LevelField {
    /// Static price (overrides indicator-based price)
    Price,

    /// Static quantity (overrides indicator-based quantity)
    Quantity,

    /// Stop-loss price
    StopLoss,

    /// Take-profit price
    TakeProfit,

    /// Risk limit for this level
    RiskLimit,

    /// Maximum position size for this level
    MaxPositionSize,

    /// Enable/disable the level
    Enabled,

    /// Repost on fill (true/false)
    RepostOnFill,

    /// Repost on cancel (true/false)
    RepostOnCancel,
}

/// Value types for level fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LevelValue {
    Decimal(Decimal),
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    String(SmolStr),
    Boolean(bool),
    Integer(i64),
}

impl LevelValue {
    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            LevelValue::Decimal(d) => Some(*d),
            LevelValue::Integer(i) => Some(Decimal::from(*i)),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            LevelValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<SmolStr> {
        match self {
            LevelValue::String(s) => Some(s.clone()),
            _ => None,
        }
    }
}

/// Risk limit types that can be updated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLimitType {
    /// Total maximum position size across all levels
    TotalMaxPosition,

    /// Maximum total number of orders
    MaxTotalOrders,

    /// Daily loss limit for all pairs
    DailyLossLimitAllPairs,

    /// Daily loss limit for a specific pair
    DailyLossLimitPerPair,
}
