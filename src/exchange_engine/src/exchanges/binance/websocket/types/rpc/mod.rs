pub mod amend_order;
pub mod balance;
pub mod cancel_all;
pub mod cancel_order;
pub mod open_order;

use amend_order::{BinanceWsAmendOrderRequest, BinanceWsAmendOrderResponse};
use cancel_all::{BinanceWsCancelAllOrdersRequest, BinanceWsCancelAllOrdersResponse};
use cancel_order::{BinanceWsCancelOrderRequest, BinanceWsCancelOrderResponse};
use smol_str::{SmolStr, ToSmolStr};

use balance::{BinanceAccountStatusRequest, BinanceAccountStatusResponse};
use open_order::{BinanceWsOrderRequest, BinanceWsOrderResponse};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BinanceRpcRequest {
    AccountStatus(BinanceAccountStatusRequest),
    TestOrder(BinanceWsOrderRequest),
    OpenOrder(BinanceWsOrderRequest),
    CancelOrder(BinanceWsCancelOrderRequest),
    CancelAll(BinanceWsCancelAllOrdersRequest),
    AmendOrder(BinanceWsAmendOrderRequest),
}

impl ToSmolStr for BinanceRpcRequest {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            BinanceRpcRequest::AccountStatus(req) => req.method.clone(),
            BinanceRpcRequest::TestOrder(req) => req.method.clone(),
            BinanceRpcRequest::OpenOrder(req) => req.method.clone(),
            BinanceRpcRequest::CancelOrder(req) => req.method.clone(),
            BinanceRpcRequest::CancelAll(req) => req.method.clone(),
            BinanceRpcRequest::AmendOrder(req) => req.method.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BinanceRpcResponse {
    AccountStatus(BinanceAccountStatusResponse),
    TestOrder(BinanceWsOrderResponse),
    OpenOrder(BinanceWsOrderResponse),
    CancelOrder(BinanceWsCancelOrderResponse),
    CancelAll(BinanceWsCancelAllOrdersResponse),
    AmendOrder(BinanceWsAmendOrderResponse),
    Error(String), // For connection errors
}

impl ToSmolStr for BinanceRpcResponse {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            BinanceRpcResponse::AccountStatus(_resp) => SmolStr::new("account.status"),
            BinanceRpcResponse::TestOrder(_resp) => SmolStr::new("order.test"),
            BinanceRpcResponse::OpenOrder(_resp) => SmolStr::new("order.place"),
            BinanceRpcResponse::CancelOrder(_resp) => SmolStr::new("order.cancel"),
            BinanceRpcResponse::CancelAll(_resp) => SmolStr::new("openOrders.cancelAll"),
            BinanceRpcResponse::AmendOrder(_resp) => SmolStr::new("order.modify"),
            BinanceRpcResponse::Error(_) => SmolStr::new("error"),
        }
    }
}

impl BinanceRpcRequest {
    pub fn get_id(&self) -> &SmolStr {
        match self {
            BinanceRpcRequest::AccountStatus(r) => &r.id,
            BinanceRpcRequest::TestOrder(r) => &r.id,
            BinanceRpcRequest::OpenOrder(r) => &r.id,
            BinanceRpcRequest::CancelOrder(r) => &r.id,
            BinanceRpcRequest::CancelAll(r) => &r.id,
            BinanceRpcRequest::AmendOrder(r) => &r.id,
        }
    }

    pub fn get_method(&self) -> &SmolStr {
        match self {
            BinanceRpcRequest::AccountStatus(r) => &r.method,
            BinanceRpcRequest::TestOrder(r) => &r.method,
            BinanceRpcRequest::OpenOrder(r) => &r.method,
            BinanceRpcRequest::CancelOrder(r) => &r.method,
            BinanceRpcRequest::CancelAll(r) => &r.method,
            BinanceRpcRequest::AmendOrder(r) => &r.method,
        }
    }

    pub fn to_message(
        &self,
    ) -> Result<tokio_tungstenite::tungstenite::protocol::Message, trading_core::error::EngineError>
    {
        // Serialize each variant individually (can't use untagged enum serialization)
        let json = match self {
            BinanceRpcRequest::AccountStatus(req) => serde_json::to_string(req)?,
            BinanceRpcRequest::TestOrder(req) => serde_json::to_string(req)?,
            BinanceRpcRequest::OpenOrder(req) => serde_json::to_string(req)?,
            BinanceRpcRequest::CancelOrder(req) => serde_json::to_string(req)?,
            BinanceRpcRequest::CancelAll(req) => serde_json::to_string(req)?,
            BinanceRpcRequest::AmendOrder(req) => serde_json::to_string(req)?,
        };
        Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(
            json,
        ))
    }
}

impl BinanceRpcResponse {
    pub fn get_id(&self) -> Option<&SmolStr> {
        match self {
            BinanceRpcResponse::AccountStatus(r) => Some(&r.id),
            BinanceRpcResponse::TestOrder(r) => Some(&r.id),
            BinanceRpcResponse::OpenOrder(r) => Some(&r.id),
            BinanceRpcResponse::CancelOrder(r) => Some(&r.id),
            BinanceRpcResponse::CancelAll(r) => Some(&r.id),
            BinanceRpcResponse::AmendOrder(r) => Some(&r.id),
            BinanceRpcResponse::Error(_) => None, // Error responses don't have IDs
        }
    }

    pub fn get_status(&self) -> Option<i64> {
        match self {
            BinanceRpcResponse::AccountStatus(r) => Some(r.status as i64),
            BinanceRpcResponse::TestOrder(r) => Some(r.status),
            BinanceRpcResponse::OpenOrder(r) => Some(r.status),
            BinanceRpcResponse::CancelOrder(r) => Some(r.status),
            BinanceRpcResponse::CancelAll(r) => Some(r.status),
            BinanceRpcResponse::AmendOrder(r) => Some(r.status),
            BinanceRpcResponse::Error(_) => None,
        }
    }

    pub fn get_retry_after(&self) -> Option<u64> {
        // Binance includes retryAfter in error.data for 429 responses
        // We need to parse it from the error message or response structure
        // For now, return None - this can be enhanced to parse from error.data
        None
    }
}
