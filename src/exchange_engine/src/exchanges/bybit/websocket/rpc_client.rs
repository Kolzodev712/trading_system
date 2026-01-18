//! WebSocket RPC client for trading operations
//!
//! Handles authenticated RPC requests like placing orders, canceling orders, etc.

use super::types::rpc::{
    amend::{BybitRpcAmendOrderRequest, BybitRpcAmendOrderResponse},
    bulk::{
        BybitRpcBatchAmendOrderRequest, BybitRpcBatchAmendOrderResponse,
        BybitRpcBatchCancelOrderRequest, BybitRpcBatchCancelOrderResponse,
        BybitRpcBatchCreateOrderRequest, BybitRpcBatchCreateOrderResponse,
    },
    cancel_all::{BybitRpcCancelAllOrdersRequest, BybitRpcCancelAllOrdersResponse},
    cancel_by_id::{BybitRpcCancelOrderRequest, BybitRpcCancelOrderResponse},
    open_order::{BybitRpcCreateOrderRequest, BybitRpcCreateOrderResponse},
    BybitRpcOperation, BybitRpcRequestHeader,
};
use super::types::ws_streaming::{BybitAuthRequest, BybitAuthResponse, BybitWebSocketOp};
use crate::exchanges::bybit::authenticator::BybitAuthenticator;
use crate::exchanges::bybit::config::BybitConfig;
use crate::exchanges::bybit::error::BybitError;
use crate::exchanges::bybit::rate_limit::BybitRateLimiter;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info};
use trading_core::{error::EngineError, utils::generate_custom_uuid};

const BYBIT_WS_TRADE_MAINNET: &str = "wss://stream.bybit.com/v5/trade";
const BYBIT_WS_TRADE_TESTNET: &str = "wss://stream-testnet.bybit.com/v5/trade";
const RPC_TIMEOUT_SECS: u64 = 10;

#[derive(Debug)]
pub struct BybitRpcClient {
    config: BybitConfig,
    authenticator: BybitAuthenticator,
    rate_limiter: Arc<BybitRateLimiter>,
}

#[derive(Debug, Clone)]
pub enum RpcRequest {
    CreateOrder(BybitRpcCreateOrderRequest),
    CancelOrder(BybitRpcCancelOrderRequest),
    CancelAllOrders(BybitRpcCancelAllOrdersRequest),
    AmendOrder(BybitRpcAmendOrderRequest),
    BatchCreateOrder(BybitRpcBatchCreateOrderRequest),
    BatchAmendOrder(BybitRpcBatchAmendOrderRequest),
    BatchCancelOrder(BybitRpcBatchCancelOrderRequest),
}

