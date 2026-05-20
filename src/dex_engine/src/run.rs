use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tracing::info;
use trading_core::config::ChannelConfig;
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::DexError;

use crate::chains::mock::MockVenueProvider;
use crate::config::DexEngineConfig;
use crate::runtime::DexEngineRuntime;
use crate::state::manager::DexStateManager;
use crate::state::reconciler::DexReconciler;

/// Bootstrap DEX engine: runtime with state manager, provider, and optional reconciliation loop.
pub async fn run(
    config: DexEngineConfig,
    channels: ChannelConfig,
    mut shutdown_rx: broadcast::Receiver<()>,
    reconciliation_interval_secs: u64,
    reconciliation_enabled: bool,
) -> Result<Arc<DexEngineRuntime>, DexError> {
    let (update_tx, update_rx) = trading_core::venue::dex::channels::dex_venue_update_channel(
        channels.dex_venue_update_capacity,
    );
    let state_manager = Arc::new(DexStateManager::new(update_tx.clone()));

    let provider: Arc<dyn VenueProvider> = Arc::new(select_provider(&config)?);
    info!("DEX engine starting for venue {:?}", config.venue);

    let wallet = provider.bootstrap_state().await?;
    state_manager.apply_wallet_balances(wallet);

    let provider_clone = provider.clone();
    let tx_clone = update_tx.clone();
    let mut stream_shutdown = shutdown_rx.resubscribe();
    tokio::spawn(async move {
        tokio::select! {
            result = provider_clone.stream_updates(tx_clone) => {
                if let Err(e) = result {
                    tracing::error!("DEX stream_updates ended: {e}");
                }
            }
            _ = stream_shutdown.recv() => {
                info!("DEX stream_updates task shutting down");
            }
        }
    });

    if reconciliation_enabled && reconciliation_interval_secs > 0 {
        let reconcile_provider = provider.clone();
        let reconcile_state = state_manager.clone();
        let mut reconcile_shutdown = shutdown_rx.resubscribe();
        let interval = Duration::from_secs(reconciliation_interval_secs);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = DexReconciler::reconcile_once(
                            reconcile_provider.as_ref() as &dyn VenueProvider,
                            &reconcile_state,
                        )
                        .await
                        {
                            tracing::warn!("DEX reconciliation failed: {e}");
                        }
                    }
                    _ = reconcile_shutdown.recv() => {
                        info!("DEX reconciliation loop shutting down");
                        break;
                    }
                }
            }
        });
    }

    let runtime = Arc::new(DexEngineRuntime {
        state_manager,
        provider,
        update_rx,
    });

    tokio::spawn(async move {
        let _ = shutdown_rx.recv().await;
        info!("DEX engine shutdown signal received");
    });

    Ok(runtime)
}

fn select_provider(_config: &DexEngineConfig) -> Result<MockVenueProvider, DexError> {
    Ok(MockVenueProvider)
}
