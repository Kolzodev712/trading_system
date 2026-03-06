//! k-paper wire types for market data only (orderbook, public_trade).
//! Mirrors exchange_engine k_paper messages for these two types.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

mod decimal_flexible {
    use rust_decimal::Decimal;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(d: D) -> Result<Decimal, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::String(s) => s.parse().map_err(serde::de::Error::custom),
            serde_json::Value::Number(n) => n.to_string().parse().map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom(
                "decimal: expected string or number",
            )),
        }
    }
}

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
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub price: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub quantity: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPublicTrade {
    #[serde(rename = "type")]
    pub _type: String,
    pub instrument_id: String,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub price: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub quantity: Decimal,
    pub side: String,
    pub timestamp: String,
    pub trade_id: String,
}
