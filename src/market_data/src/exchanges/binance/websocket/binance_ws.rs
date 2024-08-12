use super::types::{
    AccountInfo, BinanceOrderbook, BinanceTrade, BinanceWSMessage, BinanceWsEventType,
    ParsedBinanceMessage,
};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use core_logic::{
    authenticator::WsAuthenticator,
    data::{Account, OrderBook, Trade},
    error::KolzoError,
    utils::{current_timestamp, generate_custom_uuid},
    ws_client::{MessageHandler, SubscriptionHandler, WebSocketClient},
};
use futures_util::{stream::StreamExt, SinkExt};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error};

type HmacSha256 = Hmac<Sha256>;

pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

pub struct BinanceWsAuthenticator {
    secret_key: String,
    api_key: String,
}

impl BinanceWsAuthenticator {
    pub fn new(api_key: &str, secret_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
        }
    }
}

#[async_trait]
impl WsAuthenticator for BinanceWsAuthenticator {
    async fn authenticate(&self, ws_url: &str) -> Result<String, KolzoError> {
        println!("Authenticating with URL: {}", ws_url);
        Ok(ws_url.to_string())
    }

    fn sign_subscription_message(&self, message: &str) -> Option<String> {
        println!("Signing subscription message: {}", message);

        let mut json_message: serde_json::Value = serde_json::from_str(message).ok()?;
        let params = json_message.get_mut("params")?.as_array_mut()?;

        let timestamp = current_timestamp().to_string();
        params.push(serde_json::Value::String(format!(
            "timestamp={}",
            timestamp
        )));

        let mut sorted_params: Vec<(String, String)> = params
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .flat_map(|s| {
                s.split('&')
                    .map(|pair| {
                        let mut split = pair.split('=');
                        let k = split.next().unwrap_or("").to_string();
                        let v = split.next().unwrap_or("").to_string();
                        (k, v)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        sorted_params.sort_by(|a, b| a.0.cmp(&b.0));

        let payload = sorted_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<String>>()
            .join("&");

        println!("Payload for signature: {}", payload);

        let key = self.secret_key.as_bytes();
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can use Sha256");
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();

        let encoded_signature = BASE64_STANDARD.encode(&code_bytes);
        params.push(serde_json::Value::String(format!(
            "signature={}",
            encoded_signature
        )));

        println!("Encoded signature: {}", encoded_signature);

        let signed_message = serde_json::to_string(&json_message).ok()?;
        println!("Signed message: {}", signed_message);
        Some(signed_message)
    }

    fn get_auth_headers(&self) -> Option<HashMap<String, String>> {
        let mut headers = HashMap::new();
        headers.insert("X-MBX-APIKEY".to_string(), self.api_key.clone());
        println!("Auth headers: {:?}", headers);
        Some(headers)
    }
}

#[derive(Debug, Clone)]
struct DataHandler;

impl DataHandler {
    fn handle_orderbook_event(&self, v: Value) -> Result<ParsedBinanceMessage, KolzoError> {
        println!("Handling orderbook event: {:?}", v);
        let orderbook: BinanceOrderbook =
            serde_json::from_value(v).map_err(KolzoError::JsonError)?;
        let general_orderbook: OrderBook = orderbook.into();
        Ok(ParsedBinanceMessage::OrderBook(general_orderbook))
    }

    fn handle_trade_event(&self, v: Value) -> Result<ParsedBinanceMessage, KolzoError> {
        println!("Handling trade event: {:?}", v);
        let trade: BinanceTrade = serde_json::from_value(v).map_err(KolzoError::JsonError)?;
        let general_trade: Trade = trade.into();
        Ok(ParsedBinanceMessage::Trade(general_trade))
    }

    fn handle_account_event(&self, v: Value) -> Result<ParsedBinanceMessage, KolzoError> {
        println!("Handling account event: {:?}", v);
        let account_info: AccountInfo = serde_json::from_value(v).map_err(KolzoError::JsonError)?;
        let general_account: Account = account_info.into();
        Ok(ParsedBinanceMessage::Account(general_account))
    }

    fn handle_event(
        &self,
        event_type: BinanceWsEventType,
        v: Value,
    ) -> Result<ParsedBinanceMessage, KolzoError> {
        println!("Handling event of type {:?}: {:?}", event_type, v);
        match event_type {
            BinanceWsEventType::DepthUpdate => self.handle_orderbook_event(v),
            BinanceWsEventType::Trade => self.handle_trade_event(v),
            BinanceWsEventType::Account => self.handle_account_event(v),
            BinanceWsEventType::Unknown => Err(KolzoError::new_custom_error(
                "Could not find relevant Binance WS message type".into(),
            )),
        }
    }
}

impl MessageHandler<ParsedBinanceMessage> for DataHandler {
    fn handle_text_message(&self, text: String) -> Result<ParsedBinanceMessage, KolzoError> {
        println!("Handling text message: {}", text);
        let v: Value = serde_json::from_str(&text).map_err(KolzoError::JsonError)?;

        if let Some(event_type_str) = v.get("e").and_then(|e| e.as_str()) {
            let event_type = match event_type_str {
                "depthUpdate" => BinanceWsEventType::DepthUpdate,
                "trade" => BinanceWsEventType::Trade,
                "account.status" => BinanceWsEventType::Account,
                _ => BinanceWsEventType::Unknown,
            };
            self.handle_event(event_type, v)
        } else {
            Ok(ParsedBinanceMessage::Raw(
                "No event type found in message. Not processing.".to_string(),
            ))
        }
    }

    fn handle_ping(&self, ping: Vec<u8>) {
        println!("Received ping: {:?}", ping);
        // Handle ping if necessary
    }
}

#[derive(Debug, Clone)]
pub enum BinanceChannel {
    Public(BinanceChannelPublic),
    Private(BinanceChannelPrivate),
}

#[derive(Debug, Clone)]
pub enum BinanceChannelPublic {
    Orderbook { base: String, quote: String },
    Trade { base: String, quote: String },
}

#[derive(Debug, Clone)]
pub enum BinanceChannelPrivate {
    AccountStatus,
    AccountRateLimits,
}

#[derive(Debug, Clone)]
pub enum BinanceSubscriptionHandler {
    Public(BinanceChannelPublic),
    Private(BinanceChannelPrivate),
}

impl BinanceSubscriptionHandler {
    pub fn new_public(channel_info: BinanceChannelPublic) -> Self {
        Self::Public(channel_info)
    }

    pub fn new_private(channel_info: BinanceChannelPrivate) -> Self {
        Self::Private(channel_info)
    }
}

impl SubscriptionHandler for BinanceSubscriptionHandler {
    fn create_subscription_request(&self) -> Message {
        println!("Creating subscription request for: {:?}", self);
        match self {
            BinanceSubscriptionHandler::Public(channel) => {
                let channel_name = match channel {
                    BinanceChannelPublic::Orderbook { base, quote } => {
                        format!("{}{}@depth", base, quote)
                    }
                    BinanceChannelPublic::Trade { base, quote } => {
                        format!("{}{}@trade", base, quote)
                    }
                };

                let subscription_msg = BinanceWSMessage {
                    method: "SUBSCRIBE".into(),
                    params: vec![channel_name.into()],
                    id: "1".into(),
                };

                let json_msg = serde_json::to_string(&subscription_msg).unwrap();
                println!("Public subscription message: {}", json_msg);

                Message::Text(json_msg)
            }
            BinanceSubscriptionHandler::Private(channel) => {
                let channel_name = match channel {
                    BinanceChannelPrivate::AccountStatus => "account".into(),
                    BinanceChannelPrivate::AccountRateLimits => "account".into(),
                };

                let subscription_msg = BinanceWSMessage {
                    method: "SUBSCRIBE".into(),
                    params: vec![channel_name],
                    id: generate_custom_uuid(),
                };

                let json_msg = serde_json::to_string(&subscription_msg).unwrap();
                println!("Private subscription message: {}", json_msg);

                Message::Text(json_msg)
            }
        }
    }
}

pub struct BinanceWSClient {
    url: String,
    subscription_handler: BinanceSubscriptionHandler,
    authenticator: Option<Box<dyn WsAuthenticator>>,
}

impl BinanceWSClient {
    pub fn new_public(
        channel_info: BinanceChannelPublic,
        authenticator: Option<Box<dyn WsAuthenticator>>,
    ) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new_public(channel_info),
            authenticator,
        }
    }

    pub fn new_private(
        channel_info: BinanceChannelPrivate,
        authenticator: Option<Box<dyn WsAuthenticator>>,
    ) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new_private(channel_info),
            authenticator,
        }
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, KolzoError> {
        println!("Creating WebSocket stream with URL: {}", self.url);
        self.make_ws_request(&self.url, self.authenticator.as_deref())
            .await
    }

    pub async fn run(&self, tx: mpsc::Sender<ParsedBinanceMessage>) -> Result<(), KolzoError> {
        println!("Running WebSocket client...");
        let mut ws_stream = self.create_ws_stream().await?;
        self.subscribe_to_channels(
            &mut ws_stream,
            self.subscription_handler.clone(),
            self.authenticator.as_deref(),
        )
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
        authenticator: Option<&dyn WsAuthenticator>,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, KolzoError> {
        println!("Making WebSocket request to URL: {}", ws_url);
        let authenticated_url = if let Some(auth) = authenticator {
            let auth_url = auth.authenticate(ws_url).await?;
            println!("Authenticated URL: {}", auth_url);
            auth_url
        } else {
            ws_url.to_string()
        };

        let (ws_stream, _) = connect_async(&authenticated_url).await?;
        println!("WebSocket connected.");
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
                    Ok(Message::Text(text)) => {
                        println!("Received text message: {}", text);
                        match handler.handle_text_message(text) {
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
                    Ok(Message::Ping(ping)) => {
                        println!("Received ping: {:?}", ping);
                        handler.handle_ping(ping.clone());
                        if let Err(e) = ws_stream.send(Message::Pong(ping)).await {
                            error!("Error sending pong: {}", e);
                        }
                    }
                    Ok(Message::Pong(pong)) => {
                        println!("Received pong: {:?}", pong);
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
        authenticator: Option<&dyn WsAuthenticator>,
    ) -> Result<(), KolzoError>
    where
        S: SubscriptionHandler + Send + Sync + 'static,
    {
        let subscription_message = subscription_handler.create_subscription_request();
        println!("Sending subscription message: {:?}", subscription_message);
        ws_stream.send(subscription_message).await?;
        Ok(())
    }
}
