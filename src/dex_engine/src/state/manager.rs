use std::sync::Arc;

use parking_lot::RwLock;
use trading_core::venue::dex::channels::{DexVenueUpdate, DexVenueUpdateSender};
use trading_core::venue::dex::types::DexWalletBalances;
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

    pub fn apply_wallet_balances(&self, wallet: DexWalletBalances) {
        {
            let mut guard = self.state.write();
            guard.wallet = wallet.clone();
        }
        let _ = self.publisher.publish(DexVenueUpdate::WalletBalance {
            balances: wallet,
            trace_context: None,
        });
    }

    pub fn publish(&self, update: DexVenueUpdate) -> Result<(), DexError> {
        self.publisher.publish(update)
    }

    pub fn publish_execution_result(
        &self,
        result: trading_core::venue::dex::types::ExecutionResult,
    ) -> Result<(), DexError> {
        self.publish(DexVenueUpdate::ExecutionResult {
            result,
            trace_context: None,
        })
    }
}
