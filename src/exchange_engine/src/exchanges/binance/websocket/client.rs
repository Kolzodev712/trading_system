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
use core_logic::{
    authenticator::ExchangeAuthenticator,
    error::EngineError,
    utils::generate_custom_uuid,
    ws_client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};
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
};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::frame::CloseFrame, tungstenite::protocol::Message,
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error};

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

    pub async fn create_account_status_request(&self) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.get_session().await?;

        let request = BinanceAccountStatusRequest {
            id: generate_custom_uuid().into(),
            method: ACCOUNT_STATUS.into(),
            params: BinanceAccountStatusParams {
                session_logon_params: session,
                recv_window: Some(60_000),
                omit_zero_balances: Some(true),
            },
        };

        Ok(BinanceRpcRequest::AccountStatus(request))
    }

    pub async fn create_test_order_request(
        &self,
        mut params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.get_session().await?;
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
        let session = self.get_session().await?;
        params.session = session;

        let request = BinanceWsOrderRequest {
            id: generate_custom_uuid().into(),
            method: OPEN_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::OpenOrder(request))
    }

    pub async fn create_cancel_order_request(
        &self,
        mut params: BinanceWsCancelOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.get_session().await?;
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
        let session = self.get_session().await?;
        params.session = session;

        let request = BinanceWsCancelAllOrdersRequest {
            id: generate_custom_uuid().into(),
            method: CANCEL_ORDER.into(),
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
        let ws_url = format!("{}", self.url);
        self.make_ws_request(&ws_url).await
    }

    pub async fn connect_and_authenticate(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let mut ws_stream = self.create_ws_stream().await?;

        let (logon_msg, logon_params) = self.subscription_handler.create_logon_message()?;
        ws_stream.send(logon_msg).await?;

        if let Some(Ok(Message::Text(logon_response))) = ws_stream.next().await {
            let v: Value = serde_json::from_str(&logon_response)?;
            if v.get("status").and_then(|s| s.as_u64()) == Some(200) {
                debug!("✅ Successfully authenticated with session.logon");
                self.subscription_handler.set_session(logon_params).await;
            } else {
                return Err(EngineError::new_custom_error(
                    format!("❌ session.logon failed: {:?}", v).into(),
                ));
            }
        }

        Ok(ws_stream)
    }

    pub async fn subscribe_to_user_stream(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> Result<(), EngineError> {
        if let Some(subscription_message) = self.subscription_handler.create_subscription_request()
        {
            debug!("📩 Subscribing to userDataStream...");
            ws_stream.send(subscription_message).await?;
        } else {
            error!("❌ Failed to create subscription message");
        }

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
            while let Some(message) = reader.next().await {
                match message {
                    Ok(Message::Text(txt)) => match handler.handle_text_message(txt.into()) {
                        Ok(BinanceInbound::Rpc(rpc)) => {
                            let _ = inbound_tx.send(BinanceInbound::Rpc(rpc)).await;
                        }
                        Ok(BinanceInbound::Event(evt)) => {
                            let _ = inbound_tx.send(BinanceInbound::Event(evt)).await;
                        }
                        Err(e) => error!("WS parse error: {}", e),
                    },
                    Ok(Message::Binary(bin)) => handler.handle_binary(bin),
                    Ok(Message::Ping(payload)) => {
                        handler.handle_ping(payload.clone());
                        let _ = ping_tx.send(payload).await;
                    }
                    Ok(Message::Pong(p)) => handler.handle_pong(p),
                    Ok(Message::Close(c)) => handler.handle_close(c),
                    Err(e) => error!("WS read error: {}", e),
                }
            }
            debug!("💀 reader.next() returned None — stream closed.");
        });
    }

    fn handle_write_stream(
        &self,
        mut outbound_rx: mpsc::Receiver<BinanceRpcRequest>,
        mut ping_rx: mpsc::Receiver<Vec<u8>>,
        ws_write: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    ) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(rpc) = outbound_rx.recv() => {
                        let json = match rpc {
                            BinanceRpcRequest::AccountStatus(req) => serde_json::to_string(&req),
                            BinanceRpcRequest::TestOrder(req)     => serde_json::to_string(&req),
                            BinanceRpcRequest::OpenOrder(req)     => serde_json::to_string(&req),
                            BinanceRpcRequest::CancelOrder(req)   => serde_json::to_string(&req),
                            BinanceRpcRequest::CancelAll(req)     => serde_json::to_string(&req),
                        };
                        match json {
                            Ok(payload) => {
                                debug!("📤 Sending RPC JSON: {}", payload);
                                let mut w = ws_write.lock().await;
                                if let Err(e) = w.send(Message::Text(payload)).await {
                                    error!("❌ Failed to send RPC: {:?}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("❌ Serialization error: {:?}", e);
                                break;
                            }
                        }
                    }
                        Some(p) = ping_rx.recv() => {
                        let mut w = ws_write.lock().await;
                        if let Err(e) = w.send(Message::Pong(p)).await {
                            error!("❌ Failed to send Pong: {:?}", e);
                            break;
                        }
                    }

                    else => {
                        debug!("Both RPC and ping channels closed, writer exiting");
                        break;
                    }
                }
            }

            debug!("WebSocket writer task exited");
        });
    }

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
        let (ws_write, ws_read) = ws_stream.split();
        let ws_write = Arc::new(Mutex::new(ws_write));

        self.handle_write_stream(outbound_rx, ping_rx, Arc::clone(&ws_write));
        self.handle_read_stream(ws_read, inbound_tx, ping_tx, handler);

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
        mut _ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        _event_tx: mpsc::Sender<ParsedBinanceMessage>,
        _handler: H,
    ) -> Result<(), EngineError>
    where
        H: MessageHandler<ParsedBinanceMessage> + Send + Sync + 'static,
    {
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
