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
            base: SmolStr::new(base),
            quote: SmolStr::new(quote),
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
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum InstrumentType {
    #[default]
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
    Amended,
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
            OrderStatus::Amended => write!(f, "Amended"),
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

    /// Try to parse the order ID as a numeric ID (i64)
    /// Returns Some(i64) if the order ID is numeric, None otherwise
    pub fn as_numeric_id(&self) -> Option<i64> {
        self.0.as_str().parse::<i64>().ok()
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
        value.0
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
    BinanceFutures,
    Bybit,
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_pair_new() {
        let pair = Pair::new("BTC", "USDT");
        assert_eq!(pair.base, "BTC");
        assert_eq!(pair.quote, "USDT");
    }

    #[test]
    fn test_pair_convert_to_smolstr() {
        let pair = Pair::new("BTC", "USDT");
        let symbol = pair.convert_to_smolstr();
        assert_eq!(symbol.as_str(), "BTCUSDT");
    }

    #[test]
    fn test_pair_display() {
        let pair = Pair::new("ETH", "USDC");
        assert_eq!(format!("{}", pair), "ETH/USDC");
    }

    #[test]
    fn test_pair_serialization() {
        let pair = Pair::new("BTC", "USDT");
        let serialized = serde_json::to_string(&pair).unwrap();
        assert!(serialized.contains("BTC"));
        assert!(serialized.contains("USDT"));
    }

    #[test]
    fn test_pair_deserialization() {
        let json = r#"{"base":"BTC","quote":"USDT"}"#;
        let pair: Pair = serde_json::from_str(json).unwrap();
        assert_eq!(pair.base, "BTC");
        assert_eq!(pair.quote, "USDT");
    }

    #[test]
    fn test_side_display() {
        assert_eq!(format!("{}", Side::Buy), "Buy");
        assert_eq!(format!("{}", Side::Sell), "Sell");
    }

    #[test]
    fn test_side_from_to_smolstr() {
        let buy_str: SmolStr = Side::Buy.into();
        assert_eq!(buy_str.as_str(), "Buy");
        let sell_str: SmolStr = Side::Sell.into();
        assert_eq!(sell_str.as_str(), "Sell");
    }

    #[test]
    fn test_order_status_display() {
        assert_eq!(format!("{}", OrderStatus::New), "New");
        assert_eq!(
            format!("{}", OrderStatus::PartiallyFilled),
            "PartiallyFilled"
        );
        assert_eq!(format!("{}", OrderStatus::Filled), "Filled");
        assert_eq!(format!("{}", OrderStatus::Canceled), "Canceled");
        assert_eq!(format!("{}", OrderStatus::PendingCancel), "PendingCancel");
        assert_eq!(format!("{}", OrderStatus::Rejected), "Rejected");
        assert_eq!(format!("{}", OrderStatus::Expired), "Expired");
        assert_eq!(format!("{}", OrderStatus::Amended), "Amended");
    }

    #[test]
    fn test_order_id_new() {
        let order_id = OrderId::new("12345".into());
        assert_eq!(order_id.as_str(), "12345");
    }

    #[test]
    fn test_order_id_from_u64() {
        let order_id: OrderId = 12345u64.into();
        assert_eq!(order_id.as_str(), "12345");
    }

    #[test]
    fn test_order_id_from_string() {
        let order_id: OrderId = "67890".to_string().into();
        assert_eq!(order_id.as_str(), "67890");
    }

    #[test]
    fn test_order_id_from_str() {
        let order_id: OrderId = "11111".into();
        assert_eq!(order_id.as_str(), "11111");
    }

    #[test]
    fn test_order_id_serialization() {
        let order_id = OrderId::new("12345".into());
        let serialized = serde_json::to_string(&order_id).unwrap();
        assert_eq!(serialized, "\"12345\"");
    }

    #[test]
    fn test_order_id_deserialization() {
        let json = "\"67890\"";
        let order_id: OrderId = serde_json::from_str(json).unwrap();
        assert_eq!(order_id.as_str(), "67890");
    }

    #[test]
    fn test_symbol_from_str() {
        let symbol: Symbol = "BTC".into();
        assert_eq!(symbol.as_ref(), "BTC");
    }

    #[test]
    fn test_symbol_from_string() {
        let symbol: Symbol = "ETH".to_string().into();
        assert_eq!(symbol.as_ref(), "ETH");
    }

    #[test]
    fn test_symbol_from_smolstr() {
        let smol_str = SmolStr::new("USDT");
        let symbol: Symbol = smol_str.into();
        assert_eq!(symbol.as_ref(), "USDT");
    }

    #[test]
    fn test_symbol_partial_eq_str() {
        let symbol: Symbol = "BTC".into();
        assert_eq!(symbol, "BTC");
        assert_ne!(symbol, "ETH");
    }

    #[test]
    fn test_symbol_display() {
        let symbol: Symbol = "BTC".into();
        assert_eq!(format!("{}", symbol), "BTC");
    }

    #[test]
    fn test_symbol_serialization() {
        let symbol: Symbol = "BTC".into();
        let serialized = serde_json::to_string(&symbol).unwrap();
        assert_eq!(serialized, "\"BTC\"");
    }

    #[test]
    fn test_symbol_deserialization() {
        let json = "\"ETH\"";
        let symbol: Symbol = serde_json::from_str(json).unwrap();
        assert_eq!(symbol.as_ref(), "ETH");
    }

    #[test]
    fn test_millisecond_timestamp() {
        let ts = MillisecondTimestamp(1734819696263);
        assert_eq!(ts.0, 1734819696263);
    }

    #[test]
    fn test_millisecond_timestamp_serialization() {
        let ts = MillisecondTimestamp(1734819696263);
        let serialized = serde_json::to_string(&ts).unwrap();
        assert_eq!(serialized, "1734819696263");
    }

    #[test]
    fn test_millisecond_timestamp_deserialization() {
        let json = "1734819696263";
        let ts: MillisecondTimestamp = serde_json::from_str(json).unwrap();
        assert_eq!(ts.0, 1734819696263);
    }

    #[test]
    fn test_exchange_serialization() {
        let exchange = Exchange::Binance;
        let serialized = serde_json::to_string(&exchange).unwrap();
        assert_eq!(serialized, "\"Binance\"");
    }

    #[test]
    fn test_exchange_deserialization() {
        let json = "\"Binance\"";
        let exchange: Exchange = serde_json::from_str(json).unwrap();
        assert_eq!(exchange, Exchange::Binance);
    }

    #[test]
    fn test_instrument_type_serialization() {
        let inst_type = InstrumentType::Spot;
        let serialized = serde_json::to_string(&inst_type).unwrap();
        assert_eq!(serialized, "\"Spot\"");
    }

    #[test]
    fn test_instrument_creation() {
        let instrument = Instrument {
            instrument_type: InstrumentType::Spot,
            pair: Pair::new("BTC", "USDT"),
        };
        assert_eq!(instrument.instrument_type, InstrumentType::Spot);
        assert_eq!(instrument.pair.base, "BTC");
    }

    #[test]
    fn test_account_type_serialization() {
        let account_type = AccountType::Spot;
        let serialized = serde_json::to_string(&account_type).unwrap();
        assert_eq!(serialized, "\"Spot\"");
    }
}
