//! Exchange-related types (orders, balances, positions, trades)
use crate::types::common::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Order request
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    /// Symbol for the open order response
    pub symbol: Symbol,
    /// Side of the book to place the order
    pub side: Side,
    /// Type of the order to place
    pub order_type: OrderType,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size. This is the full, original order amount. Not the remaining amount.
    pub amount: Decimal,
    /// Exchange's timestamp of the order
    pub timestamp: Option<u64>,
    /// Custom params specific to exchanges:
    pub exchange_params: HashMap<String, String>,
}

/// Order
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// Exchange's order ID
    pub order_id: OrderId,
    /// Symbol for the open order response
    pub symbol: Symbol,
    /// Side of the book to place the order
    pub side: Side,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size. This is the full, original order amount. Not the remaining amount.
    pub amount: Decimal,
    /// Internal timestamp at which we received the response of the open order
    pub internal_created_at: u64,
    /// Exchange's timestamp of the order
    pub exchange_created_at: Option<u64>,
    /// Order status (New, Filled, Canceled, etc.)
    #[serde(default = "default_order_status")]
    pub status: OrderStatus,
    /// Filled amount (for partial fills)
    #[serde(default)]
    pub filled_amount: Decimal,
}

fn default_order_status() -> OrderStatus {
    OrderStatus::New
}

/// Account information
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    /// The balances of the account
    pub balances: Vec<Balance>,
    /// The account type
    pub account_type: AccountType,
    /// Unix millisecond timestamp when an update was received from the exchange
    pub update_time: MillisecondTimestamp,
}

/// Balance
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    /// Currency symbol
    pub symbol: Symbol,
    /// The free balance that can be used in new orders
    pub free: Decimal,
    /// The used balance that is bound to open orders
    pub used: Decimal,
    /// The total amount of balance available which should be the sum of [`Balance::free`] and [`Balance::used`]
    pub total: Decimal,
}

/// Position
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Trading pair
    pub pair: Pair,
    /// Position size (positive for long, negative for short)
    pub quantity: Decimal,
    /// Average entry price
    pub avg_price: Decimal,
    /// Current market price
    pub current_price: Decimal,
    /// Unrealized P&L
    pub unrealized_pnl: Decimal,
    /// Last update timestamp
    pub last_update: u64,
}

/// Trade (private trade from execution reports)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    /// Trade ID
    pub trade_id: String,
    /// Order ID that this trade belongs to
    pub order_id: OrderId,
    /// Trading pair
    pub pair: Pair,
    /// Trade price
    pub price: Decimal,
    /// Trade quantity
    pub quantity: Decimal,
    /// Trade side
    pub side: Side,
    /// Trade timestamp
    pub timestamp: u64,
    /// Fee amount
    pub fee: Decimal,
    /// Fee currency
    pub fee_currency: Symbol,
}
