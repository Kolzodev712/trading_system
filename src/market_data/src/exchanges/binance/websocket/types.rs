// Structs and enums:

use core_logic::data::{Account, OrderBook, Trade};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::primitive::str;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinanceWSMessage {
    pub method: String,
    pub params: Vec<String>,
    pub id: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceOrderbook {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<PriceLevel>,
    #[serde(rename = "a")]
    pub asks: Vec<PriceLevel>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

impl From<BinanceOrderbook> for OrderBook {
    fn from(orderbook: BinanceOrderbook) -> Self {
        let bids = orderbook
            .bids
            .into_iter()
            .map(|level| level.price)
            .collect();
        let asks = orderbook
            .asks
            .into_iter()
            .map(|level| level.price)
            .collect();
        OrderBook { bids, asks }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct BinanceTrade {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "t")]
    pub trade_id: u64,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "q")]
    pub quantity: Decimal,
    #[serde(rename = "T")]
    pub trade_time: u64,
    #[serde(rename = "m")]
    pub is_market_maker: bool,
}

impl From<BinanceTrade> for Trade {
    fn from(trade: BinanceTrade) -> Self {
        Trade {
            price: trade.price,
            quantity: trade.quantity,
            time: trade.trade_time,
            is_market_maker: trade.is_market_maker,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

impl From<AccountInfo> for Account {
    fn from(account: AccountInfo) -> Self {
        Account {
            account_type: account.account_type,
            update_time: account.update_time,
            uid: account.uid,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum BinanceWsEventType {
    #[serde(rename = "depthUpdate")]
    DepthUpdate,
    #[serde(rename = "trade")]
    Trade,
    #[serde(rename = "account.status")]
    Account,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
pub enum ParsedBinanceMessage {
    OrderBook(OrderBook),
    Trade(Trade),
    Account(Account),
    Raw(String),
}
