use super::common::{BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub side: BinanceSide,
    #[serde(rename = "type")]
    pub order_type: BinanceOrderType,
    pub time_in_force: Option<BinanceTimeInForce>,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderResponse {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub order_id: u64,
    pub order_list_id: i64,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub client_order_id: SmolStr,
    pub transact_time: Option<u64>,
    pub price: Option<Decimal>,
    pub orig_qty: Option<Decimal>,
    pub executed_qty: Option<Decimal>,
    pub cummulative_quote_qty: Option<Decimal>,
    pub status: Option<BinanceOrderStatus>,
    pub time_in_force: Option<BinanceTimeInForce>,
    #[serde(rename = "type")]
    pub order_type: Option<BinanceOrderType>,
    pub side: Option<BinanceSide>,
    pub working_time: Option<u64>,
    pub self_trade_prevention_mode: Option<String>,
    pub fills: Option<Vec<BinanceFill>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFill {
    pub price: Decimal,
    pub qty: Decimal,
    pub commission: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub commission_asset: SmolStr,
    pub trade_id: u64,
}
