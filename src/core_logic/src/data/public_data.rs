// A general level data abstraction for External data types, using in the external system logic for non-authenticated requests.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// Market data structs:
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderBook {
    pub bids: Vec<Decimal>,
    pub asks: Vec<Decimal>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub price: Decimal,
    pub quantity: Decimal,
    pub time: u64,
    pub is_market_maker: bool,
}
