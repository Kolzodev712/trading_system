//! DEX monitoring snapshot helpers (shared by gRPC monitoring).

use std::collections::HashMap;

use smol_str::SmolStr;

use crate::state::manager::DexStateManager;

#[derive(Debug, Clone, Default)]
pub struct DexBalanceRow {
    pub mint: String,
    pub amount: String,
}

#[derive(Debug, Clone, Default)]
pub struct DexPendingRow {
    pub signature: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct DexVenueSnapshotData {
    pub balances: Vec<DexBalanceRow>,
    pub pending_transactions: Vec<DexPendingRow>,
    pub venue_label: String,
}

pub fn collect_dex_snapshot(
    dex_sm: &DexStateManager,
    venue_label: impl Into<String>,
) -> DexVenueSnapshotData {
    let state_arc = dex_sm.state();
    let state = state_arc.read();
    let balances: Vec<DexBalanceRow> = state
        .wallet
        .balances
        .iter()
        .map(|(mint, amount)| DexBalanceRow {
            mint: mint.to_string(),
            amount: amount.to_string(),
        })
        .collect();
    let pending_transactions: Vec<DexPendingRow> = state
        .pending
        .iter()
        .map(|(sig, pending)| DexPendingRow {
            signature: sig.to_string(),
            status: format!("{:?}", pending.status),
        })
        .collect();
    DexVenueSnapshotData {
        balances,
        pending_transactions,
        venue_label: venue_label.into(),
    }
}

/// Keyed map form for tests.
pub fn wallet_balance_map(dex_sm: &DexStateManager) -> HashMap<SmolStr, String> {
    let state_arc = dex_sm.state();
    let state = state_arc.read();
    state
        .wallet
        .balances
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect()
}
