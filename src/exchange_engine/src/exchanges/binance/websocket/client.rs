use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::types::{
    common::{BinanceInbound, SessionLogonMessage, SessionLogonParams},
    rpc::{
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
    time::timeout,
};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::frame::CloseFrame, tungstenite::protocol::Message,
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, warn};
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
        if let Ok(resp) = serde_json::from_str::<BinanceAccountStatusResponse>(&text) {
            return Ok(BinanceInbound::Rpc(BinanceRpcResponse::AccountStatus(resp)));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsOrderResponse>(&text) {
            return Ok(BinanceInbound::Rpc(BinanceRpcResponse::OpenOrder(resp)));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelOrderResponse>(&text) {
            return Ok(BinanceInbound::Rpc(BinanceRpcResponse::CancelOrder(resp)));
        }

        if let Ok(resp) = serde_json::from_str::<BinanceWsCancelAllOrdersResponse>(&text) {
            return Ok(BinanceInbound::Rpc(BinanceRpcResponse::CancelAll(resp)));
        }

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

#[derive(Debug, Clone)]
pub struct ConnectionSession {
    pub outbound_tx: mpsc::Sender<BinanceRpcRequest>,
}

impl ConnectionSession {
    pub async fn send_rpc_request(&self, request: BinanceRpcRequest) -> Result<(), EngineError> {
        self.outbound_tx
            .send(request)
            .await
            .map_err(|_| EngineError::new_custom_error("Failed to send RPC request".into()))
    }
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
        params.insert("apiKey".into(), self.signer.api_key.clone());
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
        signed_params.insert("apiKey".into(), self.signer.api_key.clone());
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
        signed_params.insert("apiKey".into(), self.signer.api_key.clone());
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        signed_params.insert("side".into(), params.side.to_string().into());
        signed_params.insert("type".into(), params.order_type.to_string().into());
        signed_params.insert(
            "timeInForce".into(),
            params.time_in_force.to_string().into(),
        );
        signed_params.insert("price".into(), params.price.to_string().into());
        signed_params.insert("quantity".into(), params.quantity.to_string().into());

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
        signed_params.insert("apiKey".into(), self.signer.api_key.clone());
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        signed_params.insert(
            "origClientOrderId".into(),
            params.orig_client_order_id.clone(),
        );
        signed_params.insert("type".into(), params.order_type.to_string().into());

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
        signed_params.insert("apiKey".into(), self.signer.api_key.clone());
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
        inbound_tx: mpsc::Sender<BinanceInbound>,
        ping_tx: mpsc::Sender<Vec<u8>>,
        handler: H,
    ) where
        H: MessageHandler<BinanceInbound> + Send + Sync + 'static,
        R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin
            + Send
            + 'static,
    {
        tokio::spawn(async move {
            debug!("WebSocket reader task started");
            let mut message_count = 0u64;

            while let Some(message) = reader.next().await {
                message_count += 1;
                match message {
                    Ok(Message::Text(txt)) => {
                        // Check if this looks like an RPC response (has "id" field)
                        let is_rpc_response = txt.contains("\"id\"")
                            && (txt.contains("\"status\"") || txt.contains("\"result\""));
                        if is_rpc_response {
                            debug!(
                                "Reader received RPC response #{count}: {text}",
                                count = message_count,
                                text = txt
                            );
                        } else {
                            debug!(
                                "Reader received text message #{count}: {text}",
                                count = message_count,
                                text = txt
                            );
                        }
                        match handler.handle_text_message(txt.into()) {
                            Ok(BinanceInbound::Rpc(rpc)) => {
                                debug!("Parsed as RPC response, forwarding to handler...");
                                if inbound_tx.send(BinanceInbound::Rpc(rpc)).await.is_err() {
                                    warn!("Failed to send RPC to inbound channel (channel closed)");
                                    break;
                                }
                            }
                            Ok(BinanceInbound::Event(evt)) => {
                                debug!("Parsed as event, forwarding...");
                                if inbound_tx.send(BinanceInbound::Event(evt)).await.is_err() {
                                    warn!(
                                        "Failed to send event to inbound channel (channel closed)"
                                    );
                                    break;
                                }
                            }
                            Err(e) => {
                                error!(
                                    "WS parse error on message #{count}: {e}",
                                    count = message_count
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
                            "Reader received Ping #{count} (payload_len={len}, payload={payload:?})",
                            count = message_count,
                            len = payload.len(),
                            payload = payload
                        );
                        handler.handle_ping(payload.clone());
                        if ping_tx.send(payload).await.is_err() {
                            warn!("Failed to send ping to ping channel");
                        }
                    }
                    Ok(Message::Pong(p)) => {
                        debug!(
                            "Reader received Pong #{count} (payload_len={len})",
                            count = message_count,
                            len = p.len()
                        );
                        handler.handle_pong(p);
                    }
                    Ok(Message::Close(c)) => {
                        if let Some(close_frame) = &c {
                            error!(
                                "WebSocket closed by server (message #{count}): code={code}, reason={reason}",
                                count = message_count,
                                code = close_frame.code,
                                reason = close_frame.reason
                            );
                        } else {
                            error!(
                                "WebSocket closed by server (message #{count}, no close frame)",
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
                            "WebSocket read error on message #{count}: {e}",
                            count = message_count
                        );
                        // Close the inbound channel on error
                        // inbound_tx will be dropped when it goes out of scope
                        break;
                    }
                }
            }
            debug!(
                "WebSocket reader task exiting (processed {count} messages total)",
                count = message_count
            );
            // inbound_tx is automatically dropped here when it goes out of scope
        });
    }

    fn handle_write_stream(
        &self,
        mut outbound_rx: mpsc::Receiver<BinanceRpcRequest>,
        mut ping_rx: mpsc::Receiver<Vec<u8>>,
        ws_write: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    ) {
        tokio::spawn(async move {
            debug!("WebSocket writer task started");
            let mut rpc_count = 0u64;
            let mut pong_count = 0u64;

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
                        };
                        match json {
                            Ok(payload) => {
                                debug!("Writer sending RPC #{count} ({type}) to WebSocket: {payload}",
                                    count = rpc_count,
                                    type = rpc_type,
                                    payload = payload
                                );
                                let mut w = ws_write.lock().await;
                                if let Err(e) = w.send(Message::Text(payload)).await {
                                    error!("Failed to send RPC #{count} ({type}): {e:?}",
                                        count = rpc_count,
                                        type = rpc_type
                                    );
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
                        debug!("Writer sending Pong #{count} (payload_len={len})",
                            count = pong_count,
                            len = p.len()
                        );
                        let mut w = ws_write.lock().await;
                        if let Err(e) = w.send(Message::Pong(p)).await {
                            error!("Failed to send Pong #{count}: {e:?}", count = pong_count);
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
                "WebSocket writer task exited (sent {rpc_count} RPCs, {pong_count} Pongs total)",
                rpc_count = rpc_count,
                pong_count = pong_count
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn split_stream(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        inbound_tx: mpsc::Sender<BinanceInbound>,
        outbound_tx: mpsc::Sender<BinanceRpcRequest>,
        outbound_rx: mpsc::Receiver<BinanceRpcRequest>,
        ping_tx: mpsc::Sender<Vec<u8>>,
        ping_rx: mpsc::Receiver<Vec<u8>>,
        handler: impl MessageHandler<BinanceInbound> + Send + Sync + 'static,
    ) -> Result<ConnectionSession, EngineError> {
        debug!("Splitting WebSocket stream into read/write halves...");
        debug!("Channel status: inbound_tx.is_closed={}, outbound_tx.is_closed={}, ping_tx.is_closed={}",
            inbound_tx.is_closed(),
            outbound_tx.is_closed(),
            ping_tx.is_closed()
        );

        let (ws_write, ws_read) = ws_stream.split();
        let ws_write = Arc::new(Mutex::new(ws_write));

        debug!("Starting writer and reader tasks...");
        self.handle_write_stream(outbound_rx, ping_rx, Arc::clone(&ws_write));
        self.handle_read_stream(ws_read, inbound_tx, ping_tx, handler);
        debug!("Reader and writer tasks spawned, connection fully established. Returning ConnectionSession with outbound_tx (is_closed={})",
            outbound_tx.is_closed()
        );

        Ok(ConnectionSession { outbound_tx })
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
