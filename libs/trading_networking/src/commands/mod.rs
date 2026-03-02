//! Command types and router for request/response over the command bus.

mod dispatcher;
mod router;

pub use dispatcher::CommandDispatcher;
pub use router::{CommandRouter, CommandRouterError, MpscCommandRouter};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use trading_trace::ColdTraceContext;

/// Command with correlation ID and optional reply channel.
#[derive(Debug)]
pub struct Command {
    /// Correlation ID (0 = none).
    pub correlation_id: u64,
    /// Command payload.
    pub payload: CommandPayload,
    /// Reply channel (None for fire-and-forget).
    pub reply: Option<oneshot::Sender<Result<CommandResponse, String>>>,
    /// Trace context (optional).
    pub trace_context: Option<ColdTraceContext>,
}

/// Command payload variants (aligned with trading_cli RPCs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandPayload {
    /// Set trading enabled/disabled.
    SetTradingEnabled { enabled: bool },
    /// Get trading status.
    GetTradingStatus,
    /// Get all levels.
    GetLevels,
    /// Get one level by id.
    GetLevel { level_id: String },
    /// Create a level (config as minimal struct for wire format).
    CreateLevel { config: LevelConfigPayload },
    /// Update a level.
    UpdateLevel {
        level_id: String,
        config: LevelConfigPayload,
    },
    /// Delete a level.
    DeleteLevel { level_id: String },
    /// Toggle level enabled.
    ToggleLevel { level_id: String, enabled: bool },
    /// Toggle strategy enabled.
    ToggleStrategy {
        strategy_name: String,
        symbol: String,
        enabled: bool,
    },
}

/// Minimal level config for commands (string/decimal fields for wire format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelConfigPayload {
    pub id: String,
    pub symbol_base: String,
    pub symbol_quote: String,
    pub side: String,
    pub order_type: String,
    pub price: String,
    pub quantity: String,
    pub enabled: bool,
    pub risk_limit: String,
    pub max_position_size: String,
    pub repost_on_fill: bool,
    pub repost_on_cancel: bool,
}

/// Command response (success + optional data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub success: bool,
    pub message: Option<String>,
    /// Optional binary payload (e.g. level id, list of levels).
    #[serde(
        default,
        serialize_with = "serialize_option_bytes",
        deserialize_with = "deserialize_option_bytes"
    )]
    pub data: Option<bytes::Bytes>,
}

fn serialize_option_bytes<S>(v: &Option<bytes::Bytes>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match v {
        None => s.serialize_none(),
        Some(b) => serde_bytes::serialize(b.as_ref(), s),
    }
}

fn deserialize_option_bytes<'de, D>(d: D) -> Result<Option<bytes::Bytes>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Wrapper(#[serde(with = "serde_bytes")] Vec<u8>);
    let opt: Option<Wrapper> = Option::deserialize(d)?;
    Ok(opt.map(|w| bytes::Bytes::from(w.0)))
}