impl RpcRequest {
    fn operation(&self) -> BybitRpcOperation {
        match self {
            RpcRequest::CreateOrder(_) => BybitRpcOperation::CreateOrder,
            RpcRequest::CancelOrder(_) => BybitRpcOperation::CancelOrder,
            RpcRequest::CancelAllOrders(_) => BybitRpcOperation::CancelAllOrders,
            RpcRequest::AmendOrder(_) => BybitRpcOperation::AmendOrder,
            RpcRequest::BatchCreateOrder(_) => BybitRpcOperation::BatchCreateOrder,
            RpcRequest::BatchAmendOrder(_) => BybitRpcOperation::BatchAmendOrder,
            RpcRequest::BatchCancelOrder(_) => BybitRpcOperation::BatchCancelOrder,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RpcResponse {
    CreateOrder(BybitRpcCreateOrderResponse),
    CancelOrder(BybitRpcCancelOrderResponse),
    CancelAllOrders(BybitRpcCancelAllOrdersResponse),
    AmendOrder(BybitRpcAmendOrderResponse),
    BatchCreateOrder(BybitRpcBatchCreateOrderResponse),
    BatchAmendOrder(BybitRpcBatchAmendOrderResponse),
    BatchCancelOrder(BybitRpcBatchCancelOrderResponse),
}

impl BybitRpcClient {
    pub fn new(config: BybitConfig, rate_limiter: Arc<BybitRateLimiter>) -> Self {
        let authenticator = BybitAuthenticator::new(
            config.credentials.api_key.clone(),
            config.credentials.api_secret.clone(),
        );

        Self {
            config,
            authenticator,
            rate_limiter,
        }
    }

    fn get_ws_url(&self) -> &str {
        if self.config.testnet {
            BYBIT_WS_TRADE_TESTNET
        } else {
            BYBIT_WS_TRADE_MAINNET
        }
    }

    fn generate_auth_signature(&self, expires: i64) -> Result<String, EngineError> {
        crate::exchanges::bybit::authenticator::websocket::generate_auth_signature(
            &self.authenticator.api_secret,
            expires,
        )
    }

    fn create_auth_request(&self) -> Result<BybitAuthRequest, EngineError> {
        let expires = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 1) as i64
            * 1000;

        let signature = self.generate_auth_signature(expires)?;
        let api_key = self.config.credentials.api_key.expose().to_string();

        Ok(BybitAuthRequest {
            req_id: Some(generate_custom_uuid().into()),
            op: BybitWebSocketOp::Auth,
            args: [api_key, expires.to_string(), signature],
        })
    }

    fn build_rpc_header(&self) -> BybitRpcRequestHeader {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string();

        BybitRpcRequestHeader {
            x_bapi_timestamp: timestamp.into(),
            x_bapi_recv_window: Some("5000".into()),
            referer: None,
        }
    }

    pub async fn send_request(&self, request: RpcRequest) -> Result<RpcResponse, EngineError> {
        let url = self.get_ws_url();
        info!("Connecting to Bybit RPC WebSocket: {}", url);

        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(|e| EngineError::WsConnectionError(Box::new(e)))?;

        let (mut write, mut read) = ws_stream.split();

        let auth_req = self.create_auth_request()?;
        let auth_json = serde_json::to_string(&auth_req).map_err(EngineError::from)?;

        write
            .send(Message::Text(auth_json))
            .await
            .map_err(|e| EngineError::WsConnectionError(Box::new(e)))?;

        let auth_response = timeout(Duration::from_secs(5), read.next())
            .await
            .map_err(|_| EngineError::CustomError(anyhow::anyhow!("Auth timeout")))?
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Connection closed during auth"))
            })?
            .map_err(|e| EngineError::WsConnectionError(Box::new(e)))?;

        match auth_response {
            Message::Text(text) => {
                let auth_resp: BybitAuthResponse =
                    serde_json::from_str(&text).map_err(EngineError::from)?;

                if !auth_resp.success {
                    return Err(EngineError::AuthenticationError(anyhow::anyhow!(
                        "Authentication failed: {}",
                        auth_resp.ret_msg
                    )));
                }
                info!("RPC authentication successful");
            }
            _ => {
                return Err(EngineError::AuthenticationError(anyhow::anyhow!(
                    "Unexpected auth response type"
                )));
            }
        }

        let operation = request.operation();
        let endpoint = operation.endpoint();
        self.rate_limiter.wait_if_needed_endpoint(endpoint).await;

        let header = self.build_rpc_header();
        let req_id = generate_custom_uuid();
        let op_str = operation.as_str();
        let request_json = match request {
            RpcRequest::CreateOrder(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
            RpcRequest::CancelOrder(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
            RpcRequest::CancelAllOrders(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
            RpcRequest::AmendOrder(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
            RpcRequest::BatchCreateOrder(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
            RpcRequest::BatchAmendOrder(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
            RpcRequest::BatchCancelOrder(mut req) => {
                req.header = header.clone();
                req.req_id = Some(req_id.clone().into());
                req.op = op_str.into();
                serde_json::to_string(&req)
            }
        }
        .map_err(EngineError::from)?;

        write
            .send(Message::Text(request_json))
            .await
            .map_err(|e| EngineError::WsConnectionError(Box::new(e)))?;

        // Read messages until we find the matching response (by reqId and op)
        let response_text = loop {
            let response = timeout(Duration::from_secs(RPC_TIMEOUT_SECS), read.next())
                .await
                .map_err(|_| EngineError::CustomError(anyhow::anyhow!("RPC response timeout")))?
                .ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("Connection closed during RPC"))
                })?
                .map_err(|e| EngineError::WsConnectionError(Box::new(e)))?;

            match response {
                Message::Text(text) => {
                    debug!("Received RPC message: {}", text);

                    let value: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse RPC message JSON: {}. Raw text: {}",
                                e,
                                text
                            );
                            continue; // Skip invalid JSON messages
                        }
                    };

                    // Check if this is a pong response (ignore it)
                    if value.get("op").and_then(|v| v.as_str()) == Some("pong") {
                        debug!("Ignoring pong response");
                        continue;
                    }

                    // Check if this response matches our request (by reqId and op)
                    let msg_req_id = value.get("reqId").and_then(|v| v.as_str());
                    let msg_op = value.get("op").and_then(|v| v.as_str());

                    if msg_req_id == Some(&req_id) && msg_op == Some(op_str) {
                        break text; // Found matching response
                    } else {
                        debug!("Received message with reqId={:?}, op={:?}, expected reqId={}, op={}, skipping", 
                            msg_req_id, msg_op, req_id, op_str);
                        // This is not our response, continue reading
                        continue;
                    }
                }
                Message::Ping(_) => {
                    // Handle ping if needed
                    continue;
                }
                Message::Pong(_) => {
                    debug!("Received pong");
                    continue;
                }
                _ => {
                    tracing::warn!("Received unexpected message type, continuing...");
                    continue;
                }
            }
        };

        let value: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
            tracing::error!(
                "Failed to parse RPC response JSON: {}. Raw text: {}",
                e,
                response_text
            );
            EngineError::JsonError(e)
        })?;

        let op_str = value.get("op").and_then(|v| v.as_str()).ok_or_else(|| {
            tracing::error!(
                "Missing 'op' field in RPC response. Raw text: {}",
                response_text
            );
            EngineError::CustomError(anyhow::anyhow!("Missing 'op' field in response"))
        })?;

        let ret_code = value
            .get("retCode")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Missing 'retCode' field in response"))
            })?;

        if ret_code != 0 {
            let ret_msg = value
                .get("retMsg")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");

            let bybit_error = BybitError::from_ret_code(ret_code as i32, ret_msg);
            return Err(EngineError::from(bybit_error));
        }

        if let Some(header) = value.get("header").and_then(|h| h.as_object()) {
            if let (Some(limit), Some(status)) = (
                header.get("X-Bapi-Limit").and_then(|v| v.as_str()),
                header.get("X-Bapi-Limit-Status").and_then(|v| v.as_str()),
            ) {
                if let (Ok(limit_val), Ok(status_val)) =
                    (limit.parse::<u64>(), status.parse::<u64>())
                {
                    debug!(
                        "Rate limit info: {}/{} remaining for operation {}",
                        status_val, limit_val, op_str
                    );
                }
            }
        }

        self.rate_limiter.record_endpoint(endpoint).await;

        let op = BybitRpcOperation::try_from(op_str).map_err(|_| {
            EngineError::CustomError(anyhow::anyhow!("Unknown RPC operation: {}", op_str))
        })?;

        match op {
            BybitRpcOperation::CreateOrder => {
                let resp: BybitRpcCreateOrderResponse =
                    serde_json::from_value(value).map_err(EngineError::from)?;
                Ok(RpcResponse::CreateOrder(resp))
            }
            BybitRpcOperation::CancelOrder => {
                let resp: BybitRpcCancelOrderResponse =
                    serde_json::from_value(value).map_err(EngineError::from)?;
                Ok(RpcResponse::CancelOrder(resp))
            }
            BybitRpcOperation::CancelAllOrders => {
                let resp: BybitRpcCancelAllOrdersResponse = serde_json::from_value(value.clone())
                    .map_err(|e| {
                    tracing::error!(
                        "Failed to deserialize CancelAllOrders response: {}. Value: {}",
                        e,
                        serde_json::to_string(&value)
                            .unwrap_or_else(|_| "serialization failed".to_string())
                    );
                    EngineError::JsonError(e)
                })?;
                Ok(RpcResponse::CancelAllOrders(resp))
            }
            BybitRpcOperation::AmendOrder => {
                let resp: BybitRpcAmendOrderResponse =
                    serde_json::from_value(value).map_err(EngineError::from)?;
                Ok(RpcResponse::AmendOrder(resp))
            }
            BybitRpcOperation::BatchCreateOrder => {
                let resp: BybitRpcBatchCreateOrderResponse =
                    serde_json::from_value(value).map_err(EngineError::from)?;
                Ok(RpcResponse::BatchCreateOrder(resp))
            }
            BybitRpcOperation::BatchAmendOrder => {
                let resp: BybitRpcBatchAmendOrderResponse =
                    serde_json::from_value(value).map_err(EngineError::from)?;
                Ok(RpcResponse::BatchAmendOrder(resp))
            }
            BybitRpcOperation::BatchCancelOrder => {
                let resp: BybitRpcBatchCancelOrderResponse =
                    serde_json::from_value(value).map_err(EngineError::from)?;
                Ok(RpcResponse::BatchCancelOrder(resp))
            }
        }
    }
}
