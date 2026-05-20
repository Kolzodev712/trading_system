//! DEX cold event bus (broadcast, multiple subscribers).

use crate::events::DexColdEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct DexColdEventBus {
    tx: broadcast::Sender<Arc<DexColdEvent>>,
}

impl DexColdEventBus {
    pub fn publish(&self, event: Arc<DexColdEvent>) -> Result<(), DexColdEventBusError> {
        self.tx
            .send(event)
            .map_err(|_| DexColdEventBusError::NoReceivers)
            .map(|_| ())
    }

    pub fn subscribe(&self) -> DexColdEventRx {
        DexColdEventRx {
            rx: self.tx.subscribe(),
        }
    }
}

#[derive(Debug)]
pub struct DexColdEventRx {
    rx: broadcast::Receiver<Arc<DexColdEvent>>,
}

impl DexColdEventRx {
    pub async fn recv(&mut self) -> Result<Arc<DexColdEvent>, DexColdEventBusError> {
        self.rx.recv().await.map_err(|e| match e {
            broadcast::error::RecvError::Lagged(n) => DexColdEventBusError::Lagged(n),
            broadcast::error::RecvError::Closed => DexColdEventBusError::Closed,
        })
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum DexColdEventBusError {
    #[error("no receivers")]
    NoReceivers,
    #[error("receiver lagged (dropped {0} messages)")]
    Lagged(u64),
    #[error("channel closed")]
    Closed,
}

pub fn dex_cold_event_bus(capacity: usize) -> DexColdEventBus {
    let (tx, _) = broadcast::channel(capacity);
    DexColdEventBus { tx }
}
