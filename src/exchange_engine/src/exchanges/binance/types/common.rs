use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    PendingCancel,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceOrderStatus {
    New,
    Canceled,
    Replaced,
    Rejected,
    Trade,
    Expired,
}

/// To be removed when using common Side
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceSide {
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceServerTime {
    pub server_time: u64,
}
