use std::sync::Arc;

use tracing::instrument;
use trading_core::venue::dex::channels::DexVenueUpdateReceiver;
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::types::ExecutionResult;
use trading_core::venue::dex::{DexError, TradeCommand};

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
}

impl DexEngineRuntime {
    #[instrument(name = "dex.execute", skip(self))]
    pub async fn execute(&self, command: TradeCommand) -> Result<ExecutionResult, DexError> {
        DEX_EXECUTE_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let state = self.state_manager.state();
            let mut guard = state.write();
            guard.last_command = Some(command.clone());
        }
        let result = match self.provider.execute(command).await {
            Ok(r) => r,
            Err(e) => {
                DEX_EXECUTE_ERRORS_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Err(e);
            }
        };
        self.state_manager
            .publish_execution_result(result.clone())?;
        Ok(result)
    }
}
