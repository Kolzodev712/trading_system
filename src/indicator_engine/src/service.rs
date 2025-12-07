use crate::calculator::IndicatorCalculator;
use crate::indicators::traits::IndicatorConfig;
use crate::publisher::IndicatorPublisher;
use crate::subscriber::MarketDataSubscriber;
use std::sync::Arc;
use thiserror::Error;
use trading_core::channels::indicators::IndicatorUpdateSender;
use trading_core::channels::market_data::MarketDataUpdateReceiver;

pub struct IndicatorService {
    calculator: Arc<IndicatorCalculator>,
    subscriber: Option<MarketDataSubscriber>,
    publisher: Option<IndicatorPublisher>,
}

impl IndicatorService {
    pub fn new() -> Self {
        Self {
            calculator: Arc::new(IndicatorCalculator::new()),
            subscriber: None,
            publisher: None,
        }
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
    pub fn register_indicator(
        &self,
        config: IndicatorConfig,
    ) -> Result<(), crate::indicators::traits::IndicatorError> {
        self.calculator.register_indicator(config)
    }

    /// Get indicator value
    pub fn get_value(&self, indicator_id: &str) -> Option<crate::data::IndicatorValue> {
        self.calculator.get_value(indicator_id)
    }

    /// Get all values for a pair
    pub fn get_values_for_pair(&self, pair: &str) -> Vec<crate::data::IndicatorValue> {
        self.calculator.get_values_for_pair(pair)
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
}
