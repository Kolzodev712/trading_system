// General level data abstractions:

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct OrderBook {
    pub bids: Vec<Decimal>,
    pub asks: Vec<Decimal>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct Trade {
    pub price: Decimal,
    pub quantity: Decimal,
    pub time: u64,
    pub is_market_maker: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
}
