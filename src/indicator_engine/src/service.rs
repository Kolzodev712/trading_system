use crate::calculator::IndicatorCalculator;
use crate::indicators::traits::IndicatorConfig;
use crate::persistence::IndicatorPersistence;
use crate::publisher::IndicatorPublisher;
use crate::subscriber::MarketDataSubscriber;
use std::sync::Arc;
use thiserror::Error;
use trading_core::channels::indicators::IndicatorUpdateSender;
use trading_core::channels::market_data::MarketDataUpdateReceiver;

pub struct IndicatorService {
    pub(crate) calculator: Arc<IndicatorCalculator>,
    subscriber: Option<MarketDataSubscriber>,
    publisher: Option<IndicatorPublisher>,
    persistence: Option<Arc<IndicatorPersistence>>,
}

impl Default for IndicatorService {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorService {
    pub fn new() -> Self {
        Self {
            calculator: Arc::new(IndicatorCalculator::new()),
            subscriber: None,
            publisher: None,
            persistence: None,
        }
    }

    /// Set persistence for indicator service
    pub fn with_persistence(mut self, persistence: Arc<IndicatorPersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Initialize the service with channels
    pub fn initialize(
        &mut self,
        market_data_receiver: MarketDataUpdateReceiver,
        indicator_sender: IndicatorUpdateSender,
        publish_interval_secs: u64,
    ) {
        self.subscriber = Some(MarketDataSubscriber::new(
            market_data_receiver,
            self.calculator.clone(),
        ));

        self.publisher = Some(IndicatorPublisher::new(
            self.calculator.clone(),
            indicator_sender,
            publish_interval_secs,
        ));
    }

    /// Register an indicator
    pub async fn register_indicator(
        &self,
        config: IndicatorConfig,
    ) -> Result<(), crate::indicators::traits::IndicatorError> {
        // Clone config only once for persistence (if needed)
        let config_for_persistence = if self.persistence.is_some() {
            Some(config.clone())
        } else {
            None
        };

        self.calculator.register_indicator(config)?;

        // Persist to MongoDB if enabled
        if let Some(persistence) = &self.persistence {
            if let Some(config_to_save) = config_for_persistence {
                if let Err(e) = persistence.save_simple_indicator(&config_to_save).await {
                    tracing::warn!("Failed to persist indicator to MongoDB: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Load all indicators from MongoDB
    /// Returns the number of indicators successfully loaded
    pub async fn load_from_mongo(&self) -> Result<usize, ServiceError> {
        if let Some(persistence) = &self.persistence {
            match persistence.load_all_indicators().await {
                Ok(indicators) => {
                    let total_count = indicators.len();
                    let mut success_count = 0;
                    for config in indicators {
                        if let Err(e) = self.calculator.register_indicator(config) {
                            tracing::warn!("Failed to register indicator from MongoDB: {}", e);
                        } else {
                            success_count += 1;
                        }
                    }
                    tracing::info!(
                        "Loaded {} indicators from MongoDB ({} successful, {} failed)",
                        total_count,
                        success_count,
                        total_count - success_count
                    );
                    Ok(success_count)
                }
                Err(e) => {
                    tracing::warn!("Failed to load indicators from MongoDB: {}", e);
                    Err(ServiceError::PersistenceError(format!(
                        "Failed to load indicators: {}",
                        e
                    )))
                }
            }
        } else {
            Err(ServiceError::PersistenceError(
                "MongoDB persistence not enabled".to_string(),
            ))
        }
    }

    /// Get the number of registered indicators
    pub fn get_indicator_count(&self) -> usize {
        self.calculator.get_indicator_ids().len()
    }

    /// Get indicator value (latest)
    pub fn get_value(&self, indicator_id: &str) -> Option<crate::data::IndicatorValue> {
        self.calculator.get_value(indicator_id)
    }

    /// Get historical values for an indicator (most recent first)
    ///
    /// # Arguments
    /// * `indicator_id` - Indicator ID
    /// * `limit` - Maximum number of historical values to return (None = all)
    pub fn get_history(
        &self,
        indicator_id: &str,
        limit: Option<usize>,
    ) -> Vec<crate::data::IndicatorValue> {
        self.calculator.get_history(indicator_id, limit)
    }

    /// Get all values for a pair (latest values only)
    pub fn get_values_for_pair(&self, pair: &str) -> Vec<crate::data::IndicatorValue> {
        self.calculator.get_values_for_pair(pair)
    }

    /// Get all values for an indicator name (e.g., all "SMA" indicators)
    pub fn get_values_by_name(&self, indicator_name: &str) -> Vec<crate::data::IndicatorValue> {
        self.calculator.get_values_by_name(indicator_name)
    }

    /// Get indicator value by pair and name (e.g., "SMA" for "BTC/USDT")
    ///
    /// Returns the first matching indicator if multiple exist
    pub fn get_value_by_pair_and_name(
        &self,
        pair: &str,
        indicator_name: &str,
    ) -> Option<crate::data::IndicatorValue> {
        self.calculator
            .get_value_by_pair_and_name(pair, indicator_name)
    }

    /// Get all current indicator values (latest per indicator)
    pub fn get_all_values(&self) -> Vec<crate::data::IndicatorValue> {
        self.calculator.get_all_current_values()
    }

    /// Get a reference to the calculator (for direct access without locking the service)
    pub fn calculator(&self) -> Arc<crate::calculator::IndicatorCalculator> {
        self.calculator.clone()
    }

    /// Start the service (run subscriber and publisher)
    pub async fn run(&mut self) -> Result<(), ServiceError> {
        let mut subscriber = self.subscriber.take().ok_or(ServiceError::NotInitialized)?;
        let publisher = self.publisher.take().ok_or(ServiceError::NotInitialized)?;

        // Run subscriber and publisher concurrently
        tokio::select! {
            result = subscriber.run() => {
                result.map_err(ServiceError::SubscriberError)
            }
            result = publisher.run() => {
                result.map_err(ServiceError::PublisherError)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Service not initialized")]
    NotInitialized,

    #[error("Subscriber error: {0}")]
    SubscriberError(#[from] crate::subscriber::SubscriberError),

    #[error("Publisher error: {0}")]
    PublisherError(#[from] crate::publisher::PublisherError),

    #[error("Persistence error: {0}")]
    PersistenceError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::traits::{IndicatorConfig, PriceSource};
    use smol_str::SmolStr;
    use trading_core::channels::indicators::indicator_update_channel;
    use trading_core::channels::market_data::market_data_update_channel;

    #[tokio::test]
    async fn test_indicator_service_new() {
        let service = IndicatorService::new();
        assert_eq!(service.get_indicator_count(), 0);
    }

    #[tokio::test]
    async fn test_indicator_service_register_indicator() {
        let service = IndicatorService::new();
        let config = IndicatorConfig {
            id: SmolStr::new("test_sma"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 20,
            source: PriceSource::Close,
        };

        let result = service.register_indicator(config).await;
        assert!(result.is_ok());
        assert_eq!(service.get_indicator_count(), 1);
    }

    #[tokio::test]
    async fn test_indicator_service_get_value() {
        let service = IndicatorService::new();
        let config = IndicatorConfig {
            id: SmolStr::new("test_sma"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 20,
            source: PriceSource::Close,
        };

        service.register_indicator(config).await.unwrap();

        // Value should be None initially (indicator not ready)
        let value = service.get_value("test_sma");
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_indicator_service_initialize() {
        let mut service = IndicatorService::new();
        let (_, market_rx) = market_data_update_channel(10);
        let (indicator_tx, _indicator_rx) = indicator_update_channel(10);

        service.initialize(market_rx, indicator_tx, 1);
        // Should not panic
        // Test passes if we get here
    }
}
