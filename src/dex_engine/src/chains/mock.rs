use async_trait::async_trait;
use tokio::sync::broadcast;
use trading_core::venue::dex::channels::DexVenueUpdate;
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::types::{DexWalletBalances, ExecutionResult};
use trading_core::venue::dex::{DexError, TradeCommand};
use trading_core::venue::VenueId;

/// Compile-time stub until `chains/solana` (Proposal 02).
pub struct MockVenueProvider;

#[async_trait]
impl VenueProvider for MockVenueProvider {
    fn venue_id(&self) -> VenueId {
        VenueId::Solana
    }

    async fn bootstrap_state(&self) -> Result<DexWalletBalances, DexError> {
        Ok(DexWalletBalances::default())
    }

    async fn stream_updates(&self, _tx: broadcast::Sender<DexVenueUpdate>) -> Result<(), DexError> {
        Ok(())
    }

    async fn execute(&self, _command: TradeCommand) -> Result<ExecutionResult, DexError> {
        Err(DexError::UnsupportedCommand(
            "mock provider does not execute".into(),
        ))
    }

    async fn reconcile(&self) -> Result<DexWalletBalances, DexError> {
        Ok(DexWalletBalances::default())
    }
}
