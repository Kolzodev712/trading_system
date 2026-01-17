//! Bybit trades response types
//! See: <https://bybit-exchange.github.io/docs/v5/order/execution>
//! Endpoint: GET /v5/execution/list

use super::{BybitCategory, BybitSide};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::{
        common::{OrderId, Pair, Side, Symbol},
        exchange::Trade,
    },
    utils::{deserialize_smol_str, serialize_smol_str},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitTradesResponse {
    pub ret_code: i32,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub ret_msg: SmolStr,
    pub result: BybitTradesResult,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitTradesResult {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub next_page_cursor: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub category: SmolStr,
    pub list: Vec<BybitExecution>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitExecution {
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
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub exec_id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub order_id: SmolStr,
    pub side: BybitSide,
    #[serde(with = "rust_decimal::serde::str")]
    pub exec_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub exec_qty: Decimal,
    #[serde(with = "rust_decimal::serde::str", default)]
    pub exec_fee: Decimal,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub exec_fee_currency: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub exec_time: SmolStr,
}

impl BybitExecution {
    pub fn to_trade(self, symbol_registry: Arc<SymbolRegistry>) -> Option<Trade> {
        let pair = if !self.base_coin.is_empty() && !self.settle_coin.is_empty() {
            Pair::new(&self.base_coin, &self.settle_coin)
        } else {
            symbol_registry.get_or_parse_pair(&self.symbol)
        };
        let side: Side = self.side.into();

        let timestamp = self.exec_time.parse::<u64>().unwrap_or(0);

        let fee_currency = if self.exec_fee_currency.is_empty() {
            Symbol::from("UNKNOWN")
        } else {
            Symbol::from(&self.exec_fee_currency)
        };

        Some(Trade {
            trade_id: self.exec_id,
            order_id: OrderId::new(self.order_id),
            pair,
            price: self.exec_price,
            quantity: self.exec_qty,
            side,
            timestamp,
            fee: self.exec_fee,
            fee_currency,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitTradesParams {
    pub category: BybitCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_link_id: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_coin: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_coin: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_type: Option<SmolStr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<SmolStr>,
}

impl BybitTradesResponse {
    pub fn to_trades(self, symbol_registry: Arc<SymbolRegistry>) -> Vec<Trade> {
        self.result
            .list
            .into_iter()
            .filter_map(|exec| exec.to_trade(symbol_registry.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::Pair;

    #[test]
    fn test_bybit_trades_response_deserialization() {
        let json = r#"
        {
            "retCode": 0,
            "retMsg": "OK",
            "result": {
                "nextPageCursor": "",
                "list": [
                    {
                        "symbol": "BTCUSDT",
                        "execId": "exec-123",
                        "orderId": "order-456",
                        "side": "Buy",
                        "execPrice": "50000.00",
                        "execQty": "0.10",
                        "execFee": "0.001",
                        "execFeeCurrency": "BTC",
                        "execTime": "1690872862481"
                    }
                ]
            },
            "retExtInfo": {},
            "time": 1690872862481
        }
        "#;

        let response: BybitTradesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.ret_code, 0);
        assert_eq!(response.result.list.len(), 1);
        assert_eq!(response.result.list[0].symbol.as_str(), "BTCUSDT");
        assert_eq!(response.result.list[0].side, BybitSide::Buy);
    }

    #[test]
    fn test_bybit_execution_to_trade() {
        let bybit_exec = BybitExecution {
            symbol: SmolStr::new("BTCUSDT"),
            base_coin: SmolStr::new("BTC"),
            settle_coin: SmolStr::new("USDT"),
            exec_id: SmolStr::new("exec-123"),
            order_id: SmolStr::new("order-456"),
            side: BybitSide::Buy,
            exec_price: Decimal::new(5000000, 2),
            exec_qty: Decimal::new(10, 2),
            exec_fee: Decimal::new(1, 3),
            exec_fee_currency: SmolStr::new("BTC"),
            exec_time: SmolStr::new("1690872862481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let trade = bybit_exec.to_trade(symbol_registry).unwrap();

        assert_eq!(trade.trade_id.as_str(), "exec-123");
        assert_eq!(trade.pair.base, "BTC");
        assert_eq!(trade.pair.quote, "USDT");
        assert_eq!(trade.side, Side::Buy);
        assert_eq!(trade.price, Decimal::new(5000000, 2));
        assert_eq!(trade.quantity, Decimal::new(10, 2));
        assert_eq!(trade.fee, Decimal::new(1, 3));
        assert_eq!(trade.fee_currency, "BTC");
    }

    #[test]
    fn test_bybit_execution_sell() {
        let bybit_exec = BybitExecution {
            symbol: SmolStr::new("ETHUSDT"),
            base_coin: SmolStr::new("ETH"),
            settle_coin: SmolStr::new("USDT"),
            exec_id: SmolStr::new("exec-789"),
            order_id: SmolStr::new("order-101"),
            side: BybitSide::Sell,
            exec_price: Decimal::new(300000, 1),
            exec_qty: Decimal::new(50, 2),
            exec_fee: Decimal::new(15, 0),
            exec_fee_currency: SmolStr::new("USDT"),
            exec_time: SmolStr::new("1690872962481"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("ETH", "USDT")]));
        let trade = bybit_exec.to_trade(symbol_registry).unwrap();

        assert_eq!(trade.side, Side::Sell);
        assert_eq!(trade.price, Decimal::new(300000, 1));
        assert_eq!(trade.quantity, Decimal::new(50, 2));
    }
}
