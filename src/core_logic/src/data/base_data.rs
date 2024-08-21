// A general level data abstraction for Internal data types, using in the internal system logic.
use crate::utils::{deserialize_smol_str, serialize_smol_str};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Pair {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub base: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub quote: SmolStr,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Instrument {
    pub instrument_type: InstrumentType,
    pub pair: Pair,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum InstrumentType {
    Spot,
    Futures,
    Derivitives,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum Side {
    Buy,
    Sell,
}

impl From<Side> for SmolStr {
    fn from(side: Side) -> Self {
        match side {
            Side::Buy => SmolStr::new("BUY"),
            Side::Sell => SmolStr::new("SELL"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum OrderType {
    Market,
    Limit,
    PostOnly,
    Ioc,
    Fok,
}

impl From<OrderType> for SmolStr {
    fn from(order_type: OrderType) -> Self {
        match order_type {
            OrderType::Market => SmolStr::new("MARKET"),
            OrderType::Limit => SmolStr::new("LIMIT"),
            OrderType::PostOnly => SmolStr::new("POST_ONLY"),
            OrderType::Ioc => SmolStr::new("IOC"),
            OrderType::Fok => SmolStr::new("FOK"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    PendingCancel,
    Rejected,
    Expired,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct OrderId(
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    SmolStr,
);

impl OrderId {
    pub fn new(value: SmolStr) -> Self {
        OrderId(value)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
}
