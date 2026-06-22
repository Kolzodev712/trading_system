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
pub struct DexOrderRow {
    pub order_id: String,
    pub pool: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct DexVenueSnapshotData {
    pub balances: Vec<DexBalanceRow>,
    pub pending_transactions: Vec<DexPendingRow>,
    pub open_orders: Vec<DexOrderRow>,
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
    let open_orders: Vec<DexOrderRow> = state
        .open_orders
        .iter()
        .map(|order| DexOrderRow {
            order_id: order.order_id.to_string(),
            pool: order.pool_name.to_string(),
            side: order.side.to_string(),
            price: order.price.to_string(),
            quantity: order.quantity.to_string(),
            status: order.status.to_string(),
        })
        .collect();
    DexVenueSnapshotData {
        balances,
        pending_transactions,
        open_orders,
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
