//! Chain-agnostic DEX venue contract.

use async_trait::async_trait;
use tokio::sync::broadcast;

use super::channels::DexVenueUpdate;
use super::command::TradeCommand;
use super::error::DexError;
use super::types::{DexWalletBalances, ExecutionResult};
use crate::venue::VenueId;

/// Implemented per chain under `dex_engine::chains/<chain>/`.
#[async_trait]
pub trait VenueProvider: Send + Sync + 'static {
    fn venue_id(&self) -> VenueId;

    /// Initial on-chain snapshot into `DexState`.
    async fn bootstrap_state(&self) -> Result<DexWalletBalances, DexError>;

    /// Subscribe to venue updates (balances, pending txs, execution results).
    async fn stream_updates(&self, tx: broadcast::Sender<DexVenueUpdate>) -> Result<(), DexError>;

    async fn execute(&self, command: TradeCommand) -> Result<ExecutionResult, DexError>;

    async fn reconcile(&self) -> Result<DexWalletBalances, DexError>;
}
