use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use smol_str::SmolStr;
use tokio::sync::broadcast;
use tracing::{info, info_span, Instrument};
use trading_core::config::ChannelConfig;
use trading_core::venue::dex::channels::DexVenueUpdate;
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::DexError;

use crate::chains::sui::DeepBookVenueProvider;
use crate::config::{DexChain, DexEngineConfig};
use crate::runtime::DexEngineRuntime;
use crate::state::manager::DexStateManager;
use crate::state::reconciler::DexReconciler;
use crate::DeepBookPoolMeta;
use trading_core::venue::VenueId;

/// Bootstrap DEX engine: runtime with state manager, provider, and optional reconciliation loop.
pub async fn run(
    config: DexEngineConfig,
    channels: ChannelConfig,
    mut shutdown_rx: broadcast::Receiver<()>,
    reconciliation_interval_secs: u64,
    reconciliation_enabled: bool,
) -> Result<Arc<DexEngineRuntime>, DexError> {
    let _bootstrap = info_span!("dex.bootstrap").entered();
    let (update_tx, update_rx) = trading_core::venue::dex::channels::dex_venue_update_channel(
        channels.dex_venue_update_capacity,
    );
    let state_manager = Arc::new(DexStateManager::new(update_tx.clone()));

    let (provider, pool_meta) = select_provider(&config).await?;
    info!("DEX engine starting for venue {:?}", config.venue);

    let snapshot = provider.bootstrap_state().await?;
    state_manager.apply_account_snapshot(snapshot);

    let wallet_state = state_manager.clone();
    let mut wallet_rx = update_tx.subscribe();
    let mut wallet_shutdown = shutdown_rx.resubscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                update = wallet_rx.recv() => {
                    match update {
                        Ok(DexVenueUpdate::WalletBalance { balances, .. }) => {
                            wallet_state.set_wallet_balances(balances);
                        }
                        Ok(DexVenueUpdate::OpenOrders { orders, .. }) => {
                            wallet_state.set_open_orders(orders);
                        }
                        Ok(DexVenueUpdate::PendingTransaction { pending, .. }) => {
                            let state = wallet_state.state();
                            let mut guard = state.write();
                            guard.pending.insert(pending.signature.clone(), pending);
                        }
                        Ok(DexVenueUpdate::ExecutionResult { result, .. }) => {
                            if result.status != trading_core::venue::dex::types::TxStatus::Pending {
                                let state = wallet_state.state();
                                let mut guard = state.write();
                                guard.pending.remove(&result.signature);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = wallet_shutdown.recv() => break,
            }
        }
    });

    let provider_clone = provider.clone();
    let tx_clone = update_tx.clone();
    let mut stream_shutdown = shutdown_rx.resubscribe();
    tokio::spawn(
        async move {
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
        }
        .instrument(info_span!("dex.stream_updates")),
    );

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
        pool_meta,
    });

    tokio::spawn(async move {
        let _ = shutdown_rx.recv().await;
        info!("DEX engine shutdown signal received");
    });

    Ok(runtime)
}

async fn select_provider(
    config: &DexEngineConfig,
) -> Result<
    (
        Arc<dyn VenueProvider>,
        Option<Arc<HashMap<SmolStr, DeepBookPoolMeta>>>,
    ),
    DexError,
> {
    match (config.venue, config.chain) {
        (VenueId::DeepBook, DexChain::Sui) => {
            let deepbook_config = config.deepbook_config()?;
            let provider = DeepBookVenueProvider::new(deepbook_config).await?;
            let pool_meta = Arc::new(provider.pool_meta());
            Ok((Arc::new(provider), Some(pool_meta)))
        }
        (venue, chain) => Err(DexError::ProviderNotConfigured(
            format!("unsupported dex_engine venue {venue:?} + chain {chain:?}").into(),
        )),
    }
}
