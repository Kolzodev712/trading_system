use super::types::{
    common::{BybitSubscriptionRequest, BybitSubscriptionResponse, ParsedBybitMessage},
    orderbook::BybitOrderbookMessage,
    trade::BybitTradeMessage,
};
use crate::exchanges::bybit::config::BybitCategory;
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
    websocket::client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};

pub fn get_bybit_ws_url(category: &BybitCategory) -> &'static str {
    match category {
        BybitCategory::Spot => "wss://stream.bybit.com/v5/public/spot",
        BybitCategory::Linear => "wss://stream.bybit.com/v5/public/linear",
    }
}

#[derive(Debug, Clone)]
pub struct BybitChannel {
    pub channel: DataType,
    pub base: SmolStr,
    pub quote: SmolStr,
    pub limit: u64,
}

#[derive(Debug, Clone)]
pub struct BybitSubscriptionHandler {
    channels: Vec<BybitChannel>,
}

impl BybitSubscriptionHandler {
    pub fn new(channels: Vec<BybitChannel>) -> Self {
        Self { channels }
    }
}

impl SubscriptionHandler for BybitSubscriptionHandler {
    fn create_subscription_request(&self) -> Option<Message> {
        // Build topic strings for all pairs
        let topics: Vec<SmolStr> = self
            .channels
            .iter()
            .map(|ch| {
                let symbol = format!("{}{}", ch.base.to_uppercase(), ch.quote.to_uppercase());
                match ch.channel {
                    DataType::Orderbook => format!("orderbook.{}.{}", ch.limit, symbol).into(),
                    DataType::Trade => format!("publicTrade.{}", symbol).into(),
                    _ => {
                        // Unsupported data type, return empty string (will be filtered out)
                        SmolStr::new("")
                    }
                }
            })
            .filter(|topic| !topic.is_empty())
            .collect();

        let subscription_msg = BybitSubscriptionRequest {
            op: "subscribe".into(),
            args: topics,
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

    fn handle_subscription_response(&self, v: Value) -> Result<ParsedBybitMessage, EngineError> {
        let resp: BybitSubscriptionResponse =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBybitMessage::SubscriptionResponse(resp))
    }

    fn handle_orderbook_event(&self, v: Value) -> Result<ParsedBybitMessage, EngineError> {
        let bybit_orderbook: BybitOrderbookMessage =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        let symbol_str = bybit_orderbook.data.symbol.as_str();
        let pair = self.symbol_registry.get_or_parse_pair(symbol_str);
        let structured_orderbook = bybit_orderbook.into_with_pair(pair);
        Ok(ParsedBybitMessage::OrderBook {
            orderbook: structured_orderbook,
        })
    }

    fn handle_trade_event(&self, v: Value) -> Result<ParsedBybitMessage, EngineError> {
        let bybit_trade: BybitTradeMessage =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        // Bybit trade messages can contain multiple trades
        let structured_trades = bybit_trade.into_with_symbol_registry(self.symbol_registry.clone());

        // For now, we'll handle the first trade (or we could emit multiple)
        // Actually, let's handle all trades - but the message type only supports one
        // We'll need to handle this differently - for now, take the first one
        if let Some(trade) = structured_trades.first() {
            Ok(ParsedBybitMessage::Trade {
                trade: trade.clone(),
            })
        } else {
            debug!("Empty trade array in Bybit message");
            Err(EngineError::new_custom_error(
                "Empty trade array in Bybit message".into(),
            ))
        }
    }

    fn handle_event(&self, topic: &str, v: Value) -> Result<ParsedBybitMessage, EngineError> {
        if topic.starts_with("orderbook.") {
            self.handle_orderbook_event(v)
        } else if topic.starts_with("publicTrade.") {
            self.handle_trade_event(v)
        } else {
            debug!("Unknown topic: {topic}");
            Err(EngineError::new_custom_error(
                format!("Unknown Bybit topic: {}", topic).into(),
            ))
        }
    }
}

impl MessageHandler<ParsedBybitMessage> for DataHandler {
    fn handle_text_message(&self, text: SmolStr) -> Result<ParsedBybitMessage, EngineError> {
        debug!("Received text message: {text}");

        let v: Value = serde_json::from_str(&text).map_err(EngineError::JsonError)?;

        // Check if it's a subscription response
        if v.get("success").is_some() || v.get("ret_msg").is_some() {
            debug!("Subscription acknowledgment received: {text}");
            return self.handle_subscription_response(v);
        }

        // Check if it's a data message with topic
        if let Some(topic) = v.get("topic").and_then(|t| t.as_str()) {
            return self.handle_event(topic, v.clone());
        }

        debug!("Unknown or unsupported message format: {text}");
        Ok(ParsedBybitMessage::Raw(text))
    }

    fn handle_binary(&self, binary: Vec<u8>) {
        debug!("Received binary message: {:?}", binary);
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        debug!("Ping received: {:?}", ping);
    }

    fn handle_pong(&self, pong: Vec<u8>) {
        debug!("Pong received: {:?}", pong);
    }

    fn handle_close(&self, _close: Option<CloseFrame>) {
        debug!("Close frame received");
    }
}

pub struct BybitWSClient {
    url: SmolStr,
    subscription_handler: BybitSubscriptionHandler,
    symbol_registry: Arc<SymbolRegistry>,
}

impl BybitWSClient {
    pub fn new(
        channels: Vec<BybitChannel>,
        category: BybitCategory,
        symbol_registry: Arc<SymbolRegistry>,
    ) -> Self {
        Self {
            url: get_bybit_ws_url(&category).into(),
            subscription_handler: BybitSubscriptionHandler::new(channels),
            symbol_registry,
        }
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        self.make_ws_request(&self.url).await
    }

    pub async fn run(&self, tx: mpsc::Sender<ParsedBybitMessage>) -> Result<(), EngineError> {
        let mut ws_stream = self.create_ws_stream().await?;
        self.subscribe_to_channels(&mut ws_stream, self.subscription_handler.clone())
            .await?;
        let data_handler = DataHandler::new(self.symbol_registry.clone());
        self.handle_messages(ws_stream, tx, data_handler).await?;
        Ok(())
    }
}

#[async_trait]
impl WebSocketClient<ParsedBybitMessage> for BybitWSClient {
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
        tx: mpsc::Sender<ParsedBybitMessage>,
        handler: H,
    ) -> Result<(), EngineError>
    where
        H: MessageHandler<ParsedBybitMessage> + Send + Sync + 'static,
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
