use super::types::{
    balance::{BalanceUpdate, OutboundAccountPosition},
    common::{
        BinanceWSMessage, BinanceWsEventType, EventStreamTerminated, ExternalLockUpdate,
        ListStatus, ListenKeyExpired, ParsedBinanceMessage,
    },
    orders::ExecutionReport,
};
use async_trait::async_trait;
use core_logic::{
    error::EngineError,
    utils::generate_custom_uuid,
    ws_client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};
use futures_util::{stream::StreamExt, SinkExt};
use serde_json::Value;
use smol_str::SmolStr;
use std::collections::BTreeMap;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{CloseFrame, Message},
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, trace};

pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:443/ws";

#[derive(Debug, Clone)]
pub struct BinanceChannel {
    pub channel: BinanceWsEventType,
    pub base: SmolStr,
    pub quote: SmolStr,
    pub limit: u64,
}

#[derive(Debug, Clone)]
pub struct BinanceSubscriptionHandler {
    channel_info: BinanceChannel,
}

impl BinanceSubscriptionHandler {
    pub fn new(channel_info: BinanceChannel) -> Self {
        Self { channel_info }
    }
}

impl SubscriptionHandler for BinanceSubscriptionHandler {
    fn create_authenticated_request(&self, _params: BTreeMap<SmolStr, SmolStr>) -> Option<Message> {
        None
    }

    fn create_subscription_request(&self) -> Option<Message> {
        let method = match self.channel_info.channel {
            BinanceWsEventType::OutboundAccountPosition => "outboundAccountPosition",
            BinanceWsEventType::BalanceUpdate => "balanceUpdate",
            BinanceWsEventType::ExecutionReport => "executionReport",
            BinanceWsEventType::ListStatus => "listStatus",
            BinanceWsEventType::ListenKeyExpired => "listenKeyExpired",
            BinanceWsEventType::EventStreamTerminated => "eventStreamTerminated",
            BinanceWsEventType::ExternalLockUpdate => "externalLockUpdate",
            _ => return None,
        };

        let subscription_msg = BinanceWSMessage {
            method: "SUBSCRIBE".into(),
            params: vec![method.into()],
            id: generate_custom_uuid().into(),
        };

        let json_msg = serde_json::to_string(&subscription_msg).ok()?;
        Some(Message::Text(json_msg))
    }
}

#[derive(Debug, Clone)]
struct DataHandler;

impl DataHandler {
    fn handle_outbound_account_position(
        &self,
        v: Value,
    ) -> Result<ParsedBinanceMessage, EngineError> {
        let outbound_account_position: OutboundAccountPosition = serde_json::from_value(v)
            .map_err(|e| {
                error!("Error deserializing outbound account position: {}", e);
                EngineError::JsonError(e)
            })?;

        Ok(ParsedBinanceMessage::OutboundAccountPosition(
            outbound_account_position,
        ))
    }

    fn handle_balance_update(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let balance_update: BalanceUpdate =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::BalanceUpdate(balance_update))
    }

    fn handle_execution_report(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let execution_report: ExecutionReport =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::ExecutionReport(execution_report))
    }

    fn handle_list_status(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let list_status: ListStatus = serde_json::from_value(v).map_err(|e| {
            error!("Error deserializing list status: {}", e);
            EngineError::JsonError(e)
        })?;
        Ok(ParsedBinanceMessage::ListStatus(list_status))
    }

    fn handle_listen_key_expired(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let listen_key_expired: ListenKeyExpired = serde_json::from_value(v).map_err(|e| {
            error!("Error deserializing listen key expired: {}", e);
            EngineError::JsonError(e)
        })?;
        Ok(ParsedBinanceMessage::ListenKeyExpired(listen_key_expired))
    }

    fn handle_event_stream_terminated(
        &self,
        v: Value,
    ) -> Result<ParsedBinanceMessage, EngineError> {
        let event_stream_terminated: EventStreamTerminated =
            serde_json::from_value(v).map_err(|e| {
                error!("Error deserializing event stream terminated: {}", e);
                EngineError::JsonError(e)
            })?;
        Ok(ParsedBinanceMessage::EventStreamTerminated(
            event_stream_terminated,
        ))
    }

    fn handle_external_lock_update(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let external_lock_update: ExternalLockUpdate = serde_json::from_value(v).map_err(|e| {
            error!("Error deserializing external lock update: {}", e);
            EngineError::JsonError(e)
        })?;
        Ok(ParsedBinanceMessage::ExternalLockUpdate(
            external_lock_update,
        ))
    }

    fn convert_method_to_event_type(&self, method: &str) -> BinanceWsEventType {
        match method {
            "outboundAccountPosition" => BinanceWsEventType::OutboundAccountPosition,
            "balanceUpdate" => BinanceWsEventType::BalanceUpdate,
            "executionReport" => BinanceWsEventType::ExecutionReport,
            "listStatus" => BinanceWsEventType::ListStatus,
            "listenKeyExpired" => BinanceWsEventType::ListenKeyExpired,
            "eventStreamTerminated" => BinanceWsEventType::EventStreamTerminated,
            "externalLockUpdate" => BinanceWsEventType::ExternalLockUpdate,
            _ => BinanceWsEventType::Unknown,
        }
    }

    fn handle_event(&self, method: &str, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let event_type = self.convert_method_to_event_type(method);

        match event_type {
            BinanceWsEventType::OutboundAccountPosition => self.handle_outbound_account_position(v),
            BinanceWsEventType::BalanceUpdate => self.handle_balance_update(v),
            BinanceWsEventType::ExecutionReport => self.handle_execution_report(v),
            BinanceWsEventType::ListStatus => self.handle_list_status(v),
            BinanceWsEventType::ListenKeyExpired => self.handle_listen_key_expired(v),
            BinanceWsEventType::EventStreamTerminated => self.handle_event_stream_terminated(v),
            BinanceWsEventType::ExternalLockUpdate => self.handle_external_lock_update(v),
            BinanceWsEventType::Unknown => {
                debug!("Unknown event type: {:?}", v);
                Err(EngineError::new_custom_error(
                    "Could not find relevant Binance WS message type".into(),
                ))
            }
        }
    }
}

