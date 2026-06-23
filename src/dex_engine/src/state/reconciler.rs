use tracing::info;
use trading_core::venue::dex::{provider::VenueProvider, DexError};

use super::manager::DexStateManager;

/// Periodic on-chain vs internal reconciliation (skeleton).
pub struct DexReconciler;

impl DexReconciler {
    pub async fn reconcile_once<P: VenueProvider + ?Sized>(
        provider: &P,
        state: &DexStateManager,
    ) -> Result<(), DexError> {
        let snapshot = provider.reconcile().await?;
        state.apply_account_snapshot(snapshot);
        info!("DEX reconciliation completed");
        Ok(())
    }
}
