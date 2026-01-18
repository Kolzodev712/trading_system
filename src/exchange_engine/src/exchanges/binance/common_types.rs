use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::{Display, Formatter, Result};
use trading_core::{
    types::common::{OrderStatus, OrderType, Pair, Side},
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceOrderType {
    Limit,
    Market,
    StopLoss,
    StopLossLimit,
    TakeProfit,
    TakeProfitLimit,
    LimitMaker,
}

impl Display for BinanceOrderType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BinanceOrderType::Limit => write!(f, "LIMIT"),
            BinanceOrderType::Market => write!(f, "MARKET"),
            BinanceOrderType::StopLoss => write!(f, "STOP_LOSS"),
            BinanceOrderType::StopLossLimit => write!(f, "STOP_LOSS_LIMIT"),
            BinanceOrderType::TakeProfit => write!(f, "TAKE_PROFIT"),
            BinanceOrderType::TakeProfitLimit => write!(f, "TAKE_PROFIT_LIMIT"),
            BinanceOrderType::LimitMaker => write!(f, "LIMIT_MAKER"),
        }
    }
}

impl From<OrderType> for BinanceOrderType {
    fn from(order_type: OrderType) -> Self {
        match order_type {
            OrderType::Limit => BinanceOrderType::Limit,
            OrderType::Market => BinanceOrderType::Market,
            OrderType::PostOnly => BinanceOrderType::LimitMaker,
            OrderType::Ioc => BinanceOrderType::StopLoss,
            OrderType::Fok => BinanceOrderType::TakeProfit,
        }
    }
}

