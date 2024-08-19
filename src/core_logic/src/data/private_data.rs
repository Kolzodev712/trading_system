// A general level data abstraction for External data types, using in the external system logic for authenticated requests.
use super::base_data::{Instrument, OrderId, OrderType, Side};
use crate::utils::{deserialize_smol_str, serialize_smol_str};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Account {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub account_type: SmolStr,
    pub update_time: u64,
    pub uid: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    /// Pair where to open the order
    pub instrument: Instrument,
    /// Side of the book to place the order
    pub side: Side,
    /// Price at which the order will be placed
    pub price: Decimal,
    /// Order size
    pub amount: Decimal,
    /// The order type: limit, post-only, IOC or FOK
    pub order_type: OrderType,
    /// Timestamp of the order placement
    pub timestamp: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// Exchange's order ID
    pub order_id: OrderId,
    /// Pair where to open the order
    pub instrument: Instrument,
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

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub symbol: Instrument,
    pub free: Decimal,
    pub used: Decimal,
    pub total: Decimal,
    pub last_updated: u64,
}
