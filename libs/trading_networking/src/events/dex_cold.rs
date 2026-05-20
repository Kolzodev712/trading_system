//! DEX cold-lane event types (wallet, pending tx, execution results).

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use trading_core::venue::dex::types::{DexWalletBalances, ExecutionResult, TxStatus};
use trading_trace::ColdTraceContext;

/// DEX cold lane event (aligned with `DexVenueUpdate`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DexColdEvent {
    WalletBalance {
        balances: DexWalletBalances,
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
    PendingTransaction {
        signature: SmolStr,
        status: TxStatus,
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
    ExecutionResult {
        result: ExecutionResult,
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
}
