//! Zenoh-based publish/subscribe and query-reply for events and commands.
//!
//! Requires the `zenoh` feature. Uses key expressions for routing; payloads are JSON bytes
//! from the [payload] module.

use crate::commands::{CommandPayload, CommandResponse};
use crate::error::NetworkError;
use crate::events::{ColdEvent, DexColdEvent, HotEvent};
use crate::payload;
use async_trait::async_trait;
use std::sync::Arc;

/// Open a Zenoh session with the given config.
pub async fn open_session(config: zenoh::Config) -> Result<zenoh::Session, NetworkError> {
    zenoh::open(config).await.map_err(NetworkError::Zenoh)
}

/// Publish a hot event to the given key expression.
pub async fn publish_hot_event(
    session: &zenoh::Session,
    key_expr: &str,
    event: &HotEvent,
) -> Result<(), NetworkError> {
    let payload_bytes = payload::serialize_hot_event(event)?;
    session
        .put(key_expr, payload_bytes.as_ref())
        .await
        .map_err(NetworkError::Zenoh)
}

/// Publish a cold event to the given key expression.
pub async fn publish_cold_event(
    session: &zenoh::Session,
    key_expr: &str,
    event: &ColdEvent,
) -> Result<(), NetworkError> {
    let payload_bytes = payload::serialize_cold_event(event)?;
    session
        .put(key_expr, payload_bytes.as_ref())
        .await
        .map_err(NetworkError::Zenoh)
}

/// Publish a DEX cold event to the given key expression.
pub async fn publish_dex_cold_event(
    session: &zenoh::Session,
    key_expr: &str,
    event: &DexColdEvent,
) -> Result<(), NetworkError> {
    let payload_bytes = payload::serialize_dex_cold_event(event)?;
    session
        .put(key_expr, payload_bytes.as_ref())
        .await
        .map_err(NetworkError::Zenoh)
}

/// Handler for incoming commands (used by the command queryable).
#[async_trait]
pub trait CommandQueryHandler: Send + Sync + 'static {
    /// Handle a command payload and return a response.
    async fn handle(&self, payload: CommandPayload) -> Result<CommandResponse, String>;
}

/// Run a Zenoh queryable that receives commands and replies with responses.
/// Deserializes incoming query payloads to `CommandPayload`, calls `handler.handle()`,
/// then serializes the `CommandResponse` into the reply.
pub async fn run_command_queryable<H: CommandQueryHandler>(
    session: &zenoh::Session,
    key_expr: &str,
    handler: Arc<H>,
) -> Result<(), NetworkError> {
    let queryable = session
        .declare_queryable(key_expr)
        .await
        .map_err(NetworkError::Zenoh)?;
    while let Ok(query) = queryable.recv_async().await {
        let payload_bytes: Vec<u8> = query
            .payload()
            .map(|p| p.to_bytes().into_owned())
            .unwrap_or_default();
        let cmd_payload = match payload::deserialize_command(&payload_bytes) {
            Ok(p) => p,
            Err(e) => {
                let _ = query
                    .reply(
                        query.key_expr().clone(),
                        serde_json::to_vec(&CommandResponse {
                            success: false,
                            message: Some(e.to_string()),
                            data: None,
                        })
                        .unwrap_or_default(),
                    )
                    .await;
                continue;
            }
        };
        let result = handler.handle(cmd_payload).await;
        let response = match result {
            Ok(r) => r,
            Err(msg) => CommandResponse {
                success: false,
                message: Some(msg),
                data: None,
            },
        };
        let reply_bytes = serde_json::to_vec(&response).unwrap_or_default();
        let _ = query.reply(query.key_expr().clone(), reply_bytes).await;
    }
    Ok(())
}

/// Request a command via Zenoh get (query) and return the decoded response.
pub async fn request_command(
    session: &zenoh::Session,
    key_expr: &str,
    payload: CommandPayload,
) -> Result<CommandResponse, NetworkError> {
    let payload_bytes = payload::serialize_command(&payload)?;
    let replies = session
        .get(key_expr)
        .payload(payload_bytes.as_ref())
        .await
        .map_err(NetworkError::Zenoh)?;
    let reply = replies
        .recv_async()
        .await
        .map_err(|_| NetworkError::DeserializationFailed("no reply".to_string()))?;
    let sample = reply
        .result()
        .map_err(|e| NetworkError::DeserializationFailed(format!("reply error: {}", e)))?;
    let bytes = sample.payload().to_bytes().into_owned();
    let response: CommandResponse = serde_json::from_slice(&bytes)
        .map_err(|e| NetworkError::DeserializationFailed(e.to_string()))?;
    Ok(response)
}
