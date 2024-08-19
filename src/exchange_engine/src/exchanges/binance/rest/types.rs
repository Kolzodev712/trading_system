use core_logic::{
    data::{
        base_data::{DataType, OrderId, OrderStatus, OrderType, Pair, Side},
        private_data::OrderRequest,
        public_data::{OrderBook, Trade},
    },
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, deserialize_vec_smol_str,
        serialize_option_smol_str, serialize_smol_str, serialize_vec_smol_str,
    },
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};
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
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub account_type: SmolStr,
    pub balances: Vec<Balance>,
    #[serde(
        serialize_with = "serialize_vec_smol_str",
        deserialize_with = "deserialize_vec_smol_str"
    )]
    pub permissions: Vec<SmolStr>,
    pub uid: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub asset: SmolStr,
    pub free: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderRequest {
    pub symbol: Pair,
    pub side: Side,
    pub order_type: OrderType,
    pub time_in_force: Option<BinanceTimeInForce>,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub timestamp: u64,
}

impl From<OrderRequest> for BinanceOrderRequest {
    fn from(order: OrderRequest) -> Self {
        let time_in_force = BinanceTimeInForce::Gtc;

        BinanceOrderRequest {
            symbol: order.instrument.pair,
            side: order.side,
            time_in_force: Some(time_in_force),
            order_type: order.order_type,
            quantity: Some(order.amount),
            price: Some(order.price),
            timestamp: order.timestamp,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceTimeInForce {
    Gtc,
    Ioc,
    Fok,
}

impl From<BinanceTimeInForce> for SmolStr {
    fn from(time_in_focrce: BinanceTimeInForce) -> Self {
        match time_in_focrce {
            BinanceTimeInForce::Gtc => SmolStr::new("GTC"),
            BinanceTimeInForce::Ioc => SmolStr::new("IOC"),
            BinanceTimeInForce::Fok => SmolStr::new("FOK"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub symbol: Pair,
    pub order_id: u64,
    pub client_order_id: OrderId,
    pub transact_time: u64,
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub executed_qty: Decimal,
    pub cummulative_quote_qty: Decimal,
    pub status: OrderStatus,
    pub order_type: OrderType,
    pub side: Side,
    pub working_time: u64,
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

    pub fn get_params(&self) -> HashMap<SmolStr, SmolStr> {
        match self {
            BinancePublicEndpoint::Orderbook { symbol, limit } => {
                let mut params = HashMap::new();
                params.insert("symbol".into(), symbol.to_symbol());
                if let Some(l) = limit {
                    params.insert("limit".into(), l.to_smolstr());
                }
                params
            }
            BinancePublicEndpoint::Trade { symbol } => {
                let mut params = HashMap::new();
                params.insert("symbol".into(), symbol.to_symbol());
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
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        side: SmolStr,
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        order_type: SmolStr,
        quantity: Decimal,
        price: Option<Decimal>,
        #[serde(
            serialize_with = "serialize_option_smol_str",
            deserialize_with = "deserialize_option_smol_str"
        )]
        time_in_force: Option<SmolStr>,
    },
}

impl BinancePrivateEndpoint {
    pub fn get_endpoint(&self) -> &str {
        match self {
            BinancePrivateEndpoint::AccountInfo => "account",
            BinancePrivateEndpoint::PlaceOrder { .. } => "order",
        }
    }

    pub fn get_params(&self) -> HashMap<SmolStr, SmolStr> {
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
                params.insert("symbol".into(), symbol.to_symbol());
                params.insert("side".into(), side.clone());
                params.insert("type".into(), order_type.clone());
                params.insert("quantity".into(), quantity.to_smolstr());

                if let Some(p) = price {
                    params.insert("price".into(), p.to_smolstr());
                }

                if let Some(tif) = time_in_force {
                    params.insert("timeInForce".into(), tif.clone());
                }

                params
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Clone)]
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

    pub fn to_symbol(&self) -> SmolStr {
        format!("{}{}", self.base, self.quote).into()
    }
}

impl From<Pair> for BinanceSymbol {
    fn from(pair: Pair) -> Self {
        BinanceSymbol {
            base: pair.base.to_uppercase().into(),
            quote: pair.quote.to_uppercase().into(),
        }
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
