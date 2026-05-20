//! Broadcast channel types for DEX venue updates (mirrors CEX `CexVenueUpdate` shape).

use tokio::sync::broadcast;

use crate::venue::dex::types::{DexWalletBalances, ExecutionResult, PendingTransaction};

/// DEX venue update message (individual updates, not batches).
#[derive(Debug, Clone)]
pub enum DexVenueUpdate {
    WalletBalance { balances: DexWalletBalances },
    PendingTransaction { pending: PendingTransaction },
    ExecutionResult { result: ExecutionResult },
}

pub type DexVenueUpdateSender = broadcast::Sender<DexVenueUpdate>;
pub type DexVenueUpdateReceiver = broadcast::Receiver<DexVenueUpdate>;

pub fn dex_venue_update_channel(capacity: usize) -> (DexVenueUpdateSender, DexVenueUpdateReceiver) {
    broadcast::channel(capacity)
}
