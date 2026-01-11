use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::types::{
    common::{BinanceInbound, SessionLogonMessage, SessionLogonParams},
    rpc::{
        amend_order::{BinanceWsAmendOrderReqParams, BinanceWsAmendOrderRequest},
        balance::{
            BinanceAccountStatusParams, BinanceAccountStatusRequest, BinanceAccountStatusResponse,
        },
        cancel_all::{
            BinanceWsCancelAllOrdersReqParams, BinanceWsCancelAllOrdersRequest,
            BinanceWsCancelAllOrdersResponse,
        },
        cancel_order::{
            BinanceWsCancelOrderReqParams, BinanceWsCancelOrderRequest,
            BinanceWsCancelOrderResponse,
        },
        open_order::{BinanceWsOrderReqParams, BinanceWsOrderRequest, BinanceWsOrderResponse},
        BinanceRpcRequest, BinanceRpcResponse,
    },
    user_data::events::{BinanceWSMessage, ParsedBinanceMessage},
};
use async_trait::async_trait;
use futures_util::{
    stream::{SplitSink, Stream, StreamExt},
    SinkExt,
};
use serde_json::Value;
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex, RwLock},
    time::{interval, timeout, Duration},
};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::frame::CloseFrame, tungstenite::protocol::Message,
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, info, warn};
use trading_core::{
    authenticator::ExchangeAuthenticator,
    error::EngineError,
    utils::generate_custom_uuid,
    websocket::client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};

pub const BINANCE_WS_URL: &str = "wss://ws-api.binance.com:443/ws-api/v3";
pub const LOGON_SESSION: &str = "session.logon";
pub const ACCOUNT_STATUS: &str = "account.status";
pub const OPEN_ORDER: &str = "order.place";
pub const TEST_ORDER: &str = "order.test";
pub const CANCEL_ORDER: &str = "order.cancel";
pub const CANCEL_ALL_ORDERS: &str = "openOrders.cancelAll";
pub const AMEND_ORDER: &str = "order.modify";

#[derive(Debug, Clone)]
pub struct BinanceChannel {
    pub base: SmolStr,
    pub quote: SmolStr,
    pub limit: u64,
}

#[derive(Debug, Clone)]
pub struct InboundHandler;

impl MessageHandler<BinanceInbound> for InboundHandler {
    fn handle_text_message(&self, text: SmolStr) -> Result<BinanceInbound, EngineError> {
        // Try parsing as RPC responses first
        if let Ok(resp) = serde_json::from_str::<BinanceAccountStatusResponse>(&text) {
            tracing::debug!("[InboundHandler] Parsed as AccountStatus RPC response");
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::AccountStatus(resp),
            )));
        }

        // Try CancelAll first (has array result) before OpenOrder (has object result)
        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelAllOrdersResponse>(&text) {
            tracing::info!("[InboundHandler] Successfully parsed as CancelAll RPC response (id={}, status={}, cancelled {} orders)", resp.id, resp.status, resp.result.len());
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::CancelAll(resp),
            )));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelOrderResponse>(&text) {
            if resp.status == 200 {
                tracing::debug!(
                    "[InboundHandler] Parsed as CancelOrder RPC response (id={}, status={})",
                    resp.id,
                    resp.status
                );
            } else {
                tracing::info!("[InboundHandler] Parsed as CancelOrder RPC error response (id={}, status={}, error={:?})", resp.id, resp.status, resp.error);
            }
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::CancelOrder(resp),
            )));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsOrderResponse>(&text) {
            tracing::info!(
                "[InboundHandler] Successfully parsed as OpenOrder RPC response (id={}, status={})",
                resp.id,
                resp.status
            );
            return Ok(BinanceInbound::Rpc(Box::new(
                BinanceRpcResponse::OpenOrder(resp),
            )));
        } else if text.contains("\"id\"")
            && text.contains("\"status\"")
            && text.contains("\"result\"")
        {
            // This looks like an RPC response but failed to parse - log the error
            if let Err(e) = serde_json::from_str::<BinanceWsOrderResponse>(&text) {
                tracing::warn!(
                    "[InboundHandler] Failed to parse as BinanceWsOrderResponse: {}, text: {}",
                    e,
                    text
                );
            }
        }

        // Fall back to parsing as event
        let event = serde_json::from_str::<ParsedBinanceMessage>(&text)
            .map(BinanceInbound::Event)
            .unwrap_or(BinanceInbound::Event(ParsedBinanceMessage::Raw(
                text.clone(),
            )));
        Ok(event)
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        debug!(?binary, "ws binary message received");
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        debug!(?ping, "ws ping received");
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        debug!(?pong, "ws pong received");
    }

    fn handle_close(&self, close: Option<CloseFrame>) {
        debug!(?close, "ws close received");
    }
}

