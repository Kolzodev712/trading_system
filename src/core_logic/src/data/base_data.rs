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
    base: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    quote: SmolStr,
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
pub struct OrderId(
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    SmolStr,
);

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
}
