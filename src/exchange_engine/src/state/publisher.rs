//! Publisher for exchange state updates
//!
//! This module provides the `ExchangeStatePublisher` which publishes state updates
//! via broadcast channels for ultra-low latency fan-out to multiple consumers.

use tracing::{debug, warn};
use trading_core::{
    channels::exchange::ExchangeUpdateSender,
    error::EngineError,
    types::exchange::{Balance, Order, Trade},
};

/// Publisher for exchange state updates via broadcast channels
///
/// Publishes individual updates (not batches) for ultra-low latency (< 1μs).
/// Uses broadcast channels for efficient fan-out to multiple consumers.
#[derive(Debug, Clone)]
pub struct ExchangeStatePublisher {
    /// Broadcast channel sender for exchange updates
    exchange_update_tx: ExchangeUpdateSender,
}

impl ExchangeStatePublisher {
    /// Create a new publisher with a channel sender
    pub fn new(exchange_update_tx: ExchangeUpdateSender) -> Self {
        Self { exchange_update_tx }
    }

    /// Publish an order update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_order(&self, order: &Order) -> Result<(), EngineError> {
        use trading_core::channels::exchange::ExchangeUpdate;

        self.exchange_update_tx
            .send(ExchangeUpdate::Order(order.clone()))
            .map_err(|e| {
                warn!(
                    "Failed to publish order update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(anyhow::anyhow!("Failed to publish order: channel closed"))
            })?;

        debug!(
            "Published order update: order_id={}",
            order.order_id.as_str()
        );
        Ok(())
    }

    /// Publish a balance update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_balance(&self, balance: &Balance) -> Result<(), EngineError> {
        use trading_core::channels::exchange::ExchangeUpdate;

        self.exchange_update_tx
            .send(ExchangeUpdate::Balance(balance.clone()))
            .map_err(|e| {
                warn!(
                    "Failed to publish balance update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to publish balance: channel closed"
                ))
            })?;

        debug!("Published balance update: symbol={}", balance.symbol);
        Ok(())
    }

    /// Publish a trade update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_trade(&self, trade: &Trade) -> Result<(), EngineError> {
        use trading_core::channels::exchange::ExchangeUpdate;

        self.exchange_update_tx
            .send(ExchangeUpdate::Trade(trade.clone()))
            .map_err(|e| {
                warn!(
                    "Failed to publish trade update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(anyhow::anyhow!("Failed to publish trade: channel closed"))
            })?;

        debug!("Published trade update: trade_id={}", trade.trade_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::time::Duration;
    use tokio::time::timeout;
    use trading_core::{
        channels::exchange::exchange_update_channel,
        types::{
            common::{OrderId, OrderStatus, Side, Symbol},
            exchange::Order,
        },
    };

    fn create_test_order() -> Order {
        Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            symbol: Symbol::from("BTCUSDT"),
            side: Side::Buy,
            price: Decimal::new(100, 0),
            amount: Decimal::new(1, 0),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    #[tokio::test]
    async fn test_publish_order() {
        let (tx, mut rx) = exchange_update_channel(10);
        let publisher = ExchangeStatePublisher::new(tx);
        let order = create_test_order();

        publisher.publish_order(&order).await.unwrap();

        let update = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match update {
            trading_core::channels::exchange::ExchangeUpdate::Order(published_order) => {
                assert_eq!(published_order.order_id, order.order_id);
            }
            _ => panic!("Expected Order update"),
        }
    }
}
