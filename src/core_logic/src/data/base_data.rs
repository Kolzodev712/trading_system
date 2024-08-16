// A general level data abstraction for Internal data types, using in the internal system logic.
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Pair {
    base: String,
    quote: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Instrument {
    instrument_type: InstrumentType,
    pair: Pair,
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

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum OrderType {
    Market,
    Limit,
    PostOnly,
    IOC,
    FOK,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct OrderId(String);

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
}
