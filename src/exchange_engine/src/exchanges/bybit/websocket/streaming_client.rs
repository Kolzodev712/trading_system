//! WebSocket streaming client for private data streams
//!
//! Handles positions, executions, orders, and wallet updates via WebSocket

use super::mapper::BybitMessageMapper;
use super::subscriber::{
    BybitSubscriber, TOPIC_EXECUTION, TOPIC_ORDER, TOPIC_POSITION, TOPIC_WALLET,
};
use super::types::ws_streaming::{BybitAuthRequest, BybitStreamingMessage, BybitWebSocketOp};
use crate::exchanges::bybit::authenticator::BybitAuthenticator;
use crate::exchanges::bybit::config::BybitConfig;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::{
    sync::{mpsc, Mutex},
    time::{interval, Duration},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use trading_core::{
    error::EngineError,
    registry::symbol::SymbolRegistry,
    types::exchange::{Balance, Order, Position, Trade},
    utils::generate_custom_uuid,
};

const HEARTBEAT_INTERVAL_SECS: u64 = 20;

pub struct BybitStreamingClient {
    config: BybitConfig,
    authenticator: BybitAuthenticator,
    mapper: Arc<BybitMessageMapper>,
}

#[derive(Debug, Clone)]
pub enum StreamingEvent {
    Position(Position),
    Execution(Trade),
    Order(Order),
    Wallet(Vec<Balance>),
}

impl BybitStreamingClient {
    pub fn new(config: BybitConfig, symbol_registry: Arc<SymbolRegistry>) -> Self {
        let authenticator = BybitAuthenticator::new(
            config.credentials.api_key.clone(),
            config.credentials.api_secret.clone(),
        );
        let mapper = Arc::new(BybitMessageMapper::new(symbol_registry));

        Self {
            config,
            authenticator,
            mapper,
        }
    }

    fn get_ws_url(&self) -> &str {
        self.config.private_ws_url()
    }

    fn generate_auth_signature(&self, expires: i64) -> Result<String, EngineError> {
        crate::exchanges::bybit::authenticator::websocket::generate_auth_signature(
            &self.authenticator.api_secret,
            expires,
        )
    }

    fn create_auth_request(&self) -> Result<BybitAuthRequest, EngineError> {
        // Use current time in milliseconds, add 10 seconds validity window
        // This gives enough time for the auth message to be processed
        let expires = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 10) as i64
            * 1000;

        let signature = self.generate_auth_signature(expires)?;
        let api_key = self.config.credentials.api_key.expose().to_string();

        Ok(BybitAuthRequest {
            req_id: Some(generate_custom_uuid().into()),
            op: BybitWebSocketOp::Auth,
            args: [api_key, expires.to_string(), signature],
        })
    }

    pub async fn run(
        &self,
        mut event_tx: mpsc::Sender<StreamingEvent>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        let url = self.get_ws_url();
        info!("Connecting to Bybit WebSocket streaming: {}", url);

        loop {
            if shutdown_rx.try_recv().is_ok() {
                info!("Shutdown signal received, stopping streaming client");
                return Ok(());
            }

            match self
                .connect_and_stream(url, &mut event_tx, &mut shutdown_rx)
                .await
            {
                Ok(_) => {
                    info!("Streaming client connection closed normally");
                    return Ok(());
                }
                Err(e) => {
                    error!("Streaming client error: {}, reconnecting in 5s...", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn connect_and_stream(
        &self,
        url: &str,
        event_tx: &mut mpsc::Sender<StreamingEvent>,
        shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), EngineError> {
        let (ws_stream, _) = connect_async(url).await.map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("WebSocket connect failed: {}", e))
        })?;

        info!("WebSocket connected, authenticating...");

        let (write, mut read) = ws_stream.split();
        let write: Arc<Mutex<_>> = Arc::new(Mutex::new(write));

        // Small delay to ensure connection is fully established before authenticating
        tokio::time::sleep(Duration::from_millis(100)).await;

        let auth_request = self.create_auth_request()?;
        let auth_json = serde_json::to_string(&auth_request).map_err(|e| {
            EngineError::CustomError(anyhow::anyhow!("Auth serialization failed: {}", e))
        })?;

        write
            .lock()
            .await
            .send(Message::Text(auth_json))
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Auth send failed: {}", e)))?;

        let mut authenticated = false;
        let mut message_count = 0u64;

        let mut heartbeat_interval = interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received in streaming client");
                    return Ok(());
                }
                _ = heartbeat_interval.tick() => {
                    let ping = BybitSubscriber::create_ping_request(Some(generate_custom_uuid().into()));
                    let ping_json = serde_json::to_string(&ping)
                        .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Ping serialization failed: {}", e)))?;
                    if let Err(e) = write.lock().await.send(Message::Text(ping_json)).await {
                        error!("Failed to send ping: {}", e);
                        return Err(EngineError::CustomError(anyhow::anyhow!("Ping send failed: {}", e)));
                    }
                    debug!("Sent heartbeat ping");
                }
                msg_result = read.next() => {
                    match msg_result {
                        Some(Ok(Message::Text(text))) => {
                            message_count += 1;
                            let text_str = text.to_string();
                            debug!("Received message #{}: {}", message_count, text_str);

                            match self.mapper.parse_message(&text_str) {
                                Ok(BybitStreamingMessage::AuthResponse(resp)) => {
                                    if resp.success {
                                        info!("Authentication successful, subscribing to topics...");
                                        authenticated = true;

                                        let topics = vec![TOPIC_POSITION, TOPIC_EXECUTION, TOPIC_ORDER, TOPIC_WALLET];
                                        let subscribe = BybitSubscriber::create_subscribe_request(topics, Some(generate_custom_uuid().into()));
                                        let subscribe_json = serde_json::to_string(&subscribe)
                                            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Subscribe serialization failed: {}", e)))?;

                                        write.lock().await.send(Message::Text(subscribe_json)).await
                                            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Subscribe send failed: {}", e)))?;
                                        info!("Subscribed to all topics");
                                    } else {
                                        return Err(EngineError::AuthenticationError(anyhow::anyhow!("Auth failed: {}", resp.ret_msg)));
                                    }
                                }
                                Ok(BybitStreamingMessage::PongResponse(_)) => {
                                    debug!("Received pong");
                                }
                                Ok(BybitStreamingMessage::SubscribeResponse(resp)) => {
                                    if resp.success {
                                        debug!("Subscription confirmed: {}", resp.ret_msg);
                                    } else {
                                        warn!("Subscription failed: {}", resp.ret_msg);
                                    }
                                }
                                Ok(BybitStreamingMessage::Position(pos)) => {
                                    if authenticated {
                                        if let Err(e) = event_tx.send(StreamingEvent::Position(pos)).await {
                                            error!("Failed to send position event: {}", e);
                                        }
                                    }
                                }
                                Ok(BybitStreamingMessage::Execution(trade)) => {
                                    if authenticated {
                                        if let Err(e) = event_tx.send(StreamingEvent::Execution(trade)).await {
                                            error!("Failed to send execution event: {}", e);
                                        }
                                    }
                                }
                                Ok(BybitStreamingMessage::Order(order)) => {
                                    if authenticated {
                                        if let Err(e) = event_tx.send(StreamingEvent::Order(order)).await {
                                            error!("Failed to send order event: {}", e);
                                        }
                                    }
                                }
                                Ok(BybitStreamingMessage::Wallet(balances)) => {
                                    if authenticated {
                                        if let Err(e) = event_tx.send(StreamingEvent::Wallet(balances)).await {
                                            error!("Failed to send wallet event: {}", e);
                                        }
                                    }
                                }
                                Ok(BybitStreamingMessage::Unknown(text)) => {
                                    debug!("Unknown message type: {}", text);
                                }
                                Err(e) => {
                                    warn!("Failed to parse message: {}", e);
                                }
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            debug!("Received ping from server, sending pong");
                            if let Err(e) = write.lock().await.send(Message::Pong(payload)).await {
                                error!("Failed to send pong: {}", e);
                                return Err(EngineError::CustomError(anyhow::anyhow!("Pong send failed: {}", e)));
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("Received pong from server");
                        }
                        Some(Ok(Message::Binary(_))) => {
                            debug!("Received binary message (ignoring)");
                        }
                        Some(Ok(Message::Frame(_))) => {}
                        Some(Ok(Message::Close(frame))) => {
                            if let Some(close_frame) = frame {
                                error!("WebSocket closed: code={}, reason={}", close_frame.code, close_frame.reason);
                            } else {
                                error!("WebSocket closed without close frame");
                            }
                            return Err(EngineError::CustomError(anyhow::anyhow!("WebSocket closed")));
                        }
                        Some(Err(e)) => {
                            error!("WebSocket read error: {}", e);
                            return Err(EngineError::CustomError(anyhow::anyhow!("WebSocket read error: {}", e)));
                        }
                        None => {
                            warn!("WebSocket stream ended");
                            return Err(EngineError::CustomError(anyhow::anyhow!("WebSocket stream ended")));
                        }
                    }
                }
            }
        }
    }
}
