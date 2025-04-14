use crate::exchanges::binance::sign::BinanceWsAuthenticator;

use super::types::{
    common::{SessionLogonMessage, SessionLogonParams},
    user_data::events::{BinanceWSMessage, ParsedBinanceMessage},
};
use async_trait::async_trait;
use core_logic::{
    authenticator::ExchangeAuthenticator,
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
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error};

pub const BINANCE_WS_URL: &str = "wss://ws-api.binance.com:443/ws-api/v3";

#[derive(Debug, Clone)]
pub struct BinanceChannel {
    pub base: SmolStr,
    pub quote: SmolStr,
    pub limit: u64,
}

#[derive(Debug, Clone)]
pub struct BinanceSubscriptionHandler {
    signer: BinanceWsAuthenticator,
}

impl BinanceSubscriptionHandler {
    pub fn new(signer: BinanceWsAuthenticator) -> Self {
        Self { signer }
    }

    fn create_logon_message(&self) -> Result<Message, EngineError> {
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
            .ok_or_else(|| {
                EngineError::new_custom_error("Missing timestamp in signed params".into())
            })?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params.get("signature").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing signature in signed params".into())
        })?;

        let logon_msg = SessionLogonMessage {
            id: generate_custom_uuid().into(),
            method: "session.logon".into(),
            params: SessionLogonParams {
                api_key,
                timestamp,
                signature,
            },
        };

        Ok(Message::Text(serde_json::to_string(&logon_msg)?))
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
    pub fn new(signer: BinanceWsAuthenticator) -> Self {
        Self {
            url: BINANCE_WS_URL.into(),
            subscription_handler: BinanceSubscriptionHandler::new(signer),
        }
    }

    pub async fn create_ws_stream(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let ws_url = format!("{}", self.url);
        self.make_ws_request(&ws_url).await
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

    pub async fn connect_and_authenticate(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, EngineError> {
        let mut ws_stream = self.create_ws_stream().await?;
        let logon_msg = self.subscription_handler.create_logon_message()?;
        ws_stream.send(logon_msg).await?;

        if let Some(Ok(Message::Text(logon_response))) = ws_stream.next().await {
            let v: Value = serde_json::from_str(&logon_response)?;
            if v.get("status").and_then(|s| s.as_u64()) == Some(200) {
                debug!("✅ Successfully authenticated with session.logon");
            } else {
                return Err(EngineError::new_custom_error(
                    format!("❌ session.logon failed: {:?}", v).into(),
                ));
            }
        }

        Ok(ws_stream)
    }

    pub async fn start_event_loop<H>(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        tx: mpsc::Sender<ParsedBinanceMessage>,
        handler: H,
    ) -> Result<(), EngineError>
    where
        H: MessageHandler<ParsedBinanceMessage> + Send + Sync + 'static,
    {
        self.handle_messages(ws_stream, tx, handler).await
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
