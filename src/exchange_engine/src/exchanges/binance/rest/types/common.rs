use core_logic::{
    data::base_data::{OrderStatus, OrderType, Pair, Side},
    utils::{deserialize_smol_str, serialize_smol_str},
};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::{Display, Formatter, Result};

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

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceServerTime {
    pub server_time: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceListenKey {
    pub listen_key: String,
}

impl Default for BinanceListenKey {
    fn default() -> Self {
        BinanceListenKey {
            listen_key: String::new(), // Default to an empty string
        }
    }
}
