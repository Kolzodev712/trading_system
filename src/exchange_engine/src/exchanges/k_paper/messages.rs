//! k-paper WebSocket message types (wire format).
//!
//! Server sends: orderbook, public_trade, execution, balance, position, error.
//! Client sends: place_order, cancel_order, cancel_by_client_order_id, amend_order.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Deserialize a decimal from either a JSON string or number (always stored as Decimal).
mod decimal_flexible {
    use rust_decimal::Decimal;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(d: D) -> Result<Decimal, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::String(s) => s.parse().map_err(serde::de::Error::custom),
            serde_json::Value::Number(n) => n.to_string().parse().map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom(
                "decimal: expected string or number",
            )),
        }
    }

    pub fn opt_deserialize<'de, D>(d: D) -> Result<Option<Decimal>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Option::<serde_json::Value>::deserialize(d)?;
        match v {
            None => Ok(None),
            Some(serde_json::Value::String(s)) => {
                s.parse().map(Some).map_err(serde::de::Error::custom)
            }
            Some(serde_json::Value::Number(n)) => n
                .to_string()
                .parse()
                .map(Some)
                .map_err(serde::de::Error::custom),
            Some(_) => Err(serde::de::Error::custom(
                "decimal: expected null, string or number",
            )),
        }
    }
}

/// Top-level message: we dispatch by `type` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KPaperMessage {
    /// Server -> client: sent immediately on connect so client can confirm WebSocket is live
    Connected(KPaperConnected),
    /// Server -> client: full orderbook snapshot
    Orderbook(KPaperOrderbook),
    /// Server -> client: one public trade
    PublicTrade(KPaperPublicTrade),
    /// Server -> client: execution (fill or reject)
    Execution(KPaperExecution),
    /// Server -> client: balance update (accept "balance" or "Balance" from server)
    #[serde(alias = "Balance")]
    Balance(KPaperBalance),
    /// Server -> client: position update
    Position(KPaperPosition),
    /// Server -> client: direct ack for cancel (same connection)
    #[serde(rename = "CancelAck")]
    CancelAck(KPaperCancelAck),
    /// Server -> client: direct ack for amend (same connection)
    #[serde(rename = "AmendAck")]
    AmendAck(KPaperAmendAck),
    /// Server -> client: error
    Error(KPaperError),
}

/// Empty payload for connection confirmation (server sends `{"type":"connected"}` on connect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperConnected {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperCancelAck {
    pub order_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperAmendAck {
    pub order_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperOrderbook {
    pub instrument_id: String,
    pub bids: Vec<KPaperPriceLevel>,
    pub asks: Vec<KPaperPriceLevel>,
    pub sequence: u64,
    pub timestamp: String, // ISO 8601
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPriceLevel {
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub price: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub quantity: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPublicTrade {
    pub instrument_id: String,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub price: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub quantity: Decimal,
    pub side: String, // "Buy" | "Sell"
    pub timestamp: String,
    pub trade_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperExecution {
    pub order_id: String,
    #[serde(default)]
    pub trade_id: Option<String>,
    pub instrument_id: String,
    pub side: String,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub price: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub quantity: Decimal,
    #[serde(default, deserialize_with = "decimal_flexible::opt_deserialize")]
    pub fee: Option<Decimal>,
    pub timestamp: String,
    #[serde(default)]
    pub reject_reason: Option<String>,
    /// If k-paper adds this for correlation
    #[serde(default)]
    pub client_order_id: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperBalance {
    pub asset: String,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub available: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub locked: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPosition {
    pub instrument_id: String,
    pub side: String,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub quantity: Decimal,
    #[serde(deserialize_with = "decimal_flexible::deserialize")]
    pub entry_price: Decimal,
    #[serde(default, deserialize_with = "decimal_flexible::opt_deserialize")]
    pub unrealized_pnl: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperError {
    pub message: String,
}

// ---------- Client -> Server commands ----------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KPaperCommand {
    GetAccountSnapshot,
    PlaceOrder {
        request: KPaperPlaceOrderRequest,
    },
    CancelOrder {
        order_id: String,
    },
    CancelByClientOrderId {
        client_order_id: String,
    },
    AmendOrder {
        order_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        quantity: Option<Decimal>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit_price: Option<Decimal>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct KPaperPlaceOrderRequest {
    pub instrument_id: String,
    pub side: String,       // "Buy" | "Sell"
    pub order_type: String, // "Limit" | "Market"
    pub quantity: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<Decimal>,
    /// One-element array for idempotency, or null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_connected() {
        let json = r#"{"type":"connected"}"#;
        let msg: KPaperMessage = serde_json::from_str(json).unwrap();
        match &msg {
            KPaperMessage::Connected(_) => {}
            _ => panic!("expected Connected"),
        }
    }

    #[test]
    fn deserialize_balance() {
        let json = r#"{"type":"balance","asset":"USDT","available":"1000000","locked":"0","total":"1000000"}"#;
        let msg: KPaperMessage = serde_json::from_str(json).unwrap();
        match &msg {
            KPaperMessage::Balance(b) => {
                assert_eq!(b.asset, "USDT");
                assert_eq!(b.total, rust_decimal::Decimal::new(1000000, 0));
            }
            _ => panic!("expected Balance"),
        }
    }

    #[test]
    fn deserialize_execution_ack() {
        let json = r#"{"type":"execution","order_id":"ord-1","instrument_id":"BTC-USDT","side":"Buy","price":50000,"quantity":0,"timestamp":"2026-01-01T00:00:00Z","reject_reason":null}"#;
        let msg: KPaperMessage = serde_json::from_str(json).unwrap();
        match &msg {
            KPaperMessage::Execution(e) => {
                assert_eq!(e.order_id, "ord-1");
                assert_eq!(e.instrument_id, "BTC-USDT");
                assert!(e.quantity.is_zero());
                assert!(e.reject_reason.is_none());
            }
            _ => panic!("expected Execution"),
        }
    }

    #[test]
    fn serialize_place_order() {
        let cmd = KPaperCommand::PlaceOrder {
            request: KPaperPlaceOrderRequest {
                instrument_id: "BTC-USDT".to_string(),
                side: "Buy".to_string(),
                order_type: "Limit".to_string(),
                quantity: rust_decimal::Decimal::new(2, 3),
                limit_price: Some(rust_decimal::Decimal::new(50000, 0)),
                client_order_id: None,
            },
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"place_order\""));
        assert!(json.contains("BTC-USDT"));
    }

    #[test]
    fn serialize_get_account_snapshot() {
        let cmd = KPaperCommand::GetAccountSnapshot;
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(json, r#"{"type":"get_account_snapshot"}"#);
    }
}
