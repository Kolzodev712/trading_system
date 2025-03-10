use super::types::{
    balance::BinanceBalance,
    common::{BinanceWSMessage, BinanceWsEventType, Login, ParsedBinanceMessage},
};
use crate::exchanges::binance::sign::BinanceAuthenticator;
use async_trait::async_trait;
use core_logic::{
    authenticator::ExchangeAuthenticator,
    data::{base_data::DataType, private_data::Balance},
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
    fn create_authenticated_request(&self, params: BTreeMap<SmolStr, SmolStr>) -> Option<Message> {
        let method = match self.channel_info.channel {
            DataType::Account => "account.status",
            _ => "unknown",
        };

        let subscription_msg = BinanceWSMessage {
            method: method.into(),
            params: params.into_iter().map(|(_, v)| v).collect(),
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
    fn handle_login_response(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let init_resp: Login = serde_json::from_value(v).map_err(EngineError::JsonError)?;
        Ok(ParsedBinanceMessage::Login(init_resp))
    }

    fn handle_balance_event(&self, v: Value) -> Result<ParsedBinanceMessage, EngineError> {
        let balance_update: BinanceBalance =
            serde_json::from_value(v).map_err(EngineError::JsonError)?;
        let general_balance: Balance = balance_update.into();
        Ok(ParsedBinanceMessage::Balance(general_balance))
    }

    fn handle_event(
        &self,
        event_type: BinanceWsEventType,
        v: Value,
    ) -> Result<ParsedBinanceMessage, EngineError> {
        match event_type {
            BinanceWsEventType::Login => self.handle_login_response(v),
            BinanceWsEventType::Balance => self.handle_balance_event(v),
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
        debug!("Received text message: {}", text);

        let v: Value = serde_json::from_str(&text).map_err(EngineError::JsonError)?;

        if let (Some(_), Some(_)) = (v.get("result"), v.get("id")) {
            debug!("Subscription acknowledgment received: {}", text);
            return Ok(serde_json::from_value(v).map(ParsedBinanceMessage::Login)?);
        }

        if let Some(data) = v.get("e").and_then(|e| e.as_str()) {
            let event_type = match data {
                "account.status" => BinanceWsEventType::Balance,
                _ => BinanceWsEventType::Unknown,
            };
            return self.handle_event(event_type, v);
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
    subscription_handler: BinanceSubscriptionHandler,
}

impl BinanceWSClient {
    pub fn new(channel_info: BinanceChannel) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new(channel_info),
        }
    }

    async fn sign_and_prepare_params(
        &self,
        api_key: &SmolStr,
        secret_key: &SmolStr,
        timestamp: u64,
    ) -> Result<SmolStr, EngineError> {
        let mut params: BTreeMap<SmolStr, SmolStr> = BTreeMap::new();
        params.insert("apiKey".into(), api_key.clone());
        params.insert("server_time".into(), timestamp.to_string().into());

        let authenticator = BinanceAuthenticator {
            api_key: api_key.clone(),
            secret_key: secret_key.clone(),
        };

        let signed_params = authenticator.build_signed_params(params).await?;

        signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::AuthenticationError(anyhow::anyhow!("Signature not found")))
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        self.make_ws_request(&self.url).await
    }

    pub async fn run(
        &self,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        api_key: SmolStr,
        secret_key: SmolStr,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        let mut ws_stream = self.create_ws_stream().await?;

        let mut auth_params: BTreeMap<SmolStr, SmolStr> = BTreeMap::new();
        auth_params.insert("apiKey".into(), api_key.clone());
        auth_params.insert("server_time".into(), timestamp.to_string().into());

        self.authenticate_ws(&mut ws_stream, &api_key, &secret_key, timestamp)
            .await?;

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
            ws_stream.send(subscription_message).await?
        } else {
            error!("Subscription message not found");
        }

        Ok(())
    }

    async fn authenticate_ws(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        api_key: &SmolStr,
        secret_key: &SmolStr,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        let signature = self
            .sign_and_prepare_params(api_key, secret_key, timestamp)
            .await?;

        let auth_msg = BinanceWSMessage {
            method: "session.logon".into(),
            params: vec![api_key.clone(), signature, timestamp.to_string().into()],
            id: generate_custom_uuid().into(),
        };

        let json_msg = serde_json::to_string(&auth_msg).unwrap();
        let message = Message::Text(json_msg);

        ws_stream.send(message).await.map_err(EngineError::from)?;

        Ok(())
    }
}