#[derive(Debug)]
pub struct ConnectionSession {
    pub rpc_request_tx: mpsc::Sender<BinanceRpcRequest>,
    pub rpc_response_tx: mpsc::Sender<BinanceRpcResponse>,
    pub event_tx: mpsc::Sender<ParsedBinanceMessage>,
    pub reader_handle: Option<tokio::task::JoinHandle<()>>,
    pub writer_handle: Option<tokio::task::JoinHandle<()>>,
    pub writer_ready_rx: Option<tokio::sync::oneshot::Receiver<()>>,
}

#[derive(Debug, Clone)]
pub struct BinanceSubscriptionHandler {
    signer: BinanceWsAuthenticator,
    session: Arc<RwLock<Option<SessionLogonParams>>>,
}

impl BinanceSubscriptionHandler {
    pub fn new(
        signer: BinanceWsAuthenticator,
        session: Arc<RwLock<Option<SessionLogonParams>>>,
    ) -> Self {
        Self { signer, session }
    }

    pub fn session_handle(&self) -> Arc<RwLock<Option<SessionLogonParams>>> {
        Arc::clone(&self.session)
    }

    pub async fn set_session(&self, params: SessionLogonParams) {
        let mut guard = self.session.write().await;
        *guard = Some(params);
    }

    #[allow(dead_code)] // May be used for other RPC requests that don't need fresh signatures
    async fn get_session(&self) -> Result<SessionLogonParams, EngineError> {
        let guard = self.session.read().await;
        guard.clone().ok_or_else(|| {
            EngineError::new_custom_error("No session available for RPC request".into())
        })
    }

    fn build_session_logon_params(&self) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut params = BTreeMap::new();
        params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        params.insert("timestamp".into(), timestamp.to_string().into());

