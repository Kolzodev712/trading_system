use super::types::{
    common::{BinanceWSMessage, BinanceWsEventType, InitialResponse, ParsedBinanceMessage},
    orderbook::BinanceOrderbook,
    trade::BinanceTrade,
};
use async_trait::async_trait;
use futures_util::{stream::StreamExt, SinkExt};
use serde_json::Value;
use smol_str::SmolStr;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{CloseFrame, Message},
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error};
use trading_core::{
    error::EngineError,
    types::{
        common::DataType,
        market_data::{LegacyOrderbook as Orderbook, LegacyTrade as Trade},
    },
    utils::generate_custom_uuid,
    websocket::client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};

pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

#[derive(Debug, Clone)]
pub struct BinanceChannel {
    pub channel: DataType,
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
    fn create_subscription_request(&self) -> Option<Message> {
        let channel_name = match self.channel_info.channel {
            DataType::Orderbook => "depth",
            DataType::Trade => "trade",
            _ => "unknown",
        };

        let subscription_msg = BinanceWSMessage {
            method: "SUBSCRIBE".into(),
            params: vec![format!(
                "{}{}@{}",
                self.channel_info.base, self.channel_info.quote, channel_name
            )
            .into()],
            id: generate_custom_uuid().into(),
        };

        let json_msg = serde_json::to_string(&subscription_msg).unwrap();
        Some(Message::Text(json_msg))
    }
}

#[derive(Debug, Clone)]
struct DataHandler;

impl DataHandler {
    fn handle_initial_response(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let init_resp: InitialResponse =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::InitialResponse(init_resp))
    }

    fn handle_orderbook_event(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let orderbook: BinanceOrderbook =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        let general_orderbook: Orderbook = orderbook.into();
        Ok(ParsedBinanceMessage::OrderBook(general_orderbook))
    }

    fn handle_trade_event(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let trade: BinanceTrade = serde_json::from_value(v).map_err(EngineError::JsonError)?;
        let general_trade: Trade = trade.into();
        Ok(ParsedBinanceMessage::Trade(general_trade))
    }

    fn handle_event(
        &self,
        event_type: BinanceWsEventType,
        v: Value,
    ) -> Result<ParsedBinanceMessage, EngineError> {
        match event_type {
            BinanceWsEventType::InitialResponse => self.handle_initial_response(v),
            BinanceWsEventType::DepthUpdate => self.handle_orderbook_event(v),
            BinanceWsEventType::Trade => self.handle_trade_event(v),
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
        debug!("Received text message: {text}");

        let v: Value = serde_json::from_str(&text).map_err(EngineError::JsonError)?;

        if let (Some(_), Some(_)) = (v.get("result"), v.get("id")) {
            debug!("Subscription acknowledgment received: {text}");
            return Ok(serde_json::from_value(v).map(ParsedBinanceMessage::InitialResponse)?);
        }

        if let Some(data) = v.get("e").and_then(|e| e.as_str()) {
            let event_type = match data {
                "depthUpdate" => BinanceWsEventType::DepthUpdate,
                "trade" => BinanceWsEventType::Trade,
                _ => BinanceWsEventType::Unknown,
            };
            return self.handle_event(event_type, v);
        }

        debug!("Unknown or unsupported message format: {text}");
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
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        self.make_ws_request(&self.url).await
    }

    pub async fn run(&self, tx: mpsc::Sender<ParsedBinanceMessage>) -> Result<(), EngineError> {
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
                                    // Channel closed - expected during shutdown
                                    debug!("Channel closed during message send (shutdown): {e}");
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Error processing text message: {e}");
                            }
                        }
                    }
                    Ok(Message::Binary(binary)) => {
                        handler.handle_binary(binary);
                    }
                    Ok(Message::Ping(ping)) => {
                        handler.handle_ping(ping.clone());
                        if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
                            error!("Error sending pong: {e}");
                        }
                    }
                    Ok(Message::Pong(pong)) => {
                        handler.handle_pong(pong);
                    }
                    Err(e) => {
                        error!("Error while receiving a message: {e}");
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
            ws_stream.send(subscription_message).await?;
        } else {
            error!("Subscription message not found");
        }

        Ok(())
    }
}
