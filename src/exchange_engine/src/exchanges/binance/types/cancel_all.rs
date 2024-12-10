use super::common::{BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce};
use core_logic::utils::{deserialize_smol_str, serialize_smol_str};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelAllOrdersRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceCancelAllOrdersResponse {
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
