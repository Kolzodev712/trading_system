use super::types::{
    balance::{BalanceUpdate, OutboundAccountPosition},
    common::{BinanceWSMessage, BinanceWsEventType, ParsedBinanceMessage},
    orders::ExecutionReport,
};
use async_trait::async_trait;
use core_logic::{
    data::private_data::Balance,
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
use tracing::{debug, error};

pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

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
        let method = match self.channel_info.channel {
            BinanceWsEventType::OutboundAccountPosition => "outboundAccountPosition",
            BinanceWsEventType::BalanceUpdate => "balanceUpdate",
            BinanceWsEventType::ExecutionReport => "executionReport",
            _ => return None,
        };

        let subscription_msg = BinanceWSMessage {
            method: "SUBSCRIBE".into(),
            params: vec![method.into()],
            id: generate_custom_uuid().into(),
        };

        let json_msg = serde_json::to_string(&subscription_msg).unwrap();
        Some(Message::Text(json_msg))
    }
    fn create_subscription_request(&self) -> Option<Message> {
        None
    }
}

#[derive(Debug, Clone)]
struct DataHandler;

impl DataHandler {
    fn handle_outbound_account_position(
        &self,
        v: Value,
    ) -> Result<ParsedBinanceMessage, EngineError> {
        debug!("Raw outbound account position response: {:?}", v);

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
        let general_balance: Balance = balance_update.into();
        Ok(ParsedBinanceMessage::BalanceUpdate(general_balance.into()))
    }

    fn handle_execution_report(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let execution_report: ExecutionReport =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::ExecutionReport(execution_report))
    }

    fn convert_method_to_event_type(&self, method: &str) -> BinanceWsEventType {
        match method {
            "outboundAccountPosition" => BinanceWsEventType::OutboundAccountPosition,
            "balanceUpdate" => BinanceWsEventType::BalanceUpdate,
            "executionReport" => BinanceWsEventType::ExecutionReport,
            _ => BinanceWsEventType::Unknown,
        }
    }

    fn handle_event(&self, method: &str, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        debug!("Raw event received: {:?}", v);
        let event_type = self.convert_method_to_event_type(method);

        match event_type {
            BinanceWsEventType::OutboundAccountPosition => self.handle_outbound_account_position(v),
            BinanceWsEventType::BalanceUpdate => self.handle_balance_update(v),
            BinanceWsEventType::ExecutionReport => self.handle_execution_report(v),
            BinanceWsEventType::Unknown => {
                debug!("Unknown event type: {:?}", v);
                Err(EngineError::new_custom_error(
                    "Could not find relevant Binance WS message type".into(),
                ))
            }
            _ => {
                debug!("Unsupported event type: {:?}", v);
                Err(EngineError::new_custom_error(
                    "Unsupported Binance WS message type".into(),
                ))
            }
        }
    }
}

impl MessageHandler<ParsedBinanceMessage> for DataHandler {
    fn handle_text_message(&self, text: SmolStr) -> Result<ParsedBinanceMessage, EngineError> {
        debug!("Received text message: {}", text);

        let v: Value = serde_json::from_str(&text).map_err(EngineError::JsonError)?;

        if let Some(method) = v.clone().get("method").and_then(|m| m.as_str()) {
            return self.handle_event(method, v);
        }

        debug!("Unknown or unsupported message format: {}", text);
        Ok(ParsedBinanceMessage::Raw(text))
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        println!("Received binary message: {:?}", binary)
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        println!("Ping received: {:?}", ping);
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        println!("Seding Pong: {:?}", pong);
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

        let params = BTreeMap::<SmolStr, SmolStr>::new(); // Add parameters like channel to subscribe to
        if let Some(subscription_message) = self
            .subscription_handler
            .create_authenticated_request(params)
        {
            ws_stream.send(subscription_message).await?;
        } else {
            error!("Failed to create subscription message");
            return Err(EngineError::new_custom_error(
                "failed to do some crap".into(),
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
                        debug!("Received WebSocket message: {}", text);
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
        if let Some(subscription_message) =
            subscription_handler.create_authenticated_request(BTreeMap::new())
        {
            debug!("Sending subscription message: {:?}", subscription_message);
            ws_stream.send(subscription_message).await?
        } else {
            error!("Subscription message not found");
        }

        Ok(())
    }
}

// use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
// use tokio::{sync::mpsc};
// use tokio_tungstenite::WebSocketStream;
// use futures_util::stream::StreamExt;
// use futures_util::SinkExt;
// use smol_str::SmolStr;
// use tracing::{debug, error};

// // Binance WebSocket URL
// pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

// pub struct BinanceWSClient {
//     url: SmolStr,
//     listen_key: SmolStr,
// }

// impl BinanceWSClient {
//     pub fn new(listen_key: SmolStr) -> Self {
//         Self {
//             url: BINANCE_WS_URL.into(),
//             listen_key,
//         }
//     }

//     // Create WebSocket stream and start listening
//     pub async fn create_ws_stream(&self) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, String>{
//         let ws_url = format!("{}/{}", self.url, self.listen_key);
//         self.make_ws_request(&ws_url).await
//     }

//     // Run WebSocket client: connect, listen and process messages
//     pub async fn run(&self, tx: mpsc::Sender<String>) -> Result<(), String> {
//         let ws_stream = self.create_ws_stream().await?;

//         // Handle incoming messages
//         self.handle_messages(ws_stream, tx).await?;

//         Ok(())
//     }

//     // Handle incoming WebSocket messages
//     async fn handle_messages(
//         &self,
//         mut ws_stream: WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
//         tx: mpsc::Sender<String>,
//     ) -> Result<(), String> {
//         // Listen for messages from the WebSocket stream
//         tokio::spawn(async move {
//             while let Some(message) = ws_stream.next().await {
//                 match message {
//                     Ok(Message::Text(text)) => {
//                         debug!("Received WebSocket message: {}", text);
//                         // Send received message as raw text
//                         if let Err(e) = tx.send(text).await {
//                             error!("Error sending message to channel: {}", e);
//                         }
//                     }
//                     Ok(Message::Ping(ping)) => {
//                         // Respond with pong if a ping is received
//                         debug!("Received ping: {:?}", ping);
//                         if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
//                             error!("Error sending pong: {}", e);
//                         }
//                     }
//                     Ok(Message::Pong(_pong)) => {
//                         debug!("Received pong");
//                     }
//                     Err(e) => {
//                         error!("Error during WebSocket communication: {}", e);
//                     }
//                     _ => {}
//                 }
//             }
//         });

//         Ok(())
//     }

//     // Make the WebSocket request
//     async fn make_ws_request(&self, ws_url: &str) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, String> {
//         let (ws_stream, _) = connect_async(ws_url).await.map_err(|e| e.to_string())?;
//         Ok(ws_stream)
//     }
// }
