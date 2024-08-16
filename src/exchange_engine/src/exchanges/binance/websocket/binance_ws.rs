use super::types::{
    BinanceOrderbook, BinanceTrade, BinanceWSMessage, BinanceWsEventType, ParsedBinanceMessage,
};
use async_trait::async_trait;
use core_logic::{
    data::{
        base_data::DataType,
        public_data::{OrderBook, Trade},
    },
    error::KolzoError,
    utils::generate_custom_uuid,
    ws_client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};
use futures_util::{stream::StreamExt, SinkExt};
use serde_json::Value;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error};

pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

#[derive(Debug, Clone)]
pub struct BinanceChannel {
    pub channel: DataType,
    pub base: String,
    pub quote: String,
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
    fn create_subscription_request(&self) -> Message {
        let channel_name = match self.channel_info.channel {
            DataType::Orderbook => "depth",
            DataType::Trade => "trade",
        };

        let subscription_msg = BinanceWSMessage {
            method: "SUBSCRIBE".into(),
            params: vec![format!(
                "{}{}@{}",
                self.channel_info.base, self.channel_info.quote, channel_name
            )
            .into()],
            id: generate_custom_uuid(),
        };

        let json_msg = serde_json::to_string(&subscription_msg).unwrap();
        Message::Text(json_msg)
    }
}

#[derive(Debug, Clone)]
struct DataHandler;

impl DataHandler {
    fn handle_orderbook_event(&self, v: Value) -> Result<ParsedBinanceMessage, KolzoError> {
        let orderbook: BinanceOrderbook =
            serde_json::from_value(v).map_err(KolzoError::JsonError)?;
        let general_orderbook: OrderBook = orderbook.into();
        Ok(ParsedBinanceMessage::OrderBook(general_orderbook))
    }

    fn handle_trade_event(&self, v: Value) -> Result<ParsedBinanceMessage, KolzoError> {
        let trade: BinanceTrade = serde_json::from_value(v).map_err(KolzoError::JsonError)?;
        let general_trade: Trade = trade.into();
        Ok(ParsedBinanceMessage::Trade(general_trade))
    }

    fn handle_event(
        &self,
        event_type: BinanceWsEventType,
        v: Value,
    ) -> Result<ParsedBinanceMessage, KolzoError> {
        match event_type {
            BinanceWsEventType::DepthUpdate => self.handle_orderbook_event(v),
            BinanceWsEventType::Trade => self.handle_trade_event(v),
            BinanceWsEventType::Unknown => {
                debug!("Unknown event type: {:?}", v);
                Err(KolzoError::new_custom_error(
                    "Could not find relevant Binance WS message type".into(),
                ))
            }
        }
    }
}

impl MessageHandler<ParsedBinanceMessage> for DataHandler {
    fn handle_text_message(&self, text: String) -> Result<ParsedBinanceMessage, KolzoError> {
        debug!("Received text message: {}", text);

        let v: Value = serde_json::from_str(&text).map_err(KolzoError::JsonError)?;

        if let Some(event_type_str) = v.get("e").and_then(|e| e.as_str()) {
            let event_type = match event_type_str {
                "depthUpdate" => BinanceWsEventType::DepthUpdate,
                "trade" => BinanceWsEventType::Trade,
                _ => BinanceWsEventType::Unknown,
            };
            self.handle_event(event_type, v)
        } else {
            Ok(ParsedBinanceMessage::Raw(
                "No event type found in message. Not processing.".to_string(),
            ))
        }
    }

    fn handle_ping(&self, _ping: Vec<u8>) {
        println!("Ping received");
    }
}

pub struct BinanceWSClient {
    url: String,
    subscription_handler: BinanceSubscriptionHandler,
}

impl BinanceWSClient {
    pub fn new(channel_info: BinanceChannel) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new(channel_info),
        }
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, KolzoError> {
        self.make_ws_request(&self.url).await
    }

    pub async fn run(&self, tx: mpsc::Sender<ParsedBinanceMessage>) -> Result<(), KolzoError> {
        let mut ws_stream = self.create_ws_stream().await?;
        self.subscribe_to_channels(&mut ws_stream, self.subscription_handler.clone())
            .await?;
        self.handle_messages(ws_stream, tx, DataHandler).await?;
        Ok(())
    }
}

#[async_trait]
impl WebSocketClient<ParsedBinanceMessage> for BinanceWSClient {
    async fn make_ws_request(
        &self,
        ws_url: &str,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, KolzoError> {
        let (ws_stream, _) = connect_async(ws_url).await?;
        Ok(ws_stream)
    }

    async fn handle_messages<H>(
        &self,
        mut ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        handler: H,
    ) -> Result<(), KolzoError>
    where
        H: MessageHandler<ParsedBinanceMessage> + Send + Sync + 'static,
    {
        tokio::spawn(async move {
            while let Some(message) = ws_stream.next().await {
                match message {
                    Ok(Message::Text(text)) => match handler.handle_text_message(text.clone()) {
                        Ok(parsed_message) => {
                            if let Err(e) = tx.send(parsed_message).await {
                                error!("Error sending message to channel: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error processing text message: {}", e);
                        }
                    },
                    Ok(Message::Ping(ping)) => {
                        handler.handle_ping(ping.clone());
                        if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
                            error!("Error sending pong: {}", e);
                        }
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
    ) -> Result<(), KolzoError>
    where
        S: SubscriptionHandler + Send + Sync + 'static,
    {
        let subscription_message = subscription_handler.create_subscription_request();
        ws_stream.send(subscription_message).await?;
        Ok(())
    }
}
