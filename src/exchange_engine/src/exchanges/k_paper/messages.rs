//! k-paper WebSocket message types (wire format).
//!
//! Server sends: orderbook, public_trade, execution, balance, position, error.
//! Client sends: place_order, cancel_order, cancel_by_client_order_id, amend_order.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

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
    pub price: DecimalOrStr,
    pub quantity: DecimalOrStr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecimalOrStr {
    Num(Decimal),
    Str(String),
}

impl DecimalOrStr {
    pub fn to_decimal(&self) -> Result<Decimal, rust_decimal::Error> {
        match self {
            DecimalOrStr::Num(d) => Ok(*d),
            DecimalOrStr::Str(s) => s.parse(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPublicTrade {
    pub instrument_id: String,
    pub price: DecimalOrStr,
    pub quantity: DecimalOrStr,
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
    pub price: DecimalOrStr,
    pub quantity: DecimalOrStr,
    #[serde(default)]
    pub fee: Option<DecimalOrStr>,
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
    pub available: DecimalOrStr,
    pub locked: DecimalOrStr,
    pub total: DecimalOrStr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPaperPosition {
    pub instrument_id: String,
    pub side: String,
    pub quantity: DecimalOrStr,
    pub entry_price: DecimalOrStr,
    #[serde(default)]
    pub unrealized_pnl: Option<DecimalOrStr>,
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
        quantity: Option<DecimalOrStr>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit_price: Option<DecimalOrStr>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct KPaperPlaceOrderRequest {
    pub instrument_id: String,
    pub side: String,       // "Buy" | "Sell"
    pub order_type: String, // "Limit" | "Market"
    pub quantity: DecimalOrStr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<DecimalOrStr>,
    /// One-element array for idempotency, or null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<Vec<String>>,
}
