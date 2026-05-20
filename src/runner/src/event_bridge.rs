//! Subscribes to in-process `trading_core` channels and fans out to `trading_networking` buses.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::warn;
use trading_core::channels::market_data::MarketDataUpdateReceiver;
use trading_core::venue::cex::channels::CexVenueUpdateReceiver;
use trading_core::venue::dex::channels::DexVenueUpdateReceiver;
use trading_networking::events::{
    cold_event_bus, dex_cold_event_bus, hot_event_channel, ColdEvent, ColdEventBus, DexColdEvent,
    DexColdEventBus, HotEvent, HotEventRx, HotEventTx,
};

use crate::shutdown::ShutdownManager;

pub static BROADCAST_LAG_MD: AtomicU64 = AtomicU64::new(0);
pub static BROADCAST_LAG_CEX: AtomicU64 = AtomicU64::new(0);
pub static BROADCAST_LAG_DEX: AtomicU64 = AtomicU64::new(0);

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
                            Ok(trading_core::channels::market_data::MarketDataUpdate::Orderbook { orderbook, trace_context }) => {
                                let event = HotEvent::orderbook_from_structured(&orderbook, trace_context);
                                let _ = hot_tx.try_send(event);
                            }
                            Ok(trading_core::channels::market_data::MarketDataUpdate::Trade { trade, trace_context }) => {
                                let event = HotEvent::Trade {
                                    pair: trade.pair.clone(),
                                    price: trade.price,
                                    quantity: trade.quantity,
                                    timestamp: trade.timestamp,
                                    trade_id: trade.trade_id.clone(),
                                    trace_context,
                                };
                                let _ = hot_tx.try_send(event);
                            }
                            Ok(trading_core::channels::market_data::MarketDataUpdate::DexAccountHealth { .. }) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                BROADCAST_LAG_MD.fetch_add(skipped, Ordering::Relaxed);
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
                            Ok(trading_core::venue::cex::channels::CexVenueUpdate::Order { order, trace_context }) => {
                                let _ = cold_bus.publish(Arc::new(ColdEvent::Order { order, trace_context }));
                            }
                            Ok(trading_core::venue::cex::channels::CexVenueUpdate::Balance { balance, trace_context }) => {
                                let _ = cold_bus.publish(Arc::new(ColdEvent::Balance { balance, trace_context }));
                            }
                            Ok(trading_core::venue::cex::channels::CexVenueUpdate::Position { position, trace_context }) => {
                                let _ = cold_bus.publish(Arc::new(ColdEvent::Position { position, trace_context }));
                            }
                            Ok(trading_core::venue::cex::channels::CexVenueUpdate::Trade { trade, trace_context }) => {
                                let _ = cold_bus.publish(Arc::new(ColdEvent::Trade { trade, trace_context }));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                BROADCAST_LAG_CEX.fetch_add(skipped, Ordering::Relaxed);
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
                            Ok(trading_core::venue::dex::channels::DexVenueUpdate::WalletBalance { balances, trace_context }) => {
                                let _ = dex_bus.publish(Arc::new(DexColdEvent::WalletBalance { balances, trace_context }));
                            }
                            Ok(trading_core::venue::dex::channels::DexVenueUpdate::PendingTransaction { pending, trace_context }) => {
                                let _ = dex_bus.publish(Arc::new(DexColdEvent::PendingTransaction {
                                    signature: pending.signature.clone(),
                                    status: pending.status,
                                    trace_context,
                                }));
                            }
                            Ok(trading_core::venue::dex::channels::DexVenueUpdate::ExecutionResult { result, trace_context }) => {
                                let _ = dex_bus.publish(Arc::new(DexColdEvent::ExecutionResult { result, trace_context }));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                BROADCAST_LAG_DEX.fetch_add(skipped, Ordering::Relaxed);
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