impl From<BinanceOrderType> for OrderType {
    fn from(binance_order_type: BinanceOrderType) -> Self {
        match binance_order_type {
            BinanceOrderType::Limit => OrderType::Limit,
            BinanceOrderType::Market => OrderType::Market,
            BinanceOrderType::LimitMaker => OrderType::PostOnly,
            BinanceOrderType::StopLoss => OrderType::Ioc,
            BinanceOrderType::TakeProfit => OrderType::Fok,
            BinanceOrderType::StopLossLimit => OrderType::Limit, // Example fallback
            BinanceOrderType::TakeProfitLimit => OrderType::Limit, // Example fallback
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceOrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Replaced,
    Rejected,
    Trade,
    Expired,
}

impl From<OrderStatus> for BinanceOrderStatus {
    fn from(order_status: OrderStatus) -> Self {
        match order_status {
            OrderStatus::New => BinanceOrderStatus::New,
            OrderStatus::Canceled => BinanceOrderStatus::Canceled,
            OrderStatus::PendingCancel => BinanceOrderStatus::Replaced,
            OrderStatus::Rejected => BinanceOrderStatus::Rejected,
            OrderStatus::PartiallyFilled | OrderStatus::Filled => BinanceOrderStatus::Trade,
            OrderStatus::Expired => BinanceOrderStatus::Expired,
            OrderStatus::Amended => BinanceOrderStatus::Canceled, // Amended orders are treated as canceled (old order was canceled)
        }
    }
}

impl From<BinanceOrderStatus> for OrderStatus {
    fn from(binance_status: BinanceOrderStatus) -> Self {
        match binance_status {
            BinanceOrderStatus::New => OrderStatus::New,
            BinanceOrderStatus::PartiallyFilled => OrderStatus::PartiallyFilled,
            BinanceOrderStatus::Filled | BinanceOrderStatus::Trade => OrderStatus::Filled,
            BinanceOrderStatus::Canceled => OrderStatus::Canceled,
            BinanceOrderStatus::Replaced => OrderStatus::PendingCancel,
            BinanceOrderStatus::Rejected => OrderStatus::Rejected,
            BinanceOrderStatus::Expired => OrderStatus::Expired,
        }
    }
}

/// To be removed when using common Side
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceSide {
    #[default]
    Buy,
    Sell,
}

impl Display for BinanceSide {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let side_str = match self {
            BinanceSide::Buy => "BUY",
            BinanceSide::Sell => "SELL",
        };
        write!(f, "{}", side_str)
    }
}

impl From<Side> for BinanceSide {
    fn from(side: Side) -> Self {
        match side {
            Side::Buy => BinanceSide::Buy,
            Side::Sell => BinanceSide::Sell,
        }
    }
}

impl From<BinanceSide> for Side {
    fn from(binance_side: BinanceSide) -> Self {
        match binance_side {
            BinanceSide::Buy => Side::Buy,
            BinanceSide::Sell => Side::Sell,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceTimeInForce {
    Gtc,
    Ioc,
    Fok,
}

impl Display for BinanceTimeInForce {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BinanceTimeInForce::Gtc => write!(f, "GTC"),
            BinanceTimeInForce::Ioc => write!(f, "IOC"),
            BinanceTimeInForce::Fok => write!(f, "FOK"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceCancelRestrictions {
    OnlyNew,
    OnlyPartiallyFilled,
}

impl Display for BinanceCancelRestrictions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BinanceCancelRestrictions::OnlyNew => write!(f, "ONLY_NEW"),
            BinanceCancelRestrictions::OnlyPartiallyFilled => write!(f, "ONLY_PARTIALLY_FILLED"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceSelfTradePreventionMode {
    None,
    ExpireTaker,
    ExpireMaker,
    ExpireBoth,
}

impl Display for BinanceSelfTradePreventionMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BinanceSelfTradePreventionMode::None => write!(f, "NONE"),
            BinanceSelfTradePreventionMode::ExpireTaker => write!(f, "EXPIRE_TAKER"),
            BinanceSelfTradePreventionMode::ExpireMaker => write!(f, "EXPIRE_MAKER"),
            BinanceSelfTradePreventionMode::ExpireBoth => write!(f, "EXPIRE_BOTH"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceSymbol {
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

impl BinanceSymbol {
    pub fn new(base: &str, quote: &str) -> Self {
        BinanceSymbol {
            base: base.to_uppercase().into(),
            quote: quote.to_uppercase().into(),
        }
    }

    pub fn convert_to_smolstr(&self) -> SmolStr {
        format!("{}{}", self.base, self.quote).into()
    }
}

impl From<BinanceSymbol> for Pair {
    fn from(symbol: BinanceSymbol) -> Self {
        Pair {
            base: symbol.base,
            quote: symbol.quote,
        }
    }
}

impl From<Pair> for BinanceSymbol {
    fn from(symbol: Pair) -> Self {
        BinanceSymbol {
            base: symbol.base,
            quote: symbol.quote,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_deserialize_binance_order_type() {
        let json = r#""LIMIT""#;
        let order_type: BinanceOrderType = serde_json::from_str(json).unwrap();
        assert_eq!(order_type, BinanceOrderType::Limit);
    }

    #[test]
    fn test_serialize_binance_order_type() {
        let order_type = BinanceOrderType::Limit;
        let json = serde_json::to_string(&order_type).unwrap();
        assert_eq!(json, r#""LIMIT""#);
    }

    #[test]
    fn test_deserialize_binance_order_status() {
        let json = r#""PARTIALLY_FILLED""#;
        let status: BinanceOrderStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, BinanceOrderStatus::PartiallyFilled);
    }

    #[test]
    fn test_serialize_binance_order_status() {
        let status = BinanceOrderStatus::PartiallyFilled;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""PARTIALLY_FILLED""#);
    }

    #[test]
    fn test_deserialize_binance_side() {
        let json = r#""BUY""#;
        let side: BinanceSide = serde_json::from_str(json).unwrap();
        assert_eq!(side, BinanceSide::Buy);

        let json_sell = r#""SELL""#;
        let side_sell: BinanceSide = serde_json::from_str(json_sell).unwrap();
        assert_eq!(side_sell, BinanceSide::Sell);
    }

    #[test]
    fn test_serialize_binance_side() {
        let side = BinanceSide::Buy;
        let json = serde_json::to_string(&side).unwrap();
        assert_eq!(json, r#""BUY""#);

        let side_sell = BinanceSide::Sell;
        let json_sell = serde_json::to_string(&side_sell).unwrap();
        assert_eq!(json_sell, r#""SELL""#);
    }

    #[test]
    fn test_deserialize_binance_time_in_force() {
        let json = r#""GTC""#;
        let tif: BinanceTimeInForce = serde_json::from_str(json).unwrap();
        assert_eq!(tif, BinanceTimeInForce::Gtc);
    }

    #[test]
    fn test_serialize_binance_time_in_force() {
        let tif = BinanceTimeInForce::Gtc;
        let json = serde_json::to_string(&tif).unwrap();
        assert_eq!(json, r#""GTC""#);
    }

    #[test]
    fn test_deserialize_binance_symbol() {
        let json = r#"{
            "base": "BTC",
            "quote": "USDT"
        }"#;

        let symbol: BinanceSymbol = serde_json::from_str(json).unwrap();
        assert_eq!(symbol.base, "BTC");
        assert_eq!(symbol.quote, "USDT");
    }

    #[test]
    fn test_serialize_binance_symbol() {
        let symbol = BinanceSymbol {
            base: "BTC".into(),
            quote: "USDT".into(),
        };

        let json = serde_json::to_string(&symbol).unwrap();
        let expected_json = r#"{"base":"BTC","quote":"USDT"}"#;
        assert_eq!(json, expected_json);
    }
}
