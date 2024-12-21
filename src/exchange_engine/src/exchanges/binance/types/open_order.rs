use super::common::{BinanceOrderStatus, BinanceOrderType, BinanceSide, BinanceTimeInForce};
use core_logic::{
    data::private_data::Order,
    utils::{
        deserialize_smol_str, serialize_enum_as_display, serialize_option_enum_as_display,
        serialize_smol_str,
    },
};

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
    #[serde(serialize_with = "serialize_enum_as_display")]
    pub side: BinanceSide,
    #[serde(rename = "type", serialize_with = "serialize_enum_as_display")]
    pub order_type: BinanceOrderType,
    #[serde(serialize_with = "serialize_option_enum_as_display")]
    pub time_in_force: Option<BinanceTimeInForce>,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub timestamp: Option<u64>,
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
}

impl From<BinanceOrderResponse> for Order {
    fn from(binance_res: BinanceOrderResponse) -> Self {
        Order {
            symbol: binance_res.symbol.into(),
            side: binance_res.side.unwrap_or_default().into(),
            price: binance_res.price.unwrap_or_default(),
            amount: binance_res.executed_qty.unwrap_or_default(),
            order_id: binance_res.order_id.into(),
            exchange_created_at: binance_res.transact_time,
            internal_created_at: binance_res.working_time.unwrap_or_default().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_binance_order_request_serialization() {
        let order_request = BinanceOrderRequest {
            symbol: "BTCUSDT".into(),
            side: BinanceSide::Buy,
            order_type: BinanceOrderType::Limit,
            time_in_force: Some(BinanceTimeInForce::Gtc),
            quantity: Some(Decimal::new(10, 2)),
            price: Some(Decimal::new(90000, 0)),
            timestamp: Some(1734819696263),
        };

        let serialized = serde_json::to_string(&order_request).unwrap();
        println!("{}", serialized);

        assert_eq!(
            serialized,
            r#"{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"GTC","quantity":"0.10","price":"90000","timestamp":1734819696263}"#
        );
    }
}
