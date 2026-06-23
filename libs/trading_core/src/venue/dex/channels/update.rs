//! Broadcast channel types for DEX venue updates (mirrors CEX `CexVenueUpdate` shape).

use tokio::sync::broadcast;
use trading_trace::ColdTraceContext;

use crate::venue::dex::types::{
    DexOpenOrder, DexWalletBalances, ExecutionResult, PendingTransaction,
};

/// DEX venue update message (individual updates, not batches).
#[derive(Debug, Clone)]
pub enum DexVenueUpdate {
    WalletBalance {
        balances: DexWalletBalances,
        trace_context: Option<ColdTraceContext>,
    },
    OpenOrders {
        orders: Vec<DexOpenOrder>,
        trace_context: Option<ColdTraceContext>,
    },
    PendingTransaction {
        pending: PendingTransaction,
        trace_context: Option<ColdTraceContext>,
    },
    ExecutionResult {
        result: ExecutionResult,
        trace_context: Option<ColdTraceContext>,
    },
}

pub type DexVenueUpdateSender = broadcast::Sender<DexVenueUpdate>;
pub type DexVenueUpdateReceiver = broadcast::Receiver<DexVenueUpdate>;

pub fn dex_venue_update_channel(capacity: usize) -> (DexVenueUpdateSender, DexVenueUpdateReceiver) {
    broadcast::channel(capacity)
}
