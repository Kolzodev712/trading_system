pub mod amend;
pub mod balance;
pub mod cancel_all;
pub mod cancel_order;
pub mod open_order;
pub mod open_orders_list;

use serde::{Deserialize, Serialize};
use smol_str::{SmolStr, ToSmolStr};

use balance::{BinanceAccountStatusRequest, BinanceAccountStatusResponse};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BinanceRpcRequest {
    AccountStatus(BinanceAccountStatusRequest),
}

impl ToSmolStr for BinanceRpcRequest {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            BinanceRpcRequest::AccountStatus(req) => req.method.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BinanceRpcResponse {
    AccountStatus(BinanceAccountStatusResponse),
}

impl ToSmolStr for BinanceRpcResponse {
    fn to_smolstr(&self) -> SmolStr {
        match self {
            BinanceRpcResponse::AccountStatus(_resp) => SmolStr::new("account.status"),
        }
    }
}
