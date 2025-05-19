use core_logic::{
    data::{
        base_data::Pair,
        public_data::{Orderbook, Trade},
    },
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, deserialize_vec_smol_str,
        serialize_option_smol_str, serialize_smol_str, serialize_vec_smol_str,
    },
};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};
use std::primitive::str;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub method: SmolStr,
    #[serde(
        serialize_with = "serialize_vec_smol_str",
        deserialize_with = "deserialize_vec_smol_str"
    )]
    pub params: Vec<SmolStr>,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct InitialResponse {
    #[serde(
        serialize_with = "serialize_option_smol_str",
        deserialize_with = "deserialize_option_smol_str"
    )]
    result: Option<SmolStr>,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    id: SmolStr,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceWsEventType {
    #[serde(rename = "initialResponse")]
    InitialResponse,
    #[serde(rename = "depthUpdate")]
    DepthUpdate,
    #[serde(rename = "trade")]
    Trade,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    InitialResponse(InitialResponse),
    OrderBook(Orderbook),
    Trade(Trade),
    Raw(
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        SmolStr,
    ),
}

impl ToSmolStr for ParsedBinanceMessage {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            ParsedBinanceMessage::InitialResponse(_) => SmolStr::new("InitialResponse"),
            ParsedBinanceMessage::OrderBook(_) => SmolStr::new("OrderBook"),
            ParsedBinanceMessage::Trade(_) => SmolStr::new("Trade"),
            ParsedBinanceMessage::Raw(smol_str) => smol_str.clone(),
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

impl From<Pair> for BinanceSymbol {
    fn from(pair: Pair) -> Self {
        BinanceSymbol {
            base: pair.base,
            quote: pair.quote,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use smol_str::SmolStr;

    #[test]
    fn test_deserialize_binance_ws_message() {
        let json = json!({
            "method": "SUBSCRIBE",
            "params": ["btcusdt@depth"],
            "id": "123e4567-e89b-12d3-a456-426614174000"
        });

        let msg: BinanceWSMessage = serde_json::from_value(json).unwrap();

        assert_eq!(msg.method, "SUBSCRIBE");
        assert_eq!(msg.params, vec!["btcusdt@depth"]);
        assert_eq!(msg.id, "123e4567-e89b-12d3-a456-426614174000");
    }

    #[test]
    fn test_deserialize_initial_response() {
        let json = json!({
            "result": null,
            "id": "abc123"
        });

        let resp: InitialResponse = serde_json::from_value(json).unwrap();

        assert_eq!(resp.result, None);
        assert_eq!(resp.id, "abc123");
    }

    #[test]
    fn test_deserialize_event_type() {
        let depth: BinanceWsEventType = serde_json::from_str("\"depthUpdate\"").unwrap();
        let trade: BinanceWsEventType = serde_json::from_str("\"trade\"").unwrap();
        let init: BinanceWsEventType = serde_json::from_str("\"initialResponse\"").unwrap();

        assert_eq!(depth, BinanceWsEventType::DepthUpdate);
        assert_eq!(trade, BinanceWsEventType::Trade);
        assert_eq!(init, BinanceWsEventType::InitialResponse);
    }

    #[test]
    fn test_deserialize_parsed_binance_message_raw() {
        let raw_text = "some unparsed raw event";
        let msg = ParsedBinanceMessage::Raw(SmolStr::new(raw_text));

        assert_eq!(msg.to_smolstr(), raw_text.to_smolstr());
    }

    #[test]
    fn test_deserialize_binance_symbol() {
        let json = json!({
            "base": "BTC",
            "quote": "USDT"
        });

        let symbol: BinanceSymbol = serde_json::from_value(json).unwrap();
        assert_eq!(symbol.base, "BTC");
        assert_eq!(symbol.quote, "USDT");
    }
}
