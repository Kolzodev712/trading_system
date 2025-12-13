use crate::calculator::IndicatorCalculator;
use crate::data::IndicatorValue;
use smol_str::SmolStr;
use std::sync::Arc;
use thiserror::Error;
use trading_core::channels::indicators::{IndicatorUpdate, IndicatorUpdateSender};

pub struct IndicatorPublisher {
    calculator: Arc<IndicatorCalculator>,
    sender: IndicatorUpdateSender,
    publish_interval: tokio::time::Duration,
}

impl IndicatorPublisher {
    pub fn new(
        calculator: Arc<IndicatorCalculator>,
        sender: IndicatorUpdateSender,
        publish_interval_secs: u64,
    ) -> Self {
        Self {
            calculator,
            sender,
            publish_interval: tokio::time::Duration::from_secs(publish_interval_secs),
        }
    }

    /// Start publishing indicator values periodically
    pub async fn run(&self) -> Result<(), PublisherError> {
        let mut interval = tokio::time::interval(self.publish_interval);

        loop {
            interval.tick().await;

            // Get all cached values
            let cache = self.calculator.cache();
            let cache_guard = cache.read();
            let values: Vec<IndicatorValue> = cache_guard.values().cloned().collect();
            drop(cache_guard);

            // Publish each value
            for value in values {
                let update = IndicatorUpdate {
                    indicator_id: value.indicator_id.clone(),
                    indicator_name: value.indicator_name.clone(),
                    pair: value.pair.clone(),
                    value: value.value,
                    timestamp: value.timestamp,
                    period: value.period,
                    is_composite: value.is_composite,
                };

                if let Err(e) = self.sender.send(update) {
                    return Err(PublisherError::ChannelError(SmolStr::new(format!(
                        "Failed to send: {}",
                        e
                    ))));
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("Channel error: {0}")]
    ChannelError(SmolStr),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculator::IndicatorCalculator;
    use crate::indicators::traits::{IndicatorConfig, PriceSource};
    use smol_str::SmolStr;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;
    use trading_core::channels::indicators::indicator_update_channel;

    #[tokio::test]
    async fn test_publisher_new() {
        let calculator = Arc::new(IndicatorCalculator::new());
        let (tx, _rx) = indicator_update_channel(10);
        let _publisher = IndicatorPublisher::new(calculator, tx, 1);
        // Should not panic
        // Test passes if we get here
    }

    #[tokio::test]
    async fn test_publisher_publishes_values() {
        let calculator = Arc::new(IndicatorCalculator::new());
        let (tx, mut rx) = indicator_update_channel(10);
        let publisher = IndicatorPublisher::new(calculator.clone(), tx, 1);

        // Register an indicator and update it to have a value
        let config = IndicatorConfig {
            id: SmolStr::new("test_sma"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 2, // Small period for quick testing
            source: PriceSource::Close,
        };
        calculator.register_indicator(config).unwrap();

        // Update with enough data points to make indicator ready
        use crate::data::PricePoint;
        use rust_decimal::Decimal;
        for i in 0..3 {
            let price_point = PricePoint {
                open: Decimal::new(50000 + i, 0),
                high: Decimal::new(50100 + i, 0),
                low: Decimal::new(49900 + i, 0),
                close: Decimal::new(50000 + i, 0),
                volume: Decimal::new(1, 0),
                timestamp: 1000 + i as u64,
            };
            calculator.update("BTCUSDT", price_point).unwrap();
        }

        // Start publisher in background
        let publisher_handle = tokio::spawn(async move {
            let _ = publisher.run().await;
        });

        // Wait for a publish (should happen within 1 second + buffer)
        let result = timeout(Duration::from_secs(2), rx.recv()).await;

        // Stop publisher
        publisher_handle.abort();

        // Should have received at least one update
        assert!(result.is_ok());
        if let Ok(Ok(update)) = result {
            assert_eq!(update.indicator_id, SmolStr::new("test_sma"));
        }
    }
}
