//! Cold event bus: broadcast channel for state updates (multiple subscribers).

use crate::events::ColdEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Cold event bus (broadcast: multiple subscribers).
#[derive(Debug, Clone)]
pub struct ColdEventBus {
    tx: broadcast::Sender<Arc<ColdEvent>>,
}

impl ColdEventBus {
    /// Publish an event to all subscribers. Returns error if there are no receivers.
    pub fn publish(&self, event: Arc<ColdEvent>) -> Result<(), ColdEventBusError> {
        self.tx
            .send(event)
            .map_err(|_| ColdEventBusError::NoReceivers)
            .map(|_| ())
    }

    /// Create a new subscription (receiver).
    pub fn subscribe(&self) -> ColdEventRx {
        ColdEventRx {
            rx: self.tx.subscribe(),
        }
    }
}

/// Receiver for cold events (one of many subscribers).
#[derive(Debug)]
pub struct ColdEventRx {
    rx: broadcast::Receiver<Arc<ColdEvent>>,
}

impl ColdEventRx {
    /// Receive the next event (lagged if slow).
    pub async fn recv(&mut self) -> Result<Arc<ColdEvent>, ColdEventBusError> {
        self.rx.recv().await.map_err(|e| match e {
            broadcast::error::RecvError::Lagged(n) => ColdEventBusError::Lagged(n),
            broadcast::error::RecvError::Closed => ColdEventBusError::Closed,
        })
    }
}

/// Error from cold event bus.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ColdEventBusError {
    #[error("no receivers")]
    NoReceivers,
    #[error("receiver lagged (dropped {0} messages)")]
    Lagged(u64),
    #[error("channel closed")]
    Closed,
}

/// Create a cold event bus (broadcast). Returns the bus; call `subscribe()` to get receivers.
pub fn cold_event_bus(capacity: usize) -> ColdEventBus {
    let (tx, _) = broadcast::channel(capacity);
    ColdEventBus { tx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ColdEvent;
    use rust_decimal_macros::dec;
    use trading_core::types::common::{InstrumentType, OrderId, OrderStatus};
    use trading_core::types::exchange::Order;

    #[tokio::test]
    async fn test_cold_event_bus_publish_subscribe() {
        let bus = cold_event_bus(4);
        let mut rx = bus.subscribe();
        let order = Order {
            order_id: OrderId::new("o1".into()),
            client_order_id: None,
            instrument: trading_core::types::common::Instrument::new(
                InstrumentType::Spot,
                trading_core::types::common::Pair::new("BTC", "USDT"),
            ),
            side: trading_core::types::common::Side::Buy,
            price: dec!(50000),
            amount: dec!(1),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: dec!(0),
        };
        let event = Arc::new(ColdEvent::Order {
            order: order.clone(),
            trace_context: None,
        });
        bus.publish(event).unwrap();
        let received = rx.recv().await.unwrap();
        match received.as_ref() {
            ColdEvent::Order { order: o, .. } => assert_eq!(o.order_id.as_str(), "o1"),
            _ => panic!("expected Order"),
        }
    }

    #[tokio::test]
    async fn test_cold_event_bus_multiple_subscribers() {
        let bus = cold_event_bus(4);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let order = Order {
            order_id: OrderId::new("o2".into()),
            client_order_id: None,
            instrument: trading_core::types::common::Instrument::new(
                InstrumentType::Spot,
                trading_core::types::common::Pair::new("ETH", "USDT"),
            ),
            side: trading_core::types::common::Side::Sell,
            price: dec!(3000),
            amount: dec!(10),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: dec!(0),
        };
        let event = Arc::new(ColdEvent::Order {
            order,
            trace_context: None,
        });
        bus.publish(event).unwrap();
        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();
        assert!(Arc::ptr_eq(&r1, &r2));
    }
}
