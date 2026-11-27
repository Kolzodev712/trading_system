//! WebSocket message mapping and parsing

use std::sync::Arc;
use trading_core::{
    error::EngineError,
    registry::symbol::SymbolRegistry,
    types::exchange::{Order, Position, Trade},
};

use super::subscriber::{TOPIC_EXECUTION, TOPIC_ORDER, TOPIC_POSITION, TOPIC_WALLET};
use super::types::{
    execution::BybitExecutionMessage,
    order::BybitOrderMessage,
    position::BybitPositionMessage,
    wallet::BybitWalletMessage,
    ws_streaming::{
        BybitAuthResponse, BybitPongResponse, BybitStreamingMessage, BybitSubscribeResponse,
        BybitWebSocketOp,
    },
};

use tracing::debug;

pub struct BybitMessageMapper {
    symbol_registry: Arc<SymbolRegistry>,
}

impl BybitMessageMapper {
    pub fn new(symbol_registry: Arc<SymbolRegistry>) -> Self {
        Self { symbol_registry }
    }

    pub fn parse_message(&self, text: &str) -> Result<BybitStreamingMessage, EngineError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Invalid JSON: {}", e)))?;

        if let Some(op_value) = value.get("op") {
            if let Ok(op) = serde_json::from_value::<BybitWebSocketOp>(op_value.clone()) {
                match op {
                    BybitWebSocketOp::Auth => {
                        if let Ok(auth_resp) = serde_json::from_value::<BybitAuthResponse>(value) {
                            return Ok(BybitStreamingMessage::AuthResponse(auth_resp));
                        }
                    }
                    BybitWebSocketOp::Pong => {
                        if let Ok(pong_resp) = serde_json::from_value::<BybitPongResponse>(value) {
                            return Ok(BybitStreamingMessage::PongResponse(pong_resp));
                        }
                    }
                    BybitWebSocketOp::Subscribe => {
                        if let Ok(sub_resp) =
                            serde_json::from_value::<BybitSubscribeResponse>(value)
                        {
                            return Ok(BybitStreamingMessage::SubscribeResponse(sub_resp));
                        }
                    }
                    BybitWebSocketOp::Unsubscribe => {
                        if let Ok(sub_resp) =
                            serde_json::from_value::<BybitSubscribeResponse>(value)
                        {
                            return Ok(BybitStreamingMessage::SubscribeResponse(sub_resp));
                        }
                    }
                    BybitWebSocketOp::Ping => {
                        debug!("Received ping op in parse_message (should be handled by protocol)");
                    }
                }
            }
        }

        if let Ok(pos_msg) = serde_json::from_str::<BybitPositionMessage>(text) {
            if pos_msg.topic.as_str() == TOPIC_POSITION {
                let positions: Vec<Position> = pos_msg
                    .data
                    .into_iter()
                    .filter_map(|data| data.to_position(self.symbol_registry.clone()))
                    .collect();
                // A single frame can batch several positions; forward all of them.
                if !positions.is_empty() {
                    return Ok(BybitStreamingMessage::Position(positions));
                }
            }
        }

        if let Ok(exec_msg) = serde_json::from_str::<BybitExecutionMessage>(text) {
            if exec_msg.topic.as_str() == TOPIC_EXECUTION {
                let trades: Vec<Trade> = exec_msg
                    .data
                    .into_iter()
                    .map(|data| data.to_trade(self.symbol_registry.clone()))
                    .collect();
                // A single frame can batch several executions; forward all of them.
                if !trades.is_empty() {
                    return Ok(BybitStreamingMessage::Execution(trades));
                }
            }
        }

        if let Ok(order_msg) = serde_json::from_str::<BybitOrderMessage>(text) {
            if order_msg.topic.as_str() == TOPIC_ORDER {
                let orders: Vec<Order> = order_msg
                    .data
                    .into_iter()
                    .filter_map(|data| data.to_order(self.symbol_registry.clone()))
                    .collect();
                // A single frame can batch several orders; forward all of them.
                if !orders.is_empty() {
                    return Ok(BybitStreamingMessage::Order(orders));
                }
            }
        }

        if let Ok(wallet_msg) = serde_json::from_str::<BybitWalletMessage>(text) {
            if wallet_msg.topic.as_str() == TOPIC_WALLET {
                let balances = wallet_msg.to_balances();
                return Ok(BybitStreamingMessage::Wallet(balances));
            }
        }

