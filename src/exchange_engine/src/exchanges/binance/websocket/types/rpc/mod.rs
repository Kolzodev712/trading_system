pub mod balance;
pub mod cancel_all;
pub mod cancel_order;
pub mod open_order;

use cancel_all::{BinanceWsCancelAllOrdersRequest, BinanceWsCancelAllOrdersResponse};
use cancel_order::{BinanceWsCancelOrderRequest, BinanceWsCancelOrderResponse};
use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};

use balance::{BinanceAccountStatusRequest, BinanceAccountStatusResponse};
use open_order::{BinanceWsOrderRequest, BinanceWsOrderResponse};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BinanceRpcRequest {
    AccountStatus(BinanceAccountStatusRequest),
    TestOrder(BinanceWsOrderRequest),
    OpenOrder(BinanceWsOrderRequest),
    CancelOrder(BinanceWsCancelOrderRequest),
    CancelAll(BinanceWsCancelAllOrdersRequest),
}

impl ToSmolStr for BinanceRpcRequest {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            BinanceRpcRequest::AccountStatus(req) => req.method.clone(),
            BinanceRpcRequest::TestOrder(req) => req.method.clone(),
            BinanceRpcRequest::OpenOrder(req) => req.method.clone(),
            BinanceRpcRequest::CancelOrder(req) => req.method.clone(),
            BinanceRpcRequest::CancelAll(req) => req.method.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BinanceRpcResponse {
    AccountStatus(BinanceAccountStatusResponse),
    TestOrder(BinanceWsOrderResponse),
    OpenOrder(BinanceWsOrderResponse),
    CancelOrder(BinanceWsCancelOrderResponse),
    CancelAll(BinanceWsCancelAllOrdersResponse),
}

impl ToSmolStr for BinanceRpcResponse {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            BinanceRpcResponse::AccountStatus(_resp) => SmolStr::new("account.status"),
            BinanceRpcResponse::TestOrder(_resp) => SmolStr::new("order.test"),
            BinanceRpcResponse::OpenOrder(_resp) => SmolStr::new("order.place"),
            BinanceRpcResponse::CancelOrder(_resp) => SmolStr::new("order.cancel"),
            BinanceRpcResponse::CancelAll(_resp) => SmolStr::new("openOrders.cancelAll"),
        }
    }
}
