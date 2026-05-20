use std::sync::Arc;

use trading_core::venue::dex::channels::DexVenueUpdateReceiver;
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::types::ExecutionResult;
use trading_core::venue::dex::{DexError, TradeCommand};

use crate::state::manager::DexStateManager;

/// Live DEX engine handles (state + provider + update stream).
pub struct DexEngineRuntime {
    pub state_manager: Arc<DexStateManager>,
    pub provider: Arc<dyn VenueProvider>,
    pub update_rx: DexVenueUpdateReceiver,
}

impl DexEngineRuntime {
    pub async fn execute(&self, command: TradeCommand) -> Result<ExecutionResult, DexError> {
        {
            let state = self.state_manager.state();
            let mut guard = state.write();
            guard.last_command = Some(command.clone());
        }
        let result = self.provider.execute(command).await?;
        self.state_manager
            .publish_execution_result(result.clone())?;
        Ok(result)
    }
}
