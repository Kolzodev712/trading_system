// A general level data abstraction for External data types, using in the external system logic for authenticated requests.
use super::base_data::{Instrument, OrderId, OrderType, Pair, Side};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Account {
    pub account_type: String,
    pub update_time: u64,
    pub uid: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct OrderRequest {
    /// Pair where to open the order
    pub pair: Pair,
    /// Side of the book to place the order
    pub side: Side,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size
    pub amount: Decimal,
    /// The order type: limit, post-only, IOC or FOK
    pub order_type: OrderType,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Order {
    /// Exchange's order ID
    pub order_id: OrderId,
    /// Pair where to open the order
    pub pair: Instrument,
    /// Side of the book to place the order
    pub side: Side,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size. This is the full, original order amount. Not the remaining amount.
    pub amount: Decimal,
    /// Internal timestamp at which we received the response of the open order
    pub internal_created_at: DateTime<Utc>,
    /// Exchange's timestamp of the order
    pub exchange_created_at: Option<DateTime<Utc>>,
}
