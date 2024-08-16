use core_logic::data::{DataType, OrderBook, Trade};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#order-book>
pub struct BinanceOrderbookSnapshot {
    pub last_update_id: u64,
    pub bids: Vec<[Decimal; 2]>,
    pub asks: Vec<[Decimal; 2]>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#recent-trades-list>
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#account-information-user_data>
pub struct AccountInfo {
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,
    pub brokered: bool,
    pub require_self_trade_prevention: bool,
    pub prevent_sor: bool,
    pub update_time: u64,
    pub account_type: String,
    pub balances: Vec<Balance>,
    pub permissions: Vec<String>,
    pub uid: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinancePublicEndpoint {
    Orderbook {
        symbol: BinanceSymbol,
        limit: Option<u64>,
    },
    Trade {
        symbol: BinanceSymbol,
    },
}

impl BinancePublicEndpoint {
    pub fn get_endpoint(&self) -> &str {
        match self {
            BinancePublicEndpoint::Orderbook { .. } => "depth",
            BinancePublicEndpoint::Trade { .. } => "trades",
        }
    }

    pub fn get_params(&self) -> HashMap<String, String> {
        match self {
            BinancePublicEndpoint::Orderbook { symbol, limit } => {
                let mut params = HashMap::new();
                params.insert("symbol".to_string(), symbol.to_symbol());
                if let Some(l) = limit {
                    params.insert("limit".to_string(), l.to_string());
                }
                params
            }
            BinancePublicEndpoint::Trade { symbol } => {
                let mut params = HashMap::new();
                params.insert("symbol".to_string(), symbol.to_symbol());
                params
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinancePrivateEndpoint {
    AccountInfo,
    PlaceOrder {
        symbol: BinanceSymbol,
        side: String,
        order_type: String,
        quantity: Decimal,
        price: Option<Decimal>,
        time_in_force: Option<String>,
    },
}

impl BinancePrivateEndpoint {
    pub fn get_endpoint(&self) -> &str {
        match self {
            BinancePrivateEndpoint::AccountInfo => "account",
            BinancePrivateEndpoint::PlaceOrder { .. } => "order",
        }
    }

    pub fn get_params(&self) -> HashMap<String, String> {
        match self {
            BinancePrivateEndpoint::AccountInfo => HashMap::new(),
            BinancePrivateEndpoint::PlaceOrder {
                symbol,
                side,
                order_type,
                quantity,
                price,
                time_in_force,
            } => {
                let mut params = HashMap::new();
                params.insert("symbol".to_string(), symbol.to_symbol());
                params.insert("side".to_string(), side.clone());
                params.insert("type".to_string(), order_type.clone());
                params.insert("quantity".to_string(), quantity.to_string());

                if let Some(p) = price {
                    params.insert("price".to_string(), p.to_string());
                }

                if let Some(tif) = time_in_force {
                    params.insert("timeInForce".to_string(), tif.clone());
                }

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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceDataType {
    Orderbook,
    Trade,
}

impl From<DataType> for BinanceDataType {
    fn from(data_type: DataType) -> Self {
        match data_type {
            DataType::Orderbook => BinanceDataType::Orderbook,
            DataType::Trade => BinanceDataType::Trade,
        }
    }
}