impl MessageHandler<ParsedBinanceMessage> for DataHandler {
    fn handle_text_message(&self, text: SmolStr) -> Result<ParsedBinanceMessage, EngineError> {
        let v: Value = serde_json::from_str(&text).map_err(EngineError::JsonError)?;

        if let Some(event_type) = v.clone().get("e").and_then(|e| e.as_str()) {
            return self.handle_event(event_type, v);
        } else {
            trace!("Unknown or unsupported message format: {}", text);
            Ok(ParsedBinanceMessage::Raw(text))
        }
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        debug!("Received binary message: {:?}", binary)
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        debug!("Ping received: {:?}", ping);
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        debug!("Seding Pong: {:?}", pong);
    }

    fn handle_close(&self, _close: Option<CloseFrame>) {
        unimplemented!()
    }
}

pub struct BinanceWSClient {
    url: SmolStr,
    listen_key: SmolStr,
    subscription_handler: BinanceSubscriptionHandler,
}

impl BinanceWSClient {
    pub fn new(listen_key: SmolStr, channel_info: BinanceChannel) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            listen_key,
            subscription_handler: BinanceSubscriptionHandler::new(channel_info),
        }
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let ws_url = format!("{}/{}", self.url, self.listen_key);
        self.make_ws_request(&ws_url).await
    }

    pub async fn run(&self, tx: mpsc::Sender<ParsedBinanceMessage>) -> Result<(), EngineError> {
        let mut ws_stream = self.create_ws_stream().await?;

        if let Some(subscription_message) = self.subscription_handler.create_subscription_request()
        {
            ws_stream.send(subscription_message).await?;
        } else {
            return Err(EngineError::new_custom_error(
                "Failed to create subscription message".into(),
            ));
        }

        self.handle_messages(ws_stream, tx, DataHandler).await?;

        Ok(())
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
        tx: mpsc::Sender<ParsedBinanceMessage>,
        handler: H,
    ) -> Result<(), EngineError>
    where
        H: MessageHandler<ParsedBinanceMessage> + Send + Sync + 'static,
    {
        tokio::spawn(async move {
            while let Some(message) = ws_stream.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        match handler.handle_text_message(text.clone().into()) {
                            Ok(parsed_message) => {
                                if let Err(e) = tx.send(parsed_message).await {
                                    error!("Error sending message to channel: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Error processing text message: {}", e);
                            }
                        }
                    }
                    Ok(Message::Binary(binary)) => {
                        handler.handle_binary(binary);
                    }
                    Ok(Message::Ping(ping)) => {
                        handler.handle_ping(ping.clone());
                        if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
                            error!("Error sending pong: {}", e);
                        }
                    }
                    Ok(Message::Pong(pong)) => {
                        handler.handle_pong(pong);
                    }
                    Err(e) => {
                        error!("Error while receiving a message: {}", e);
                    }
                    _ => {}
                }
            }
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
