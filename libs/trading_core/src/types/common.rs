//! Common types used across the trading system
use crate::utils::{deserialize_smol_str, serialize_smol_str};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt;

/// Trading pair (base/quote)
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
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

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.base, self.quote)
    }
}

/// Trading instrument
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Instrument {
    pub instrument_type: InstrumentType,
    pub pair: Pair,
}

/// Instrument type
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum InstrumentType {
    Spot,
    Futures,
    Derivitives,
}

/// Order side
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

/// Order type
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum OrderType {
    Market,
    Limit,
    PostOnly,
    Ioc,
    Fok,
}

/// Order status
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

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderStatus::New => write!(f, "New"),
            OrderStatus::PartiallyFilled => write!(f, "PartiallyFilled"),
            OrderStatus::Filled => write!(f, "Filled"),
            OrderStatus::Canceled => write!(f, "Canceled"),
            OrderStatus::PendingCancel => write!(f, "PendingCancel"),
            OrderStatus::Rejected => write!(f, "Rejected"),
            OrderStatus::Expired => write!(f, "Expired"),
        }
    }
}

/// Order ID
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

impl From<String> for OrderId {
    fn from(value: String) -> Self {
        OrderId(SmolStr::new(value))
    }
}

impl From<&str> for OrderId {
    fn from(value: &str) -> Self {
        OrderId(SmolStr::new(value))
    }
}

/// Data type
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DataType {
    Orderbook,
    Trade,
    Account,
    Order,
}

/// Account type
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum AccountType {
    Spot,
    Margin,
    Futures,
    Savings,
    Wallet,
    Unknown,
}

/// Symbol type (newtype wrapper around SmolStr)
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct Symbol(
    #[serde(
        serialize_with = "crate::utils::serialize_smol_str",
        deserialize_with = "crate::utils::deserialize_smol_str"
    )]
    SmolStr,
);

impl std::fmt::Display for Symbol {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Symbol> for SmolStr {
    #[inline]
    fn from(value: Symbol) -> Self {
        value.0.into()
    }
}

impl From<&Symbol> for SmolStr {
    #[inline]
    fn from(value: &Symbol) -> Self {
        value.0.clone()
    }
}

impl From<SmolStr> for Symbol {
    #[inline]
    fn from(value: SmolStr) -> Self {
        Self(value)
    }
}

impl From<&SmolStr> for Symbol {
    #[inline]
    fn from(value: &SmolStr) -> Self {
        Self(value.clone())
    }
}

impl From<&str> for Symbol {
    #[inline]
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<String> for Symbol {
    #[inline]
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for Symbol {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for Symbol {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for Symbol {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Millisecond timestamp (newtype wrapper)
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[repr(transparent)]
pub struct MillisecondTimestamp(pub u64);

/// Exchange identifier
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
pub enum Exchange {
    Binance,
    Unsupported,
}
