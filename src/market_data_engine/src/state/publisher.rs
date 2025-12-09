//! Publisher for market data state updates
//!
//! This module provides the `MarketDataStatePublisher` which publishes state updates
//! via broadcast channels for ultra-low latency fan-out to multiple consumers.

use tracing::{debug, warn};
use trading_core::{
    channels::market_data::{MarketDataUpdate, MarketDataUpdateSender},
    error::EngineError,
    types::market_data::{LegacyOrderbook as Orderbook, LegacyTrade as Trade},
};

/// Publisher for market data state updates via broadcast channels
///
/// Publishes individual updates (not batches) for ultra-low latency (< 1μs).
/// Uses broadcast channels for efficient fan-out to multiple consumers.
#[derive(Debug, Clone)]
pub struct MarketDataStatePublisher {
    /// Broadcast channel sender for market data updates
    market_data_tx: MarketDataUpdateSender,
}

impl MarketDataStatePublisher {
    /// Create a new publisher with a channel sender
    pub fn new(market_data_tx: MarketDataUpdateSender) -> Self {
        Self { market_data_tx }
    }

    /// Publish an orderbook update via broadcast channel
    ///
    /// # Arguments
    /// * `pair` - Trading pair
    /// * `orderbook` - Orderbook data
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_orderbook(
        &self,
        pair: &trading_core::types::common::Pair,
        orderbook: &Orderbook,
    ) -> Result<(), EngineError> {
        self.market_data_tx
            .send(MarketDataUpdate::Orderbook {
                pair: pair.clone(),
                orderbook: orderbook.clone(),
            })
            .map_err(|e| {
                warn!(
                    "Failed to publish orderbook update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(anyhow::anyhow!(
                    "Failed to publish orderbook: channel closed"
                ))
            })?;

        debug!("Published orderbook update for pair: {}", pair);
        Ok(())
    }

    /// Publish a trade update via broadcast channel
    ///
    /// # Arguments
    /// * `pair` - Trading pair
    /// * `trade` - Trade data
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_trade(
        &self,
        pair: &trading_core::types::common::Pair,
        trade: &Trade,
    ) -> Result<(), EngineError> {
        self.market_data_tx
            .send(MarketDataUpdate::Trade {
                pair: pair.clone(),
                trade: trade.clone(),
            })
            .map_err(|e| {
                warn!(
                    "Failed to publish trade update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(anyhow::anyhow!("Failed to publish trade: channel closed"))
            })?;

        debug!("Published trade update for pair: {}", pair);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::time::Duration;
    use tokio::time::timeout;
    use trading_core::channels::market_data::market_data_update_channel;

    fn create_test_orderbook() -> Orderbook {
        Orderbook {
            bids: vec![Decimal::new(50000, 0), Decimal::new(1, 0)],
            asks: vec![Decimal::new(51000, 0), Decimal::new(1, 0)],
        }
    }

    fn create_test_trade() -> Trade {
        Trade {
            price: Decimal::new(123456, 2),
            quantity: Decimal::new(5, 1),
            time: 123456789,
            is_market_maker: true,
        }
    }

    #[tokio::test]
    async fn test_publish_orderbook() {
        let (tx, mut rx) = market_data_update_channel(10);
        let publisher = MarketDataStatePublisher::new(tx);
        let orderbook = create_test_orderbook();

        use trading_core::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        publisher
            .publish_orderbook(&pair, &orderbook)
            .await
            .unwrap();

        let update = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match update {
            MarketDataUpdate::Orderbook {
                orderbook: published_orderbook,
                ..
            } => {
                assert_eq!(published_orderbook, orderbook);
            }
            _ => panic!("Expected Orderbook update"),
        }
    }

    #[tokio::test]
    async fn test_publish_trade() {
        let (tx, mut rx) = market_data_update_channel(10);
        let publisher = MarketDataStatePublisher::new(tx);
        let trade = create_test_trade();

        use trading_core::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        publisher.publish_trade(&pair, &trade).await.unwrap();

        let update = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match update {
            MarketDataUpdate::Trade {
                trade: published_trade,
                ..
            } => {
                assert_eq!(published_trade, trade);
            }
            _ => panic!("Expected Trade update"),
        }
    }
}
