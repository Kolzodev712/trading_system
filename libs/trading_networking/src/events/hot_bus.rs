//! Hot event bus: bounded MPSC channel for ultra-low latency market data.

use crate::events::HotEvent;
use tokio::sync::mpsc;

/// Sender for hot events (non-blocking send).
#[derive(Debug, Clone)]
pub struct HotEventTx {
    tx: mpsc::Sender<HotEvent>,
}

impl HotEventTx {
    /// Try to send an event. Returns error if channel is full or closed.
    pub fn try_send(&self, event: HotEvent) -> Result<(), HotEventBusError> {
        self.tx.try_send(event).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => HotEventBusError::Full,
            mpsc::error::TrySendError::Closed(_) => HotEventBusError::Closed,
        })
    }
    /// Send an event, waiting if the channel is full.
    pub async fn send(&self, event: HotEvent) -> Result<(), HotEventBusError> {
        self.tx
            .send(event)
            .await
            .map_err(|_| HotEventBusError::Closed)
    }
}

/// Receiver for hot events (single consumer).
#[derive(Debug)]
pub struct HotEventRx {
    rx: mpsc::Receiver<HotEvent>,
}

impl HotEventRx {
    /// Receive the next event.
    pub async fn recv(&mut self) -> Option<HotEvent> {
        self.rx.recv().await
    }
}

/// Error from hot event bus.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HotEventBusError {
    #[error("channel full")]
    Full,
    #[error("channel closed")]
    Closed,
}

/// Create a bounded hot event channel (MPSC).
/// Returns (sender, receiver). Single consumer; use for hot path market data.
pub fn hot_event_channel(capacity: usize) -> (HotEventTx, HotEventRx) {
    let (tx, rx) = mpsc::channel(capacity);
    (HotEventTx { tx }, HotEventRx { rx })
}

/// Trait for sending hot events (e.g. for gateway abstraction later).
pub trait HotEventBus {
    /// Try to send without blocking.
    fn try_send(&self, event: HotEvent) -> Result<(), HotEventBusError>;
}

impl HotEventBus for HotEventTx {
    fn try_send(&self, event: HotEvent) -> Result<(), HotEventBusError> {
        self.tx.try_send(event).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => HotEventBusError::Full,
            mpsc::error::TrySendError::Closed(_) => HotEventBusError::Closed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::HotEvent;
    use rust_decimal_macros::dec;
    use trading_core::types::common::Pair;

    #[tokio::test]
    async fn test_hot_event_channel_send_recv() {
        let (tx, mut rx) = hot_event_channel(4);
        let event = HotEvent::Orderbook {
            pair: Pair::new("BTC", "USDT"),
            best_bid: dec!(50000),
            best_ask: dec!(50100),
            bid_qty: dec!(1),
            ask_qty: dec!(1),
            timestamp: 12345,
            full_snapshot: None,
            trace_context: None,
        };
        tx.try_send(event.clone()).unwrap();
        let received = rx.recv().await.unwrap();
        match (&received, &event) {
            (HotEvent::Orderbook { pair: p1, .. }, HotEvent::Orderbook { pair: p2, .. }) => {
                assert_eq!(p1, p2);
            }
            _ => panic!("variant mismatch"),
        }
    }

    #[tokio::test]
    async fn test_hot_event_channel_full() {
        let (tx, mut rx) = hot_event_channel(2);
        let e = HotEvent::Orderbook {
            pair: Pair::new("BTC", "USDT"),
            best_bid: dec!(0),
            best_ask: dec!(0),
            bid_qty: dec!(0),
            ask_qty: dec!(0),
            timestamp: 0,
            full_snapshot: None,
            trace_context: None,
        };
        tx.try_send(e.clone()).unwrap();
        tx.try_send(e.clone()).unwrap();
        let r = tx.try_send(e);
        assert!(matches!(r, Err(HotEventBusError::Full)));
        let _ = rx.recv().await;
        let _ = rx.recv().await;
    }
}
