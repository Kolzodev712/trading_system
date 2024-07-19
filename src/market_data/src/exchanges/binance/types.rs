// Structs and enums:

use core_logic::data::OrderBook;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::primitive::str;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
    pub method: String,
    pub params: Vec<String>,
    pub id: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceOrderbook {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<PriceLevel>,
    #[serde(rename = "a")]
    pub asks: Vec<PriceLevel>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

impl From<BinanceOrderbook> for OrderBook {
    fn from(orderbook: BinanceOrderbook) -> Self {
        let bids = orderbook
            .bids
            .into_iter()
            .map(|level| level.price)
            .collect();
        let asks = orderbook
            .asks
            .into_iter()
            .map(|level| level.price)
            .collect();
        OrderBook { bids, asks }
    }
}
