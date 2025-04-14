pub mod amend;
pub mod balance;
pub mod cancel_all;
pub mod cancel_order;
pub mod open_order;
pub mod open_orders_list;

use serde::{Deserialize, Serialize};



use balance::{BinanceAccountStatusRequest, BinanceAccountStatusResponse};


#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum BinanceRpcRequest {
    AccountStatus(BinanceAccountStatusRequest)
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum BinanceRpcResponse {
    AccountStatus(BinanceAccountStatusResponse)
}