        let signed_params = self.signer.build_signed_params(params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub fn create_logon_message(&self) -> Result<(Message, SessionLogonParams), EngineError> {
        let logon_params = self.build_session_logon_params()?;

        let logon_msg = SessionLogonMessage {
            id: generate_custom_uuid().into(),
            method: LOGON_SESSION.into(),
            params: logon_params.clone(),
        };

        Ok((
            Message::Text(serde_json::to_string(&logon_msg)?),
            logon_params,
        ))
    }

    fn build_signed_account_status_params(
        &self,
        recv_window: Option<u64>,
        omit_zero_balances: Option<bool>,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        // Include ALL parameters that will be sent in the signature calculation
        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());

        // Include recvWindow and omitZeroBalances in signature if they're present
        if let Some(recv_window) = recv_window {
            signed_params.insert("recvWindow".into(), recv_window.to_string().into());
        }
        if let Some(omit_zero) = omit_zero_balances {
            signed_params.insert("omitZeroBalances".into(), omit_zero.to_string().into());
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_account_status_request(&self) -> Result<BinanceRpcRequest, EngineError> {
        // Generate a NEW signature with a NEW timestamp for each request
        // Reusing the logon session would cause signature validation to fail
        // Include recvWindow and omitZeroBalances in the signature calculation
        let recv_window = Some(60_000);
        let omit_zero_balances = Some(true);

        let session = self.build_signed_account_status_params(recv_window, omit_zero_balances)?;

        let request = BinanceAccountStatusRequest {
            id: generate_custom_uuid().into(),
            method: ACCOUNT_STATUS.into(),
            params: BinanceAccountStatusParams {
                session_logon_params: session,
                recv_window,
                omit_zero_balances,
            },
        };

        Ok(BinanceRpcRequest::AccountStatus(request))
    }

    fn build_signed_order_params(
        &self,
        params: &BinanceWsOrderReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        signed_params.insert("side".into(), params.side.to_string().into());
        signed_params.insert("type".into(), params.order_type.to_string().into());
        if let Some(tif) = &params.time_in_force {
            signed_params.insert("timeInForce".into(), tif.to_string().into());
        }
        if let Some(price) = &params.price {
            signed_params.insert("price".into(), price.to_string().into());
        }
        if let Some(quantity) = &params.quantity {
            signed_params.insert("quantity".into(), quantity.to_string().into());
        }
        if let Some(quote_qty) = &params.quote_order_qty {
            signed_params.insert("quoteOrderQty".into(), quote_qty.to_string().into());
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_test_order_request(
        &self,
        mut params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_order_params(&params)?;
        params.session = session;

        let request = BinanceWsOrderRequest {
            id: generate_custom_uuid().into(),
            method: TEST_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::OpenOrder(request))
    }

    pub async fn create_open_order_request(
        &self,
        mut params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_order_params(&params)?;
        params.session = session;

        let request = BinanceWsOrderRequest {
            id: generate_custom_uuid().into(),
            method: OPEN_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::OpenOrder(request))
    }

    fn build_signed_cancel_order_params(
        &self,
        params: &BinanceWsCancelOrderReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        if let Some(order_id) = params.order_id {
            signed_params.insert("orderId".into(), order_id.to_string().into());
        }
        if let Some(orig_client_order_id) = &params.orig_client_order_id {
            signed_params.insert("origClientOrderId".into(), orig_client_order_id.clone());
        }
        if let Some(new_client_order_id) = &params.new_client_order_id {
            signed_params.insert("newClientOrderId".into(), new_client_order_id.clone());
        }
        if let Some(cancel_restrictions) = &params.cancel_restrictions {
            signed_params.insert(
                "cancelRestrictions".into(),
                cancel_restrictions.to_string().into(),
            );
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    fn build_signed_cancel_all_params(
        &self,
        params: &BinanceWsCancelAllOrdersReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_cancel_order_request(
        &self,
        mut params: BinanceWsCancelOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_cancel_order_params(&params)?;
        params.session = session;

        let request = BinanceWsCancelOrderRequest {
            id: generate_custom_uuid().into(),
            method: CANCEL_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::CancelOrder(request))
    }

    pub async fn create_cancel_all_orders_request(
        &self,
        mut params: BinanceWsCancelAllOrdersReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_cancel_all_params(&params)?;
        params.session = session;

        let request = BinanceWsCancelAllOrdersRequest {
            id: generate_custom_uuid().into(),
            method: CANCEL_ALL_ORDERS.into(),
            params,
        };

        Ok(BinanceRpcRequest::CancelAll(request))
    }

    fn build_signed_amend_order_params(
        &self,
        params: &BinanceWsAmendOrderReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        if let Some(order_id) = params.order_id {
            signed_params.insert("orderId".into(), order_id.to_string().into());
        }
        if let Some(orig_client_order_id) = &params.orig_client_order_id {
            signed_params.insert("origClientOrderId".into(), orig_client_order_id.clone());
        }
        if let Some(price) = &params.price {
            signed_params.insert("price".into(), price.to_string().into());
        }
        if let Some(quantity) = &params.quantity {
            signed_params.insert("quantity".into(), quantity.to_string().into());
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_amend_order_request(
        &self,
        mut params: BinanceWsAmendOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_amend_order_params(&params)?;
        params.session = session;

        let request = BinanceWsAmendOrderRequest {
            id: generate_custom_uuid().into(),
            method: AMEND_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::AmendOrder(request))
    }
}

impl SubscriptionHandler for BinanceSubscriptionHandler {
    fn create_subscription_request(&self) -> Option<Message> {
        let subscription_msg = BinanceWSMessage {
            id: generate_custom_uuid().into(),
            method: "userDataStream.subscribe".into(),
        };

        Some(Message::Text(
            serde_json::to_string(&subscription_msg).ok()?,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct BinanceWSClient {
    url: SmolStr,
    subscription_handler: BinanceSubscriptionHandler,
}

impl BinanceWSClient {
    pub fn new(
        signer: BinanceWsAuthenticator,
        session: Arc<RwLock<Option<SessionLogonParams>>>,
    ) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new(signer, session),
        }
    }

    async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let ws_url = format!("{url}", url = self.url);
        debug!("Connecting to Binance WebSocket: {ws_url}");
        let stream = self.make_ws_request(&ws_url).await?;
        debug!("WebSocket connection established successfully");
        Ok(stream)
    }

    pub async fn connect_and_authenticate(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let mut ws_stream = self.create_ws_stream().await?;

        debug!("Creating session.logon message...");
        let (logon_msg, logon_params) = self.subscription_handler.create_logon_message()?;

        // Log the logon message content (without sensitive data)
        if let Message::Text(logon_text) = &logon_msg {
            let logon_json: Value = serde_json::from_str(logon_text).unwrap_or_default();
            debug!(
                "Sending session.logon: method={method}, id={id}",
                method = logon_json
                    .get("method")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown"),
                id = logon_json
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("unknown")
            );
        }

        ws_stream.send(logon_msg).await?;
        debug!("session.logon message sent, waiting for response...");

        match ws_stream.next().await {
            Some(Ok(Message::Text(logon_response))) => {
                debug!("Received logon response: {logon_response}");
                let v: Value = serde_json::from_str(&logon_response)?;
                if v.get("status").and_then(|s| s.as_u64()) == Some(200) {
                    debug!("Successfully authenticated with session.logon (status=200)");
                    self.subscription_handler.set_session(logon_params).await;
                } else {
                    let status = v.get("status").and_then(|s| s.as_u64()).unwrap_or(0);
                    let error_msg = format!("session.logon failed with status={status}: {v:?}");
                    error!("{error_msg}");
                    return Err(EngineError::new_custom_error(error_msg.into()));
                }
            }
            Some(Ok(other_msg)) => {
                let error_msg = format!("Unexpected message type during logon: {other_msg:?}");
                error!("{error_msg}");
                return Err(EngineError::new_custom_error(error_msg.into()));
            }
            Some(Err(e)) => {
                let error_msg = format!("Error reading logon response: {e}");
                error!("{error_msg}");
                return Err(EngineError::new_custom_error(error_msg.into()));
            }
            None => {
                let error_msg =
                    "No response received from session.logon - connection may have closed";
                error!("{error_msg}");
                return Err(EngineError::new_custom_error(error_msg.into()));
            }
        }

        debug!("Authentication complete, WebSocket ready for subscription");
        Ok(ws_stream)
    }

    pub async fn subscribe_to_user_stream(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> Result<(), EngineError> {
        if let Some(subscription_message) = self.subscription_handler.create_subscription_request()
        {
            // Log subscription message content
            if let Message::Text(sub_text) = &subscription_message {
                let sub_json: Value = serde_json::from_str(sub_text).unwrap_or_default();
                debug!("Subscribing to userDataStream: method={method}, id={id}, full_message={message}",
                    method = sub_json.get("method").and_then(|m| m.as_str()).unwrap_or("unknown"),
                    id = sub_json.get("id").and_then(|i| i.as_str()).unwrap_or("unknown"),
                    message = sub_text
                );
            } else {
                debug!("Subscribing to userDataStream (non-text message)");
            }

            ws_stream.send(subscription_message).await?;
            debug!(
                "userDataStream.subscribe message sent, waiting for response (timeout=500ms)..."
            );

            // Wait briefly for any immediate subscription response
            // Note: Binance may not send an acknowledgment for userDataStream.subscribe
            // It may just start sending events, so we use a short timeout
            match timeout(tokio::time::Duration::from_millis(500), ws_stream.next()).await {
                Ok(Some(Ok(Message::Text(response)))) => {
                    debug!("Received subscription response: {response}");
                    let v: Value = serde_json::from_str(&response)?;
                    // Check if this is a subscription acknowledgment or error
                    if let Some(status) = v.get("status").and_then(|s| s.as_u64()) {
                        if status == 200 {
                            debug!("Subscription acknowledged with status=200: {response}");
                        } else {
                            warn!("Subscription returned non-200 status={status}: {response}");
                        }
                    } else if v.get("result").is_some() {
                        debug!("Subscription result received: {response}");
                    } else if v.get("e").is_some() {
                        // This is an event - subscription likely succeeded
                        debug!("Received event during subscription (subscription likely succeeded): event_type={event_type}, full={response}",
                            event_type = v.get("e").and_then(|e| e.as_str()).unwrap_or("unknown")
                        );
                    } else {
                        // Unknown message format
                        warn!("Received unknown message format during subscription: {response}");
                    }
                }
                Ok(Some(Ok(Message::Ping(payload)))) => {
                    debug!("Received Ping during subscription (payload_len={len}), will respond with Pong",
                        len = payload.len()
                    );
                }
                Ok(Some(Ok(Message::Pong(_)))) => {
                    debug!("Received Pong during subscription");
                }
                Ok(Some(Ok(Message::Close(close_frame)))) => {
                    if let Some(cf) = &close_frame {
                        error!(
                            "WebSocket closed during subscription: code={code}, reason={reason}",
                            code = cf.code,
                            reason = cf.reason
                        );
                    } else {
                        error!("WebSocket closed during subscription (no close frame)");
                    }
                    return Err(EngineError::new_custom_error(
                        "WebSocket stream closed during subscription".into(),
                    ));
                }
                Ok(Some(Ok(other))) => {
                    warn!("Received non-text message during subscription: {other:?}");
                }
                Ok(Some(Err(e))) => {
                    error!("Error reading subscription response: {e}");
                    return Err(EngineError::new_custom_error(
                        format!("Error reading subscription response: {e}").into(),
                    ));
                }
                Ok(None) => {
                    error!("WebSocket stream closed during subscription (None returned)");
                    return Err(EngineError::new_custom_error(
                        "WebSocket stream closed during subscription".into(),
                    ));
                }
                Err(_) => {
                    // Timeout - this is OK, Binance may not send immediate response
                    // Events will come through the reader task after stream split
                    debug!("No immediate subscription response (timeout after 500ms - this is normal, events will come later)");
                }
            }
        } else {
            error!("Failed to create subscription message");
            return Err(EngineError::new_custom_error(
                "Failed to create subscription message".into(),
            ));
        }

        debug!("Subscription process complete");
        Ok(())
    }

    fn handle_read_stream<H, R>(
        &self,
        mut reader: R,
        rpc_response_tx: mpsc::Sender<BinanceRpcResponse>,
        event_tx: mpsc::Sender<ParsedBinanceMessage>,
        ping_tx: mpsc::Sender<Vec<u8>>,
        handler: H,
    ) -> tokio::task::JoinHandle<()>
    where
        H: MessageHandler<BinanceInbound> + Send + Sync + 'static,
        R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin
            + Send
            + 'static,
    {
        tokio::spawn(async move {
            use std::sync::atomic::{AtomicU64, Ordering};
            static READER_ID: AtomicU64 = AtomicU64::new(0);
            let reader_id = READER_ID.fetch_add(1, Ordering::SeqCst);

            info!("[READER {}] WebSocket reader task started", reader_id);
            let mut message_count = 0u64;

            loop {
                let message_opt = reader.next().await;
                if message_opt.is_none() {
                    warn!("[READER {}] reader.next() returned None immediately - stream ended before processing any messages. Reader task exiting.", reader_id);
                    break;
                }
                let message = message_opt.unwrap();
                message_count += 1;
                debug!("[READER {}] Received message #{count}: type={type:?}", reader_id, count = message_count, type = match &message { Ok(Message::Text(_)) => "Text", Ok(Message::Binary(_)) => "Binary", Ok(Message::Ping(_)) => "Ping", Ok(Message::Pong(_)) => "Pong", Ok(Message::Close(_)) => "Close", Err(_) => "Error" });
                match message {
                    Ok(Message::Text(txt)) => {
                        debug!(
                            "[READER {}] Processing Text message #{count}: {text}",
                            reader_id,
                            count = message_count,
                            text = txt
                        );
                        // Check if this looks like an RPC response (has "id" field)
                        let is_rpc_response = txt.contains("\"id\"")
                            && (txt.contains("\"status\"") || txt.contains("\"result\""));
                        if is_rpc_response {
                            tracing::info!(
                                "[READER {}] Received RPC response #{count}: {text}",
                                reader_id,
                                count = message_count,
                                text = txt
                            );
                        } else {
                            debug!(
                                "[READER {}] Received text message #{count}: {text}",
                                reader_id,
                                count = message_count,
                                text = txt
                            );
                        }
                        let txt_clone = txt.clone();
                        match handler.handle_text_message(txt.into()) {
                            Ok(BinanceInbound::Rpc(rpc)) => {
                                tracing::info!("[READER {}] Parsed as RPC response (message #{count}), forwarding to rpc_response_tx...", reader_id, count = message_count);
                                if rpc_response_tx.send(*rpc).await.is_err() {
                                    warn!("[READER {}] Failed to send RPC to rpc_response_tx (channel closed) - reader task exiting", reader_id);
                                    break;
                                }
                            }
                            Ok(BinanceInbound::Event(evt)) => {
                                debug!("[READER {}] Parsed as event (message #{count}), forwarding to event_tx...", reader_id, count = message_count);
                                if event_tx.send(evt).await.is_err() {
                                    error!(
                                        "[READER {}] Failed to send event to event_tx (channel closed) - reader task exiting",
                                        reader_id
                                    );
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "[READER {}] WS parse error on message #{count}: {e}, text: {text}",
                                    reader_id,
                                    count = message_count,
                                    text = txt_clone
                                );
                            }
                        }
                    }
                    Ok(Message::Binary(bin)) => {
                        debug!(
                            "Reader received binary message #{count} (len={len})",
                            count = message_count,
                            len = bin.len()
                        );
                        handler.handle_binary(bin);
                    }
                    Ok(Message::Ping(payload)) => {
                        debug!(
                            "[KEEPALIVE] Reader received Ping #{count} from Binance (payload_len={len})",
                            count = message_count,
                            len = payload.len()
                        );
                        handler.handle_ping(payload.clone());
                        if ping_tx.send(payload).await.is_err() {
                            warn!("Failed to send ping to ping channel");
                        }
                    }
                    Ok(Message::Pong(p)) => {
                        debug!(
                            "[KEEPALIVE] Reader received Pong #{count} from Binance (response to our Ping, payload_len={len})",
                            count = message_count,
                            len = p.len()
                        );
                        handler.handle_pong(p);
                    }
                    Ok(Message::Close(c)) => {
                        if let Some(close_frame) = &c {
                            error!(
                                "[READER {}] WebSocket closed by server (message #{count}): code={code}, reason={reason} - THIS IS WHY CONNECTION DROPS",
                                reader_id,
                                count = message_count,
                                code = close_frame.code,
                                reason = close_frame.reason
                            );
                        } else {
                            error!(
                                "[READER {}] WebSocket closed by server (message #{count}, no close frame) - THIS IS WHY CONNECTION DROPS",
                                reader_id,
                                count = message_count
                            );
                        }
                        handler.handle_close(c);
                        // Close the inbound channel to signal the connection is dead
                        // inbound_tx will be dropped when it goes out of scope
                        break;
                    }
                    Err(e) => {
                        error!(
                            "[READER {}] WebSocket read error on message #{count}: {e} - THIS IS WHY CONNECTION DROPS",
                            reader_id,
                            count = message_count
                        );
                        // Close the inbound channel on error
                        // inbound_tx will be dropped when it goes out of scope
                        break;
                    }
                }
            }
            // Loop exited - reader.next() returned None
            info!(
                "[READER {}] WebSocket reader task exiting - reader.next() returned None (stream ended). Processed {count} messages total.",
                reader_id,
                count = message_count
            );
            // Channels are automatically dropped here when they go out of scope
            info!(
                "[READER {}] Reader task finished, channels dropped",
                reader_id
            );
        })
    }

    fn handle_write_stream(
        &self,
        mut outbound_rx: mpsc::Receiver<BinanceRpcRequest>,
        mut ping_rx: mpsc::Receiver<Vec<u8>>,
        ws_write: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
        ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            debug!("WebSocket writer task started");

            // Signal that writer is ready
            if let Some(tx) = ready_tx {
                let _ = tx.send(());
                debug!("Writer task signaled ready");
            }
            let mut rpc_count = 0u64;
            let mut pong_count = 0u64;
            let mut ping_count = 0u64;

            // Proactive keepalive: send Ping every 30 seconds to keep connection alive
            let mut keepalive_interval = interval(Duration::from_secs(30));
            keepalive_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    Some(rpc) = outbound_rx.recv() => {
                        rpc_count += 1;
                        let (rpc_type, json) = match rpc {
                            BinanceRpcRequest::AccountStatus(req) => ("AccountStatus", serde_json::to_string(&req)),
                            BinanceRpcRequest::TestOrder(req)     => ("TestOrder", serde_json::to_string(&req)),
                            BinanceRpcRequest::OpenOrder(req)     => ("OpenOrder", serde_json::to_string(&req)),
                            BinanceRpcRequest::CancelOrder(req)   => ("CancelOrder", serde_json::to_string(&req)),
                            BinanceRpcRequest::CancelAll(req)     => ("CancelAll", serde_json::to_string(&req)),
                            BinanceRpcRequest::AmendOrder(req)     => ("AmendOrder", serde_json::to_string(&req)),
                        };
                        match json {
                            Ok(payload) => {
                                tracing::info!("[WRITER] Sending RPC #{} ({}) to WebSocket: {}", rpc_count, rpc_type, payload);
                                let mut w = ws_write.lock().await;
                                if let Err(e) = w.send(Message::Text(payload)).await {
                                    tracing::error!("[WRITER] Failed to send RPC #{} ({}): {:?}", rpc_count, rpc_type, e);
                                    break;
                                }
                                debug!("RPC #{count} ({type}) sent successfully", count = rpc_count, type = rpc_type);
                            }
                            Err(e) => {
                                error!("Serialization error for RPC #{count} ({type}): {e:?}",
                                    count = rpc_count,
                                    type = rpc_type
                                );
                                break;
                            }
                        }
                    }
                    Some(p) = ping_rx.recv() => {
                        pong_count += 1;
                        debug!("[KEEPALIVE] Writer sending Pong #{count} response to Binance Ping (payload_len={len})",
                            count = pong_count,
                            len = p.len()
                        );
                        let mut w = ws_write.lock().await;
                        if let Err(e) = w.send(Message::Pong(p)).await {
                            error!("Failed to send Pong #{count}: {e:?}", count = pong_count);
                            break;
                        }
                    }
                    _ = keepalive_interval.tick() => {
                        // Proactive keepalive: send Ping to server
                        ping_count += 1;
                        debug!("[KEEPALIVE] Writer sending proactive Ping #{count} to Binance (every 30s)", count = ping_count);
                        let mut w = ws_write.lock().await;
                        // Send empty ping payload (Binance accepts empty pings)
                        if let Err(e) = w.send(Message::Ping(vec![])).await {
                            error!("Failed to send proactive Ping #{count}: {e:?}", count = ping_count);
                            break;
                        }
                    }
                    else => {
                        debug!("Both RPC and ping channels closed, writer exiting (sent {rpc_count} RPCs, {pong_count} Pongs)",
                            rpc_count = rpc_count,
                            pong_count = pong_count
                        );
                        break;
                    }
                }
            }

            debug!(
                "WebSocket writer task exited (sent {rpc_count} RPCs, {pong_count} Pongs, {ping_count} proactive Pings total)",
                rpc_count = rpc_count,
                pong_count = pong_count,
                ping_count = ping_count
            );
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn split_stream(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        rpc_request_tx: mpsc::Sender<BinanceRpcRequest>,
        rpc_request_rx: mpsc::Receiver<BinanceRpcRequest>,
        rpc_response_tx: mpsc::Sender<BinanceRpcResponse>,
        event_tx: mpsc::Sender<ParsedBinanceMessage>,
        ping_tx: mpsc::Sender<Vec<u8>>,
        ping_rx: mpsc::Receiver<Vec<u8>>,
        handler: impl MessageHandler<BinanceInbound> + Send + Sync + 'static,
    ) -> Result<ConnectionSession, EngineError> {
        debug!("split_stream: Starting to split WebSocket stream...");
        debug!("Channel status: rpc_request_rx ready, rpc_response_tx.is_closed={}, event_tx.is_closed={}, ping_tx.is_closed={}",
            rpc_response_tx.is_closed(),
            event_tx.is_closed(),
            ping_tx.is_closed()
        );

        let (ws_write, ws_read) = ws_stream.split();
        debug!("split_stream: Stream split successful, spawning reader/writer tasks...");
        let ws_write = Arc::new(Mutex::new(ws_write));

        debug!("Starting writer and reader tasks...");
        // Create ready signal for writer task
        let (writer_ready_tx, writer_ready_rx) = tokio::sync::oneshot::channel();
        debug!("Spawning writer task with ready signal...");
        let writer_handle = self.handle_write_stream(
            rpc_request_rx,
            ping_rx,
            Arc::clone(&ws_write),
            Some(writer_ready_tx),
        );
        debug!("Spawning reader task...");
        let reader_handle = self.handle_read_stream(
            ws_read,
            rpc_response_tx.clone(),
            event_tx.clone(),
            ping_tx,
            handler,
        );
        debug!("split_stream: Both tasks spawned, returning ConnectionSession");

        Ok(ConnectionSession {
            rpc_request_tx,
            rpc_response_tx,
            event_tx,
            reader_handle: Some(reader_handle),
            writer_handle: Some(writer_handle),
            writer_ready_rx: Some(writer_ready_rx),
        })
    }
}

#[async_trait]
impl WebSocketClient<ParsedBinanceMessage> for BinanceWSClient {
    async fn make_ws_request(
        &self,
        ws_url: &str,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let (ws_stream, _) = connect_async(ws_url).await?;
        Ok(ws_stream)
    }

    async fn handle_messages<H>(
        &self,
        mut ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        event_tx: mpsc::Sender<ParsedBinanceMessage>,
        handler: H,
    ) -> Result<(), EngineError>
    where
        H: MessageHandler<ParsedBinanceMessage> + Send + Sync + 'static,
    {
        tokio::spawn(async move {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(txt)) => match handler.handle_text_message(txt.into()) {
                        Ok(parsed) => {
                            if let Err(e) = event_tx.send(parsed).await {
                                error!("failed to forward parsed event: {e:?}");
                            }
                        }
                        Err(e) => {
                            error!("WS parse error: {e:?}");
                        }
                    },

                    Ok(Message::Binary(bin)) => handler.handle_binary(bin),

                    Ok(Message::Ping(ping)) => {
                        handler.handle_ping(ping.clone());
                        // echo back
                        if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
                            error!("error sending pong: {e:?}");
                        }
                    }

                    Ok(Message::Pong(pong)) => handler.handle_pong(pong),

                    Ok(Message::Close(cf)) => handler.handle_close(cf),

                    Err(e) => {
                        error!("error while receiving ws message: {e}");
                        break;
                    }
                }
            }
            debug!("handle_messages: stream closed");
        });

        Ok(())
    }

    async fn subscribe_to_channels<S>(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        subscription_handler: S,
    ) -> Result<(), EngineError>
    where
        S: SubscriptionHandler + Send + Sync + 'static,
    {
        if let Some(subscription_message) = subscription_handler.create_subscription_request() {
            debug!("Sending subscription message: {:?}", subscription_message);
            ws_stream.send(subscription_message).await?
        } else {
            error!("Subscription message not found");
        }

        Ok(())
    }
}
