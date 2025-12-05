// A general level data abstraction for Internal data types, using in the internal system logic.
use crate::utils::{deserialize_smol_str, serialize_smol_str};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt;

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

impl Pair {
    pub fn new(base: &str, quote: &str) -> Self {
        Pair {
            base: base.into(),
            quote: quote.into(),
        }
    }

    pub fn convert_to_smolstr(&self) -> SmolStr {
        format!("{}{}", self.base, self.quote).into()
    }
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

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side_str = match self {
            Side::Buy => "Buy",
            Side::Sell => "Sell",
        };
        write!(f, "{}", side_str)
    }
}

impl From<Side> for SmolStr {
    fn from(side: Side) -> Self {
        match side {
            Side::Buy => SmolStr::new("Buy"),
            Side::Sell => SmolStr::new("Sell"),
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

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
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

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<u64> for OrderId {
    fn from(value: u64) -> Self {
        OrderId(SmolStr::new(value.to_string()))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
    Account,
    Order,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum AccountType {
    Spot,
    Margin,
    Futures,
    Savings,
    Wallet,
    Unknown,
}
