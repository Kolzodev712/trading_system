use core_logic::data::{OrderBook, Trade};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderbookSnapshot {
    pub last_update_id: u64,
    pub bids: Vec<[Decimal; 2]>,
    pub asks: Vec<[Decimal; 2]>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTradeSnapshot {
    pub id: u64,
    pub price: Decimal,
    pub qty: Decimal,
    pub quote_qty: Decimal,
    pub time: u64,
    pub is_buyer_maker: bool,
    pub is_best_match: bool,
}

impl From<BinanceOrderbookSnapshot> for OrderBook {
    fn from(snapshot: BinanceOrderbookSnapshot) -> Self {
        OrderBook {
            bids: snapshot
                .bids
                .into_iter()
                .flat_map(|bid| vec![bid[0], bid[1]])
                .collect(),
            asks: snapshot
                .asks
                .into_iter()
                .flat_map(|ask| vec![ask[0], ask[1]])
                .collect(),
        }
    }
}

impl From<BinanceTradeSnapshot> for Trade {
    fn from(snapshot: BinanceTradeSnapshot) -> Self {
        Trade {
            price: snapshot.price,
            quantity: snapshot.qty,
            time: snapshot.time,
            is_market_maker: snapshot.is_buyer_maker,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceRestEndpoint {
    Orderbook {
        symbol: BinanceSymbol,
        limit: Option<u32>,
    },
    Trade {
        symbol: BinanceSymbol,
    },
}

impl BinanceRestEndpoint {
    pub fn get_endpoint(&self) -> &str {
        match self {
            BinanceRestEndpoint::Orderbook { .. } => "depth",
            BinanceRestEndpoint::Trade { .. } => "trades",
        }
    }

    pub fn get_params(&self) -> HashMap<String, String> {
        match self {
            BinanceRestEndpoint::Orderbook { symbol, limit } => {
                let mut params = HashMap::new();
                params.insert("symbol".to_string(), symbol.to_symbol());
                if let Some(l) = limit {
                    params.insert("limit".to_string(), l.to_string());
                }
                params
            }
            BinanceRestEndpoint::Trade { symbol } => {
                let mut params = HashMap::new();
                params.insert("symbol".to_string(), symbol.to_symbol());
                params
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceSymbol {
    pub base: String,
    pub quote: String,
}

impl BinanceSymbol {
    pub fn new(base: &str, quote: &str) -> Self {
        BinanceSymbol {
            base: base.to_uppercase(),
            quote: quote.to_uppercase(),
        }
    }

    pub fn to_symbol(&self) -> String {
        format!("{}{}", self.base, self.quote)
    }
}
