//! DEX-specific errors (separate from CEX `EngineError`).

use smol_str::SmolStr;
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinError;

#[derive(Error, Debug)]
pub enum DexError {
    #[error("DEX channel send error: {0}")]
    ChannelSend(SmolStr),

    #[error("DEX channel receive error: {0}")]
    ChannelReceive(#[from] RecvError),

    #[error("DEX channel closed")]
    ChannelClosed,

    #[error("DEX connection closed")]
    ConnectionClosed,

    #[error("DEX task join error: {0}")]
    Join(#[from] JoinError),

    #[error("DEX RPC error: {0}")]
    Rpc(SmolStr),

    #[error("DEX transaction error: {0}")]
    Transaction(SmolStr),

    #[error("DEX reconciliation error: {0}")]
    Reconciliation(SmolStr),

    #[error("DEX provider not configured: {0}")]
    ProviderNotConfigured(SmolStr),

    #[error("DEX invalid config: {0}")]
    InvalidConfig(SmolStr),

    #[error("DEX unsupported command: {0}")]
    UnsupportedCommand(SmolStr),

    #[error("DEX custom error: {0}")]
    Custom(SmolStr),
}
