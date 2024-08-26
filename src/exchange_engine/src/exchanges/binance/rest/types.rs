use chrono::{DateTime, Utc};
use core_logic::{
    data::{
        base_data::{DataType, OrderId, OrderType, Pair, Side},
        private_data::{Order, OrderRequest},
        public_data::{OrderBook, Trade},
    },
    utils::{
        deserialize_option_smol_str, deserialize_smol_str, deserialize_vec_smol_str,
        serialize_option_smol_str, serialize_smol_str, serialize_vec_smol_str,
    },
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::{format_smolstr, SmolStr, ToSmolStr};
use std::{collections::HashMap, fmt};

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
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub side: BinanceSide,
    pub order_type: BinanceOrderType,
    pub time_in_force: Option<BinanceTimeInForce>,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub timestamp: u64,
}

impl From<OrderRequest> for BinanceOrderRequest {
    fn from(order: OrderRequest) -> Self {
        let time_in_force = BinanceTimeInForce::Gtc;

        BinanceOrderRequest {
            symbol: format_smolstr!(
                "{}{}",
                order.instrument.pair.base,
                order.instrument.pair.quote
            ),
            side: order.side.into(),
            time_in_force: Some(time_in_force),
            order_type: order.order_type.into(),
            quantity: Some(order.amount),
            price: Some(order.price),
            timestamp: order.timestamp,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub transact_time: u64,
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
}

impl From<BinanceOrderResponse> for Order {
    fn from(response: BinanceOrderResponse) -> Self {
        Order {
            order_id: OrderId::new(SmolStr::new(response.order_id.to_string())),
            instrument: response.symbol.into(),
            side: response.side.into(),
            price: response.price,
            amount: response.orig_qty,
            internal_created_at: Utc::now(),
            exchange_created_at: response
                .transact_time
                .checked_div(1000) // Safely divide to get seconds
                .and_then(|seconds| chrono::NaiveDateTime::from_timestamp_opt(seconds as i64, 0)) // Handle None case
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelOrderByIdRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub status: BinanceOrderStatus,
    pub side: BinanceSide,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub order_type: BinanceOrderType,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelOrderByIdResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub status: BinanceOrderStatus,
    pub time_in_force: BinanceTimeInForce,
    #[serde(rename = "type")]
    pub type_: BinanceOrderType,
    pub side: BinanceSide,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceOrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    PendingCancel,
    Rejected,
    Expired,
}

impl fmt::Display for BinanceOrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            BinanceOrderStatus::New => "NEW",
            BinanceOrderStatus::PartiallyFilled => "PARTIALLY_FILLED",
            BinanceOrderStatus::Filled => "FILLED",
            BinanceOrderStatus::Canceled => "CANCELED",
            BinanceOrderStatus::PendingCancel => "PENDING_CANCEL",
            BinanceOrderStatus::Rejected => "REJECTED",
            BinanceOrderStatus::Expired => "EXPIRED",
        };
        write!(f, "{}", status_str)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceOrderType {
    Market,
    Limit,
    PostOnly,
}

impl From<OrderType> for BinanceOrderType {
    fn from(order_type: OrderType) -> Self {
        match order_type {
            OrderType::Market => BinanceOrderType::Market,
            OrderType::Limit => BinanceOrderType::Limit,
            OrderType::PostOnly => BinanceOrderType::PostOnly,
            _ => panic!("Unsupported order type for Binance"), // Or handle it in a way that fits your application's needs
        }
    }
}

impl fmt::Display for BinanceOrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_str = match self {
            BinanceOrderType::Market => "market",
            BinanceOrderType::Limit => "limit",
            BinanceOrderType::PostOnly => "postOnly",
        };
        write!(f, "{}", type_str)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
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

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceSide {
    Buy,
    Sell,
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

impl fmt::Display for BinanceSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side_str = match self {
            BinanceSide::Buy => "BUY",
            BinanceSide::Sell => "SELL",
        };
        write!(f, "{}", side_str)
    }
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
#[serde(rename_all = "camelCase")]
pub enum BinancePrivateEndpoint {
    AccountInfo,
    PlaceOrder {
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        symbol: SmolStr,
        side: BinanceSide,
        order_type: BinanceOrderType,
        quantity: Decimal,
        price: Option<Decimal>,
        #[serde(
            serialize_with = "serialize_option_smol_str",
            deserialize_with = "deserialize_option_smol_str"
        )]
        time_in_force: Option<SmolStr>,
    },
    CancelOrder {
        #[serde(
            serialize_with = "serialize_smol_str",
            deserialize_with = "deserialize_smol_str"
        )]
        symbol: SmolStr,
        order_id: u64,
        status: BinanceOrderStatus,
        time_in_force: BinanceTimeInForce,
        order_type: BinanceOrderType,
        side: BinanceSide,
    },
}

impl BinancePrivateEndpoint {
    pub fn get_endpoint(&self) -> &str {
        match self {
            BinancePrivateEndpoint::AccountInfo => "account",
            BinancePrivateEndpoint::PlaceOrder { .. } => "order",
            BinancePrivateEndpoint::CancelOrder { .. } => "order",
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
                params.insert("symbol".into(), symbol.clone());
                params.insert("side".into(), side.clone().to_smolstr());
                params.insert("type".into(), order_type.clone().to_smolstr());
                params.insert("quantity".into(), quantity.to_smolstr());

                if let Some(p) = price {
                    params.insert("price".into(), p.to_smolstr());
                }

                if let Some(tif) = time_in_force {
                    params.insert("timeInForce".into(), tif.clone());
                }

                params
            }
            BinancePrivateEndpoint::CancelOrder {
                symbol,
                order_id,
                status,
                time_in_force,
                order_type,
                side,
            } => {
                let mut params = HashMap::new();
                params.insert("symbol".into(), symbol.clone());
                params.insert("orderId".into(), order_id.clone().to_smolstr());
                params.insert("side".into(), side.clone().to_smolstr());
                params.insert("type".into(), order_type.clone().to_smolstr());
                params.insert("status".into(), status.clone().to_smolstr());
                params.insert("timeInForce".into(), time_in_force.clone().into());

                params
            }
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
