// Structs and enums:

use core_logic::{
    data::public_data::{Orderbook, Trade},
    utils::{
        deserialize_smol_str, deserialize_vec_smol_str, serialize_smol_str, serialize_vec_smol_str,
    },
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::primitive::str;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub method: SmolStr,
    #[serde(
        serialize_with = "serialize_vec_smol_str",
        deserialize_with = "deserialize_vec_smol_str"
    )]
    pub params: Vec<SmolStr>,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceOrderbook {
    #[serde(
        rename = "e",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub event_type: SmolStr,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(
        rename = "s",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<PriceLevel>,
    #[serde(rename = "a")]
    pub asks: Vec<PriceLevel>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

impl From<BinanceOrderbook> for Orderbook {
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
        Orderbook { bids, asks }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceTrade {
    #[serde(
        rename = "e",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub event_type: SmolStr,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(
        rename = "s",
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(rename = "t")]
    pub trade_id: u64,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "q")]
    pub quantity: Decimal,
    #[serde(rename = "T")]
    pub trade_time: u64,
    #[serde(rename = "m")]
    pub is_market_maker: bool,
}

impl From<BinanceTrade> for Trade {
    fn from(trade: BinanceTrade) -> Self {
        Trade {
            price: trade.price,
            quantity: trade.quantity,
            time: trade.trade_time,
            is_market_maker: trade.is_market_maker,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceWsEventType {
    #[serde(rename = "depthUpdate")]
    DepthUpdate,
    #[serde(rename = "trade")]
    Trade,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    OrderBook(Orderbook),
    Trade(Trade),
    Raw(
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        SmolStr,
    ),
}
