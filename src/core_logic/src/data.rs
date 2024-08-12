// General level data abstractions:

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

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
pub struct Account {
    pub account_type: String,
    pub update_time: u64,
    pub uid: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
}
