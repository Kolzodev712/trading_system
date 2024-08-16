// A general level data abstraction for External data types, using in the external system logic for non-authenticated requests.
use super::base_data::Instrument;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// Market data structs:
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct OrderBook {
    pub bids: Vec<Decimal>,
    pub asks: Vec<Decimal>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Trade {
    pub price: Decimal,
    pub quantity: Decimal,
    pub time: u64,
    pub is_market_maker: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Balance {
    pub symbol: Instrument,
    pub free: Decimal,
    pub used: Decimal,
    pub total: Decimal,
    pub last_updated: u64,
}
