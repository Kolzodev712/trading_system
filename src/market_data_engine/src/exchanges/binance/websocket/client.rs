use super::types::{
    common::{BinanceWSMessage, BinanceWsEventType, InitialResponse, ParsedBinanceMessage},
    orderbook::BinanceOrderbook,
    trade::BinanceTrade,
};
use async_trait::async_trait;
use futures_util::{stream::StreamExt, SinkExt};
use serde_json::Value;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{CloseFrame, Message},
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error};
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::{
    error::EngineError,
    types::common::DataType,
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
    channels: Vec<BinanceChannel>,
}

impl BinanceSubscriptionHandler {
    pub fn new(channels: Vec<BinanceChannel>) -> Self {
        Self { channels }
    }
}

impl SubscriptionHandler for BinanceSubscriptionHandler {
    fn create_subscription_request(&self) -> Option<Message> {
        let channel_name = match self.channels.first()?.channel {
            DataType::Orderbook => "depth",
            DataType::Trade => "trade",
            _ => "unknown",
        };

        // Build subscription params for all pairs
        let params: Vec<SmolStr> = self
            .channels
            .iter()
            .map(|ch| format!("{}{}@{}", ch.base, ch.quote, channel_name).into())
            .collect();

        let subscription_msg = BinanceWSMessage {
            method: "SUBSCRIBE".into(),
            params,
            id: generate_custom_uuid().into(),
        };

        let json_msg = serde_json::to_string(&subscription_msg).unwrap();
        Some(Message::Text(json_msg))
    }
}

#[derive(Debug, Clone)]
struct DataHandler {
    symbol_registry: Arc<SymbolRegistry>,
}

impl DataHandler {
    fn new(symbol_registry: Arc<SymbolRegistry>) -> Self {
        Self { symbol_registry }
    }

    fn handle_initial_response(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let init_resp: InitialResponse =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::InitialResponse(init_resp))
    }

    fn handle_orderbook_event(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let binance_orderbook: BinanceOrderbook =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        let pair = self
            .symbol_registry
            .get_or_parse_pair(binance_orderbook.symbol.as_str());
        let structured_orderbook = binance_orderbook.into_with_pair(pair);
        Ok(ParsedBinanceMessage::OrderBook {
            orderbook: structured_orderbook,
        })
    }

    fn handle_trade_event(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let binance_trade: BinanceTrade =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        let pair = self
            .symbol_registry
            .get_or_parse_pair(binance_trade.symbol.as_str());
        let structured_trade = binance_trade.into_with_pair(pair);
        Ok(ParsedBinanceMessage::Trade {
            trade: structured_trade,
        })
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
    symbol_registry: Arc<SymbolRegistry>,
}

impl BinanceWSClient {
    pub fn new(channels: Vec<BinanceChannel>, symbol_registry: Arc<SymbolRegistry>) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new(channels),
            symbol_registry,
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
        let data_handler = DataHandler::new(self.symbol_registry.clone());
        self.handle_messages(ws_stream, tx, data_handler).await?;
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
                        match handler.handle_text_message(text.as_str().into()) {
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
                        handler.handle_binary(binary.to_vec());
                    }
                    Ok(Message::Ping(ping)) => {
                        handler.handle_ping(ping.to_vec());
                        if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
                            error!("Error sending pong: {e}");
                        }
                    }
                    Ok(Message::Pong(pong)) => {
                        handler.handle_pong(pong.to_vec());
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
