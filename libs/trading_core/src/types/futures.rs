//! Futures trading types and enums
use crate::types::common::Pair;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Position mode (one-way vs hedge mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionMode {
    /// One-way mode: only one position per symbol (long or short)
    OneWay,
    /// Hedge mode: can have both long and short positions simultaneously
    Hedge,
}

/// Margin type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarginType {
    /// Isolated margin: margin is isolated per position
    Isolated,
    /// Cross margin: margin is shared across all positions
    Cross,
}

/// Position side (for hedge mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionSide {
    /// Both sides (one-way mode)
    Both,
    /// Long side only (hedge mode)
    Long,
    /// Short side only (hedge mode)
    Short,
}

/// Working type for stop orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkingType {
    /// Mark price
    MarkPrice,
    /// Contract price (last traded price)
    ContractPrice,
}

/// Futures account collateral/balance
/// Note: Separate from Balance because futures balances have fundamentally different semantics
/// (collateral accounts vs asset holdings)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesBalance {
    /// Asset symbol (e.g., "USDT")
    pub asset: crate::types::common::Symbol,
    /// Available balance (can be used for new positions)
    pub available_balance: Decimal,
    /// Wallet balance (total balance including unrealized P&L)
    pub wallet_balance: Decimal,
    /// Cross wallet balance (for cross margin mode)
    pub cross_wallet_balance: Decimal,
    /// Cross unrealized P&L (for cross margin mode)
    pub cross_unrealized_pnl: Decimal,
    /// Available for withdrawal
    pub available_withdraw: Decimal,
    /// Last update timestamp
    pub last_update: u64,
}

/// Margin information for a position
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginInfo {
    /// Initial margin required
    pub initial_margin: Decimal,
    /// Maintenance margin required
    pub maintenance_margin: Decimal,
    /// Margin ratio (maintenance / position value)
    pub margin_ratio: Decimal,
    /// Isolated margin (for isolated margin mode)
    pub isolated_margin: Option<Decimal>,
}

/// Leverage information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeverageInfo {
    /// Symbol/pair
    pub pair: Pair,
    /// Current leverage (e.g., 10 = 10x)
    pub leverage: u32,
    /// Maximum leverage allowed
    pub max_leverage: u32,
    /// Last update timestamp
    pub last_update: u64,
}

/// Funding rate information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    /// Trading pair
    pub pair: Pair,
    /// Current funding rate (e.g., 0.0001 = 0.01%)
    pub rate: Decimal,
    /// Next funding time (timestamp)
    pub next_funding_time: u64,
    /// Last funding time (timestamp)
    pub last_funding_time: u64,
    /// Mark price at last funding
    pub mark_price: Decimal,
    /// Index price at last funding
    pub index_price: Decimal,
}

/// Futures-specific order request
/// Extends OrderRequest with futures-specific fields
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesOrderRequest {
    /// Base order request fields
    #[serde(flatten)]
    pub base: crate::types::exchange::OrderRequest,
    /// Position side (for hedge mode: Both, Long, Short)
    pub position_side: Option<PositionSide>,
    /// Reduce-only flag
    pub reduce_only: Option<bool>,
    /// Close position flag (close entire position)
    pub close_position: Option<bool>,
    /// Stop price (for stop orders)
    pub stop_price: Option<Decimal>,
    /// Activation price (for trailing stop)
    pub activation_price: Option<Decimal>,
    /// Callback rate (for trailing stop)
    pub callback_rate: Option<Decimal>,
    /// Working type (Mark price or Contract price for stop orders)
    pub working_type: Option<WorkingType>,
    /// Price protect (protect against adverse price movements)
    pub price_protect: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_position_mode_serialization() {
        let mode = PositionMode::OneWay;
        let serialized = serde_json::to_string(&mode).unwrap();
        assert_eq!(serialized, "\"oneWay\"");

        let deserialized: PositionMode = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, PositionMode::OneWay);
    }

    #[test]
    fn test_margin_type_serialization() {
        let margin_type = MarginType::Isolated;
        let serialized = serde_json::to_string(&margin_type).unwrap();
        assert_eq!(serialized, "\"isolated\"");

        let deserialized: MarginType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, MarginType::Isolated);
    }

    #[test]
    fn test_futures_balance() {
        let balance = FuturesBalance {
            asset: "USDT".into(),
            available_balance: Decimal::new(1000, 0),
            wallet_balance: Decimal::new(1000, 0),
            cross_wallet_balance: Decimal::new(1000, 0),
            cross_unrealized_pnl: Decimal::ZERO,
            available_withdraw: Decimal::new(1000, 0),
            last_update: 1000000,
        };

        let serialized = serde_json::to_string(&balance).unwrap();
        let deserialized: FuturesBalance = serde_json::from_str(&serialized).unwrap();
        assert_eq!(balance, deserialized);
    }

    #[test]
    fn test_leverage_info() {
        let leverage = LeverageInfo {
            pair: Pair::new("BTC", "USDT"),
            leverage: 10,
            max_leverage: 125,
            last_update: 1000000,
        };

        let serialized = serde_json::to_string(&leverage).unwrap();
        let deserialized: LeverageInfo = serde_json::from_str(&serialized).unwrap();
        assert_eq!(leverage, deserialized);
    }
}
