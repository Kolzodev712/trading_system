//! Bybit WebSocket execution (trade) message type
//! See: <https://bybit-exchange.github.io/docs/v5/websocket/private/execution>

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::sync::Arc;
use trading_core::{
    registry::symbol::SymbolRegistry,
    types::{
        common::{OrderId, Pair, Side},
        exchange::Trade,
    },
    utils::{deserialize_smol_str, serialize_smol_str},
};

use super::BybitSide;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitExecutionMessage {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub id: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub topic: SmolStr,
    pub creation_time: u64,
    pub data: Vec<BybitExecutionData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitExecutionData {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
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
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub order_link_id: SmolStr,
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
    pub fee_currency: SmolStr,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub exec_time: SmolStr,
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
    pub quote_coin: SmolStr,
    pub is_maker: bool,
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str",
        default
    )]
    pub create_type: SmolStr,
}

impl BybitExecutionData {
    pub fn to_trade(self, symbol_registry: Arc<SymbolRegistry>) -> Trade {
        let pair = if !self.base_coin.is_empty() && !self.quote_coin.is_empty() {
            Pair::new(&self.base_coin, &self.quote_coin)
        } else {
            symbol_registry.get_or_parse_pair(&self.symbol)
        };

        let side: Side = self.side.into();
        let timestamp = self.exec_time.parse::<u64>().unwrap_or(0);

        Trade {
            trade_id: self.exec_id,
            order_id: OrderId::new(self.order_id),
            pair,
            price: self.exec_price,
            quantity: self.exec_qty,
            side,
            timestamp,
            fee: self.exec_fee,
            fee_currency: if self.fee_currency.is_empty() {
                trading_core::types::common::Symbol::from("UNKNOWN")
            } else {
                trading_core::types::common::Symbol::from(&self.fee_currency)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Pair;

    #[test]
    fn test_bybit_execution_message_deserialization() {
        let json = r#"
        {
            "id": "exec-msg-123",
            "topic": "execution",
            "creationTime": 1697682317044,
            "data": [
                {
                    "symbol": "BTCUSDT",
                    "execId": "exec-123",
                    "orderId": "order-456",
                    "orderLinkId": "link-789",
                    "side": "Buy",
                    "execPrice": "50000.00",
                    "execQty": "0.10",
                    "execFee": "0.001",
                    "feeCurrency": "BTC",
                    "execTime": "1690872862481",
                    "baseCoin": "BTC",
                    "quoteCoin": "USDT",
                    "isMaker": false,
                    "createType": "CreateByUser"
                }
            ]
        }
        "#;

        let message: BybitExecutionMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.topic, "execution");
        assert_eq!(message.data.len(), 1);
        assert_eq!(message.data[0].symbol, "BTCUSDT");
        assert_eq!(message.data[0].side, BybitSide::Buy);
        assert_eq!(message.data[0].base_coin, "BTC");
        assert_eq!(message.data[0].quote_coin, "USDT");
    }

    #[test]
    fn test_bybit_execution_data_to_trade() {
        let bybit_exec = BybitExecutionData {
            symbol: SmolStr::new("BTCUSDT"),
            exec_id: SmolStr::new("exec-123"),
            order_id: SmolStr::new("order-456"),
            order_link_id: SmolStr::new("link-789"),
            side: BybitSide::Buy,
            exec_price: Decimal::new(5000000, 2),
            exec_qty: Decimal::new(10, 2),
            exec_fee: Decimal::new(1, 3),
            fee_currency: SmolStr::new("BTC"),
            exec_time: SmolStr::new("1690872862481"),
            base_coin: SmolStr::new("BTC"),
            quote_coin: SmolStr::new("USDT"),
            is_maker: false,
            create_type: SmolStr::new("CreateByUser"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let trade = bybit_exec.to_trade(symbol_registry);

        assert_eq!(trade.trade_id, "exec-123");
        assert_eq!(trade.order_id, OrderId::new("order-456".into()));
        assert_eq!(trade.pair.base, "BTC");
        assert_eq!(trade.pair.quote, "USDT");
        assert_eq!(trade.side, Side::Buy);
        assert_eq!(trade.price, Decimal::new(5000000, 2));
        assert_eq!(trade.quantity, Decimal::new(10, 2));
        assert_eq!(trade.fee, Decimal::new(1, 3));
    }

    #[test]
    fn test_bybit_execution_data_sell() {
        let bybit_exec = BybitExecutionData {
            symbol: SmolStr::new("ETHUSDT"),
            exec_id: SmolStr::new("exec-789"),
            order_id: SmolStr::new("order-101"),
            order_link_id: SmolStr::new(""),
            side: BybitSide::Sell,
            exec_price: Decimal::new(300000, 1),
            exec_qty: Decimal::new(50, 2),
            exec_fee: Decimal::new(5, 4),
            fee_currency: SmolStr::new("USDT"),
            exec_time: SmolStr::new("1690872862482"),
            base_coin: SmolStr::new("ETH"),
            quote_coin: SmolStr::new("USDT"),
            is_maker: true,
            create_type: SmolStr::new("CreateByUser"),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("ETH", "USDT")]));
        let trade = bybit_exec.to_trade(symbol_registry);

        assert_eq!(trade.side, Side::Sell);
        assert_eq!(trade.price, Decimal::new(300000, 1));
        assert_eq!(trade.quantity, Decimal::new(50, 2));
    }

    #[test]
    fn test_bybit_execution_data_empty_fee_currency() {
        let bybit_exec = BybitExecutionData {
            symbol: SmolStr::new("BTCUSDT"),
            exec_id: SmolStr::new("exec-123"),
            order_id: SmolStr::new("order-456"),
            order_link_id: SmolStr::new(""),
            side: BybitSide::Buy,
            exec_price: Decimal::new(5000000, 2),
            exec_qty: Decimal::new(10, 2),
            exec_fee: Decimal::ZERO,
            fee_currency: SmolStr::new(""),
            exec_time: SmolStr::new("1690872862481"),
            base_coin: SmolStr::new(""),
            quote_coin: SmolStr::new(""),
            is_maker: false,
            create_type: SmolStr::new(""),
        };

        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let trade = bybit_exec.to_trade(symbol_registry);

        assert_eq!(
            trade.fee_currency,
            trading_core::types::common::Symbol::from("UNKNOWN")
        );
    }
}
