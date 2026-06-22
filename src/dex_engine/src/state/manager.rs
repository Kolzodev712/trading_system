use std::sync::Arc;

use parking_lot::RwLock;
use trading_core::venue::dex::channels::{DexVenueUpdate, DexVenueUpdateSender};
use trading_core::venue::dex::types::{
    DexAccountSnapshot, DexOpenOrder, DexWalletBalances, TxStatus,
};
use trading_core::venue::dex::DexError;

use super::dex_state::DexState;
use super::publisher::DexStatePublisher;

#[derive(Debug)]
pub struct DexStateManager {
    state: Arc<RwLock<DexState>>,
    publisher: DexStatePublisher,
}

impl DexStateManager {
    pub fn new(update_tx: DexVenueUpdateSender) -> Self {
        Self {
            state: Arc::new(RwLock::new(DexState::new())),
            publisher: DexStatePublisher::new(update_tx),
        }
    }

    pub fn state(&self) -> Arc<RwLock<DexState>> {
        self.state.clone()
    }

    pub fn apply_account_snapshot(&self, snapshot: DexAccountSnapshot) {
        self.set_wallet_balances(snapshot.wallet.clone());
        self.set_open_orders(snapshot.open_orders.clone());
        let _ = self.publisher.publish(DexVenueUpdate::WalletBalance {
            balances: snapshot.wallet,
            trace_context: None,
        });
        let _ = self.publisher.publish(DexVenueUpdate::OpenOrders {
            orders: snapshot.open_orders,
            trace_context: None,
        });
    }

    pub fn apply_wallet_balances(&self, wallet: DexWalletBalances) {
        self.set_wallet_balances(wallet.clone());
        let _ = self.publisher.publish(DexVenueUpdate::WalletBalance {
            balances: wallet,
            trace_context: None,
        });
    }

    /// Update in-memory wallet balances without publishing (used by update subscribers).
    pub fn set_wallet_balances(&self, wallet: DexWalletBalances) {
        let mut guard = self.state.write();
        guard.wallet = wallet;
    }

    pub fn set_open_orders(&self, orders: Vec<DexOpenOrder>) {
        let mut guard = self.state.write();
        guard.open_orders = orders;
    }

    pub fn publish(&self, update: DexVenueUpdate) -> Result<(), DexError> {
        self.publisher.publish(update)
    }

    pub fn publish_execution_result(
        &self,
        result: trading_core::venue::dex::types::ExecutionResult,
    ) -> Result<(), DexError> {
        if result.status != TxStatus::Pending {
            let mut guard = self.state.write();
            guard.pending.remove(&result.signature);
        }
        self.publish(DexVenueUpdate::ExecutionResult {
            result,
            trace_context: None,
        })
    }
}
