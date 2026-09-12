//! WebSocket RPC client for trading operations
//!
//! Handles authenticated RPC requests like placing orders, canceling orders, etc.

use super::types::rpc::{
    amend::{BybitRpcAmendOrderData, BybitRpcAmendOrderRequest, BybitRpcAmendOrderResponse},
    bulk::{
        BybitRpcBatchAmendOrderRequest, BybitRpcBatchAmendOrderResponse,
        BybitRpcBatchCancelOrderRequest, BybitRpcBatchCancelOrderResponse,
        BybitRpcBatchCreateOrderRequest, BybitRpcBatchCreateOrderResponse,
    },
    cancel_all::{
        BybitRpcCancelAllOrdersData, BybitRpcCancelAllOrdersRequest,
        BybitRpcCancelAllOrdersResponse,
    },
    cancel_by_id::{
        BybitRpcCancelOrderData, BybitRpcCancelOrderRequest, BybitRpcCancelOrderResponse,
    },
    open_order::{
        BybitRpcCreateOrderArgs, BybitRpcCreateOrderData, BybitRpcCreateOrderRequest,
        BybitRpcCreateOrderResponse,
    },
    BybitRpcOperation, BybitRpcRequestHeader, BybitRpcResponse,
};
use super::types::ws_streaming::{BybitAuthRequest, BybitAuthResponse, BybitWebSocketOp};
use crate::exchanges::bybit::authenticator::BybitAuthenticator;
use crate::exchanges::bybit::config::BybitConfig;
use crate::exchanges::bybit::error::BybitError;
use crate::exchanges::bybit::rate_limit::BybitRateLimiter;
use crate::exchanges::bybit::rest::client::BybitHttpClient;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info};
use trading_core::{error::EngineError, rest::HttpMethod, utils::generate_custom_uuid};

const RPC_TIMEOUT_SECS: u64 = 10;

#[derive(Debug)]
pub struct BybitRpcClient {
    config: BybitConfig,
    authenticator: BybitAuthenticator,
    rate_limiter: Arc<BybitRateLimiter>,
    rest_client: BybitHttpClient,
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
            config: config.clone(),
            authenticator,
            rate_limiter,
            rest_client: BybitHttpClient::new(config),
        }
    }

    fn get_ws_url(&self) -> Result<&str, EngineError> {
        self.config.trade_ws_url().ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!(
                "Bybit demo does not support WS Trade; orders must go through REST"
            ))
        })
    }

    async fn send_via_rest(&self, request: RpcRequest) -> Result<RpcResponse, EngineError> {
        info!("Bybit demo mode: placing order via REST (WS Trade is not supported)");
        match request {
            RpcRequest::CreateOrder(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("CreateOrder missing args"))
                })?;
                let data: Option<BybitRpcCreateOrderData> = self
                    .rest_client
                    .call(
                        "order/create",
                        HttpMethod::POST,
                        None::<BybitRpcCreateOrderArgs>,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::CreateOrder(rest_rpc_response(
                    "order.create",
                    data,
                )))
            }
            RpcRequest::CancelOrder(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("CancelOrder missing args"))
                })?;
                let data: Option<BybitRpcCancelOrderData> = self
                    .rest_client
                    .call(
                        "order/cancel",
                        HttpMethod::POST,
                        None,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::CancelOrder(rest_rpc_response(
                    "order.cancel",
                    data,
                )))
            }
            RpcRequest::AmendOrder(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("AmendOrder missing args"))
                })?;
                let data: Option<BybitRpcAmendOrderData> = self
                    .rest_client
                    .call(
                        "order/amend",
                        HttpMethod::POST,
                        None,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::AmendOrder(rest_rpc_response(
                    "order.amend",
                    data,
                )))
            }
            RpcRequest::CancelAllOrders(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("CancelAllOrders missing args"))
                })?;
                let data: Option<BybitRpcCancelAllOrdersData> = self
                    .rest_client
                    .call(
                        "order/cancel-all",
                        HttpMethod::POST,
                        None,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::CancelAllOrders(rest_rpc_response(
                    "order.cancel-all",
                    data,
                )))
            }
            RpcRequest::BatchCreateOrder(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("BatchCreateOrder missing args"))
                })?;
                let data = self
                    .rest_client
                    .call(
                        "order/create-batch",
                        HttpMethod::POST,
                        None,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::BatchCreateOrder(rest_rpc_response(
                    "order.create-batch",
                    data,
                )))
            }
            RpcRequest::BatchAmendOrder(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("BatchAmendOrder missing args"))
                })?;
                let data = self
                    .rest_client
                    .call(
                        "order/amend-batch",
                        HttpMethod::POST,
                        None,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::BatchAmendOrder(rest_rpc_response(
                    "order.amend-batch",
                    data,
                )))
            }
            RpcRequest::BatchCancelOrder(req) => {
                let args = req.args.into_iter().next().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("BatchCancelOrder missing args"))
                })?;
                let data = self
                    .rest_client
                    .call(
                        "order/cancel-batch",
                        HttpMethod::POST,
                        None,
                        Some(args),
                        None,
                        true,
                    )
                    .await?;
                Ok(RpcResponse::BatchCancelOrder(rest_rpc_response(
                    "order.cancel-batch",
                    data,
                )))
            }
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
        if self.config.demo {
            return self.send_via_rest(request).await;
        }

        let url = self.get_ws_url()?;
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

        // For batch operations, determine the count and use batch rate limiting
        let batch_count = match &request {
            RpcRequest::BatchCreateOrder(req) => {
                req.args.iter().map(|arg| arg.request.len()).sum::<usize>() as u64
            }
            RpcRequest::BatchAmendOrder(req) => {
                req.args.iter().map(|arg| arg.request.len()).sum::<usize>() as u64
            }
            RpcRequest::BatchCancelOrder(req) => {
                req.args.iter().map(|arg| arg.request.len()).sum::<usize>() as u64
            }
            _ => 0,
        };

        // Use batch rate limiting for batch operations, regular for single operations
        if batch_count > 0 {
            self.rate_limiter
                .wait_if_needed_batch_endpoint(endpoint, batch_count)
                .await;
        } else {
            self.rate_limiter.wait_if_needed_endpoint(endpoint).await;
        }

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

        // Record batch operations with count, single operations with count 1
        if batch_count > 0 {
            self.rate_limiter
                .record_batch_endpoint(endpoint, batch_count)
                .await;
        } else {
            self.rate_limiter.record_endpoint(endpoint).await;
        }

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

fn rest_rpc_response<T>(op: &str, data: Option<T>) -> BybitRpcResponse<T> {
    BybitRpcResponse {
        req_id: None,
        ret_code: 0,
        ret_msg: "OK".into(),
        op: op.into(),
        data,
        ret_ext_info: None,
        header: None,
        conn_id: smol_str::SmolStr::new_inline(""),
    }
}
