//! Subscribes to in-process `trading_core` channels and fans out to `trading_networking` buses.

use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::warn;
use trading_core::channels::market_data::MarketDataUpdateReceiver;
use trading_core::metrics::{
    record_broadcast_lag_cex, record_broadcast_lag_dex, record_broadcast_lag_md,
};
use trading_core::venue::cex::channels::CexVenueUpdateReceiver;
use trading_core::venue::dex::channels::DexVenueUpdateReceiver;
use trading_networking::events::{
    cold_event_bus, cold_event_from_cex, dex_cold_event_bus, dex_cold_event_from_dex,
    hot_event_channel, hot_event_from_market_data, ColdEventBus, DexColdEventBus, HotEventRx,
    HotEventTx,
};

use crate::platform::shutdown::ShutdownManager;

// Re-export for existing scrape/import paths.
pub use trading_core::metrics::{BROADCAST_LAG_CEX, BROADCAST_LAG_DEX, BROADCAST_LAG_MD};

pub struct EventBridgeHandles {
    pub hot_tx: HotEventTx,
    pub hot_rx: HotEventRx,
    pub cold_bus: ColdEventBus,
    pub dex_bus: DexColdEventBus,
    _tasks: Vec<JoinHandle<()>>,
}

pub struct EventBridgeInputs<'a> {
    pub market_data_rx: MarketDataUpdateReceiver,
    pub cex_update_rx: Option<CexVenueUpdateReceiver>,
    pub dex_update_rx: Option<DexVenueUpdateReceiver>,
    pub shutdown_manager: &'a ShutdownManager,
    pub md_capacity: usize,
    pub cex_capacity: usize,
    pub dex_capacity: usize,
}

/// Start internal→external event bridge tasks. Returns buses consumed by Zenoh gateway when enabled.
pub fn start_event_bridge(inputs: EventBridgeInputs<'_>) -> EventBridgeHandles {
    let (hot_tx, hot_rx) = hot_event_channel(inputs.md_capacity);
    let cold_bus = cold_event_bus(inputs.cex_capacity);
    let dex_bus = dex_cold_event_bus(inputs.dex_capacity);
    let mut tasks = Vec::new();

    {
        let hot_tx = hot_tx.clone();
        let mut md_rx = inputs.market_data_rx;
        let mut shutdown_rx = inputs.shutdown_manager.subscribe();
        tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    update = md_rx.recv() => {
                        match update {
                            Ok(update) => {
                                if let Some(event) = hot_event_from_market_data(update) {
                                    let _ = hot_tx.try_send(event);
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                record_broadcast_lag_md(skipped);
                                warn!("[event_bridge] market data lagged; skipped {skipped}");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }
        }));
    }

    if let Some(mut cex_rx) = inputs.cex_update_rx {
        let cold_bus = cold_bus.clone();
        let mut shutdown_rx = inputs.shutdown_manager.subscribe();
        tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    update = cex_rx.recv() => {
                        match update {
                            Ok(update) => {
                                let event = cold_event_from_cex(update);
                                let _ = cold_bus.publish(Arc::new(event));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                record_broadcast_lag_cex(skipped);
                                warn!("[event_bridge] CEX updates lagged; skipped {skipped}");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }
        }));
    }

    if let Some(mut dex_rx) = inputs.dex_update_rx {
        let dex_bus = dex_bus.clone();
        let mut shutdown_rx = inputs.shutdown_manager.subscribe();
        tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    update = dex_rx.recv() => {
                        match update {
                            Ok(update) => {
                                let event = dex_cold_event_from_dex(update);
                                let _ = dex_bus.publish(Arc::new(event));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                record_broadcast_lag_dex(skipped);
                                warn!("[event_bridge] DEX updates lagged; skipped {skipped}");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }
        }));
    }

    EventBridgeHandles {
        hot_tx,
        hot_rx,
        cold_bus,
        dex_bus,
        _tasks: tasks,
    }
}
