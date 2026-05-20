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
        let balances = provider.reconcile().await?;
        state.apply_wallet_balances(balances);
        info!("DEX reconciliation completed");
        Ok(())
    }
}