        Ok(BybitStreamingMessage::Unknown(text.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Pair;

    #[test]
    fn test_parse_auth_response() {
        let json = r#"
        {
            "success": true,
            "ret_msg": "",
            "op": "auth",
            "conn_id": "conn-123"
        }
        "#;

        let registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let mapper = BybitMessageMapper::new(registry);
        let result = mapper.parse_message(json).unwrap();

        match result {
            BybitStreamingMessage::AuthResponse(resp) => {
                assert!(resp.success);
                assert_eq!(resp.conn_id, "conn-123");
            }
            _ => panic!("Expected AuthResponse"),
        }
    }

    #[test]
    fn test_parse_pong_response() {
        let json = r#"
        {
            "success": true,
            "ret_msg": "pong",
            "conn_id": "conn-123",
            "req_id": "ping-123",
            "op": "pong"
        }
        "#;

        let registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let mapper = BybitMessageMapper::new(registry);
        let result = mapper.parse_message(json).unwrap();

        match result {
            BybitStreamingMessage::PongResponse(resp) => {
                assert!(resp.success);
                assert_eq!(resp.ret_msg, "pong");
            }
            _ => panic!("Expected PongResponse"),
        }
    }

    #[test]
    fn test_parse_subscribe_response() {
        let json = r#"
        {
            "success": true,
            "ret_msg": "subscribe",
            "conn_id": "conn-123",
            "req_id": "sub-123",
            "op": "subscribe"
        }
        "#;

        let registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let mapper = BybitMessageMapper::new(registry);
        let result = mapper.parse_message(json).unwrap();

        match result {
            BybitStreamingMessage::SubscribeResponse(resp) => {
                assert!(resp.success);
                assert_eq!(resp.ret_msg, "subscribe");
            }
            _ => panic!("Expected SubscribeResponse"),
        }
    }

    #[test]
    fn test_parse_unknown_message() {
        let json = r#"{"unknown": "message"}"#;

        let registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let mapper = BybitMessageMapper::new(registry);
        let result = mapper.parse_message(json).unwrap();

        match result {
            BybitStreamingMessage::Unknown(text) => {
                assert!(text.contains("unknown"));
            }
            _ => panic!("Expected Unknown"),
        }
    }

    #[test]
    fn test_parse_multi_execution_frame_keeps_all_items() {
        let json = r#"
        {
            "id": "exec-msg-1",
            "topic": "execution",
            "creationTime": 1697682317044,
            "data": [
                {
                    "symbol": "BTCUSDT", "execId": "exec-1", "orderId": "order-1",
                    "side": "Buy", "execPrice": "50000", "execQty": "0.1",
                    "execFee": "0.001", "feeCurrency": "BTC", "execTime": "1690872862481",
                    "baseCoin": "BTC", "quoteCoin": "USDT", "isMaker": false
                },
                {
                    "symbol": "BTCUSDT", "execId": "exec-2", "orderId": "order-2",
                    "side": "Sell", "execPrice": "50001", "execQty": "0.2",
                    "execFee": "0.002", "feeCurrency": "BTC", "execTime": "1690872862482",
                    "baseCoin": "BTC", "quoteCoin": "USDT", "isMaker": true
                }
            ]
        }
        "#;

        let registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let mapper = BybitMessageMapper::new(registry);

        match mapper.parse_message(json).unwrap() {
            BybitStreamingMessage::Execution(trades) => {
                assert_eq!(trades.len(), 2);
                assert_eq!(trades[0].trade_id, "exec-1");
                assert_eq!(trades[1].trade_id, "exec-2");
            }
            other => panic!("Expected Execution, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_multi_order_frame_keeps_all_items() {
        let json = r#"
        {
            "id": "order-msg-1",
            "topic": "order",
            "creationTime": 1697682317044,
            "data": [
                {
                    "orderId": "order-1", "orderLinkId": "l1", "symbol": "BTCUSDT",
                    "baseCoin": "BTC", "settleCoin": "USDT", "price": "50000", "qty": "0.1",
                    "side": "Buy", "orderStatus": "New", "avgPrice": "0", "cumExecQty": "0",
                    "orderType": "Limit", "createdTime": "1690872862481",
                    "updatedTime": "1690872862481", "category": "linear"
                },
                {
                    "orderId": "order-2", "orderLinkId": "l2", "symbol": "BTCUSDT",
                    "baseCoin": "BTC", "settleCoin": "USDT", "price": "51000", "qty": "0.2",
                    "side": "Sell", "orderStatus": "New", "avgPrice": "0", "cumExecQty": "0",
                    "orderType": "Limit", "createdTime": "1690872862482",
                    "updatedTime": "1690872862482", "category": "linear"
                }
            ]
        }
        "#;

        let registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
        let mapper = BybitMessageMapper::new(registry);

        match mapper.parse_message(json).unwrap() {
            BybitStreamingMessage::Order(orders) => assert_eq!(orders.len(), 2),
            other => panic!("Expected Order, got {:?}", other),
        }
    }
}
