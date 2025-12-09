//! Channel types for market data updates
//!
//! Uses broadcast channels for fan-out to multiple consumers (Trading Engine, Indicator Engine, etc.)
use crate::types::{
    common::Pair,
    market_data::{LegacyOrderbook, LegacyTrade},
};
use tokio::sync::broadcast;

/// Market data update message (individual updates, not batches)
#[derive(Debug, Clone)]
pub enum MarketDataUpdate {
    /// Orderbook update with pair information
    Orderbook {
        /// Trading pair
        pair: Pair,
        /// Orderbook data
        orderbook: LegacyOrderbook,
    },
    /// Public trade update with pair information
    Trade {
        /// Trading pair
        pair: Pair,
        /// Trade data
        trade: LegacyTrade,
    },
}

/// Market data update channel sender (broadcast for fan-out)
pub type MarketDataUpdateSender = broadcast::Sender<MarketDataUpdate>;

/// Market data update channel receiver (broadcast for fan-out)
pub type MarketDataUpdateReceiver = broadcast::Receiver<MarketDataUpdate>;

/// Create a new market data update broadcast channel
///
/// # Arguments
/// * `capacity` - Channel capacity (number of messages to buffer). Use 1000 for high-frequency updates.
pub fn market_data_update_channel(
    capacity: usize,
) -> (MarketDataUpdateSender, MarketDataUpdateReceiver) {
    broadcast::channel(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[tokio::test]
    async fn test_market_data_channel_orderbook() {
        let (tx, mut rx1) = market_data_update_channel(100);

        // Create second receiver to test fan-out
        let mut rx2 = tx.subscribe();

        let orderbook = LegacyOrderbook {
            bids: vec![Decimal::new(50000, 0), Decimal::new(1, 0)],
            asks: vec![Decimal::new(51000, 0), Decimal::new(1, 0)],
        };

        // Send orderbook update
        use crate::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        tx.send(MarketDataUpdate::Orderbook {
            pair: pair.clone(),
            orderbook: orderbook.clone(),
        })
        .unwrap();

        // Both receivers should get the update
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        match (received1, received2) {
            (
                MarketDataUpdate::Orderbook { orderbook: ob1, .. },
                MarketDataUpdate::Orderbook { orderbook: ob2, .. },
            ) => {
                assert_eq!(ob1, orderbook);
                assert_eq!(ob2, orderbook);
            }
            _ => panic!("Expected Orderbook variant"),
        }
    }

    #[tokio::test]
    async fn test_market_data_channel_trade() {
        let (tx, mut rx) = market_data_update_channel(100);

        let trade = LegacyTrade {
            price: Decimal::new(50500, 0),
            quantity: Decimal::new(1, 0),
            time: 1234567890,
            is_market_maker: false,
        };

        use crate::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        tx.send(MarketDataUpdate::Trade {
            pair: pair.clone(),
            trade: trade.clone(),
        })
        .unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            MarketDataUpdate::Trade { trade: t, .. } => assert_eq!(t, trade),
            _ => panic!("Expected Trade variant"),
        }
    }

    #[tokio::test]
    async fn test_market_data_channel_fan_out() {
        let (tx, _) = market_data_update_channel(100);

        // Create multiple receivers to test fan-out
        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();
        let mut rx3 = tx.subscribe();

        let orderbook = LegacyOrderbook {
            bids: vec![Decimal::new(50000, 0)],
            asks: vec![Decimal::new(51000, 0)],
        };

        use crate::types::common::Pair;
        let pair = Pair::new("BTC", "USDT");
        tx.send(MarketDataUpdate::Orderbook {
            pair: pair.clone(),
            orderbook: orderbook.clone(),
        })
        .unwrap();

        // All three receivers should get the update
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();
        let received3 = rx3.recv().await.unwrap();

        assert!(matches!(received1, MarketDataUpdate::Orderbook { .. }));
        assert!(matches!(received2, MarketDataUpdate::Orderbook { .. }));
        assert!(matches!(received3, MarketDataUpdate::Orderbook { .. }));
    }
}
