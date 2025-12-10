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
