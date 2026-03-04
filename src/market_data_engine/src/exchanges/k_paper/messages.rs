//! k-paper wire types for market data only (orderbook, public_trade).
//! Mirrors exchange_engine k_paper messages for these two types.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperOrderbook {
    #[serde(rename = "type")]
    pub _type: String,
    pub instrument_id: String,
    pub bids: Vec<KPaperPriceLevel>,
    pub asks: Vec<KPaperPriceLevel>,
    pub sequence: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPriceLevel {
    pub price: DecimalOrStr,
    pub quantity: DecimalOrStr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecimalOrStr {
    Num(Decimal),
    Str(String),
}

impl DecimalOrStr {
    pub fn to_decimal(&self) -> Result<Decimal, rust_decimal::Error> {
        match self {
            DecimalOrStr::Num(d) => Ok(*d),
            DecimalOrStr::Str(s) => s.parse(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPublicTrade {
    #[serde(rename = "type")]
    pub _type: String,
    pub instrument_id: String,
    pub price: DecimalOrStr,
    pub quantity: DecimalOrStr,
    pub side: String,
    pub timestamp: String,
    pub trade_id: String,
}
