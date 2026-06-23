use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use smol_str::SmolStr;
use tracing::instrument;
use trading_core::venue::dex::channels::{DexVenueUpdate, DexVenueUpdateReceiver};
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::types::{ExecutionResult, PendingTransaction, TxStatus};
use trading_core::venue::dex::{DexError, TradeCommand};

use crate::chains::sui::types::DeepBookPoolMeta;
use crate::state::manager::DexStateManager;

/// Counters for platform metrics (optional; set by runner).
pub static DEX_EXECUTE_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static DEX_EXECUTE_ERRORS_TOTAL: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Live DEX engine handles (state + provider + update stream).
pub struct DexEngineRuntime {
    pub state_manager: Arc<DexStateManager>,
    pub provider: Arc<dyn VenueProvider>,
    pub update_rx: DexVenueUpdateReceiver,
    /// Populated for DeepBook on Sui after bootstrap (avoids duplicate indexer fetches).
    pub pool_meta: Option<Arc<HashMap<SmolStr, DeepBookPoolMeta>>>,
}

impl DexEngineRuntime {
    pub fn pool_meta(&self) -> Option<Arc<HashMap<SmolStr, DeepBookPoolMeta>>> {
        self.pool_meta.clone()
    }
    #[instrument(name = "dex.execute", skip(self))]
    pub async fn execute(&self, command: TradeCommand) -> Result<ExecutionResult, DexError> {
        DEX_EXECUTE_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let state = self.state_manager.state();
            let mut guard = state.write();
            guard.last_command = Some(command.clone());
        }
        let result = match self.provider.execute(command.clone()).await {
            Ok(r) => r,
            Err(e) => {
                DEX_EXECUTE_ERRORS_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Err(e);
            }
        };
        if result.status == TxStatus::Pending {
            let pending = PendingTransaction {
                signature: result.signature.clone(),
                command,
                status: TxStatus::Pending,
                submitted_at: Instant::now(),
            };
            {
                let state = self.state_manager.state();
                let mut guard = state.write();
                guard
                    .pending
                    .insert(result.signature.clone(), pending.clone());
            }
            let _ = self
                .state_manager
                .publish(DexVenueUpdate::PendingTransaction {
                    pending,
                    trace_context: None,
                });
        }
        self.state_manager
            .publish_execution_result(result.clone())?;
        Ok(result)
    }
}
