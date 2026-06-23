//! Indicator subscriber - subscribes to indicator updates via channels
use indicator_engine::data::IndicatorValue;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::channels::indicators::IndicatorUpdateReceiver;

/// Subscribes to indicator updates and maintains a cache
pub struct IndicatorSubscriber {
    pub(crate) receiver: IndicatorUpdateReceiver,
    indicators: Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>,
}

impl IndicatorSubscriber {
    /// Create a new indicator subscriber
    pub fn new(receiver: IndicatorUpdateReceiver) -> Self {
        Self {
            receiver,
            indicators: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get access to the indicators cache
    pub fn cache(&self) -> Arc<RwLock<HashMap<SmolStr, IndicatorValue>>> {
        self.indicators.clone()
    }

    /// Set the indicators cache (for sharing cache between tasks)
    pub fn set_cache(&mut self, cache: Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>) {
        self.indicators = cache;
    }

    /// Start listening for indicator updates
    pub async fn run(&mut self) -> Result<(), SubscriberError> {
        loop {
            match self.receiver.recv().await {
                Ok(update) => {
                    let indicator_value = IndicatorValue {
                        indicator_id: update.indicator_id.clone(),
                        indicator_name: update.indicator_name.clone(),
                        pair: update.pair.clone(),
                        value: update.value,
                        timestamp: update.timestamp,
                        period: update.period,
                        is_composite: update.is_composite,
                    };

                    let mut indicators = self.indicators.write().await;
                    indicators.insert(update.indicator_id.clone(), indicator_value);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        "Indicator receiver lagged, skipped {skipped} messages. This may indicate slow processing."
                    );
                    // Continue receiving - broadcast channels automatically skip lagged messages
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::warn!("Indicator channel closed - shutting down subscriber");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Get indicator value by ID (from cache)
    pub async fn get_indicator(&self, indicator_id: &str) -> Option<IndicatorValue> {
        let indicators = self.indicators.read().await;
        indicators.get(&SmolStr::new(indicator_id)).cloned()
    }

    /// Get all indicators for a pair (from cache)
    pub async fn get_indicators_for_pair(&self, pair: &str) -> Vec<IndicatorValue> {
        let pair_smol = SmolStr::new(pair);
        let indicators = self.indicators.read().await;
        indicators
            .values()
            .filter(|v| v.pair == pair_smol)
            .cloned()
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubscriberError {
    #[error("Channel error: {0}")]
    ChannelError(SmolStr),
}
