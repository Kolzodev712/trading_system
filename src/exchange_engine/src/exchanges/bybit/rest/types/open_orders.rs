//! Bybit open orders response types.
//! Docs page: <https://bybit-exchange.github.io/docs/v5/order/open-order>
//! HTTP: `GET /v5/order/realtime` (`BYBIT_OPEN_ORDERS_PATH`).

use super::{BybitCategory, BybitOrderStatus, BybitOrderType, BybitSide};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::{
        common::{Instrument, InstrumentType, OrderId, OrderStatus, Side},
        exchange::Order,
    },
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitOpenOrdersResponse {
    pub ret_code: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub ret_msg: SmolStr,
    pub result: BybitOpenOrdersResult,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitOpenOrdersResult {
    #[serde(deserialize_with = "deserialize_bybit_category")]
    pub category: BybitCategory,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub next_page_cursor: SmolStr,
    pub list: Vec<BybitOrder>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitOrder {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub order_id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub order_link_id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub base_coin: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub settle_coin: SmolStr,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub qty: Decimal,
    pub side: BybitSide,
    pub order_status: BybitOrderStatus,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub avg_price: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub cum_exec_qty: Decimal,
    pub order_type: BybitOrderType,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub created_time: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub updated_time: SmolStr,
}

fn deserialize_bybit_category<'de, D>(deserializer: D) -> Result<BybitCategory, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    BybitCategory::try_from(s.as_str())
        .map_err(|_| serde::de::Error::custom(format!("Unknown category: {}", s)))
}

impl BybitOrder {
    pub fn to_order(
        self,
        category: &BybitCategory,
        symbol_registry: Arc<SymbolRegistry>,
    ) -> Option<Order> {
        let side: Side = self.side.into();
        let status: OrderStatus = self.order_status.into();
        let instrument_type: InstrumentType = category.clone().into();
        let pair = symbol_registry.get_or_parse_pair(&self.symbol);

        let created_time = self.created_time.parse::<u64>().unwrap_or(0);
        let updated_time = self.updated_time.parse::<u64>().unwrap_or(0);

        let client_order_id = if self.order_link_id.is_empty() {
            None
        } else {
            Some(self.order_link_id)
        };

        Some(Order {
            order_id: OrderId::new(self.order_id),
            client_order_id,
            instrument: Instrument::new(instrument_type, pair),
            side,
            price: self.price,
            amount: self.qty,
            exchange_created_at: Some(created_time),
            internal_created_at: updated_time,
            status,
            filled_amount: self.cum_exec_qty,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitOpenOrderParams {
    pub category: BybitCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_coin: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_coin: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_link_id: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_filter: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_status: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<SmolStr>,
}

impl BybitOpenOrdersResponse {
    pub fn to_orders(self, symbol_registry: Arc<SymbolRegistry>) -> Vec<Order> {
        let category = self.result.category.clone();
        self.result
            .list
            .into_iter()
            .filter_map(|order| order.to_order(&category, symbol_registry.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::Pair;

    #[test]
    fn test_bybit_open_orders_response_deserialization() {
        let json = r#"
        {
            "retCode": 0,
            "retMsg": "OK",
            "result": {
                "category": "linear",
                "nextPageCursor": "",
                "list": [
                    {
                        "orderId": "fd4300ae-7847-404e-b947-b46980a4d140",
                        "orderLinkId": "test-000005",
                        "symbol": "ETHUSDT",
                        "price": "1600.00",
                        "qty": "0.10",
                        "side": "Buy",
                        "orderStatus": "New",
                        "avgPrice": "0",
                        "cumExecQty": "0.00",
                        "orderType": "Limit",
                        "createdTime": "1684738540559",
                        "updatedTime": "1684738540561"
                    }
                ]
            },
            "retExtInfo": {},
            "time": 1684765770483
        }
        "#;

        let response: BybitOpenOrdersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert_eq!(response.result.category, BybitCategory::Linear);
        assert_eq!(response.result.list.len(), 1);
        assert_eq!(response.result.list[0].symbol.as_str(), "ETHUSDT");
        assert_eq!(response.result.list[0].side, BybitSide::Buy);
    }

    #[test]
    fn test_bybit_order_to_order() {
        let bybit_order = BybitOrder {
            order_id: SmolStr::new("fd4300ae-7847-404e-b947-b46980a4d140"),
            order_link_id: SmolStr::new("test-000005"),
            symbol: SmolStr::new("ETHUSDT"),
            base_coin: SmolStr::new("ETH"),
            settle_coin: SmolStr::new("USDT"),
            price: Decimal::new(160000, 2),
            qty: Decimal::new(10, 2),
            side: BybitSide::Buy,
            order_status: BybitOrderStatus::New,
            avg_price: Decimal::ZERO,
            cum_exec_qty: Decimal::ZERO,
            order_type: BybitOrderType::Limit,
            created_time: SmolStr::new("1684738540559"),
            updated_time: SmolStr::new("1684738540561"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("ETH", "USDT")]));
        let order = bybit_order
            .to_order(&BybitCategory::Linear, symbol_registry)
            .unwrap();

        assert_eq!(order.instrument.symbol(), "ETHUSDT");
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Decimal::new(160000, 2));
        assert_eq!(order.amount, Decimal::new(10, 2));
        assert_eq!(order.status, OrderStatus::New);
        assert_eq!(order.instrument.instrument_type(), InstrumentType::Futures);
    }

    #[test]
    fn test_bybit_order_partially_filled() {
        let bybit_order = BybitOrder {
            order_id: SmolStr::new("order-123"),
            order_link_id: SmolStr::new(""),
            symbol: SmolStr::new("BTCUSDT"),
            base_coin: SmolStr::new("BTC"),
            settle_coin: SmolStr::new("USDT"),
            price: Decimal::new(5000000, 2),
            qty: Decimal::new(100, 2),
            side: BybitSide::Sell,
            order_status: BybitOrderStatus::PartiallyFilled,
            avg_price: Decimal::new(5005000, 2),
            cum_exec_qty: Decimal::new(50, 2),
            order_type: BybitOrderType::Limit,
            created_time: SmolStr::new("1690872862481"),
            updated_time: SmolStr::new("1690872962481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let order = bybit_order
            .to_order(&BybitCategory::Spot, symbol_registry)
            .unwrap();

        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.amount, Decimal::new(100, 2));
        assert_eq!(order.filled_amount, Decimal::new(50, 2));
        assert_eq!(order.status, OrderStatus::New);
    }
}
