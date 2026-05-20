use crate::calculator::IndicatorCalculator;
use crate::config::{load_indicators_file, ConfigError, IndicatorsFile};
use crate::indicators::composite::CompositeIndicatorConfig;
use crate::indicators::traits::IndicatorConfig;
use crate::level_pricing::{LevelPricingRegistry, LevelPricingRule};
use crate::persistence::IndicatorPersistence;
use crate::publisher::IndicatorPublisher;
use crate::subscriber::MarketDataSubscriber;
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use trading_core::channels::indicators::IndicatorUpdateSender;
use trading_core::channels::market_data::MarketDataUpdateReceiver;

pub struct IndicatorService {
    pub(crate) calculator: Arc<IndicatorCalculator>,
    level_pricing: Arc<RwLock<LevelPricingRegistry>>,
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
            level_pricing: Arc::new(RwLock::new(LevelPricingRegistry::new())),
            subscriber: None,
            publisher: None,
            persistence: None,
        }
    }

    /// Level template id → indicator pricing bindings (from indicators config / Mongo).
    pub fn level_pricing_registry(&self) -> Arc<RwLock<LevelPricingRegistry>> {
        self.level_pricing.clone()
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

    /// Register a composite indicator.
    pub async fn register_composite(
        &self,
        config: CompositeIndicatorConfig,
    ) -> Result<(), crate::indicators::traits::IndicatorError> {
        self.calculator.register_composite(config)
    }

    /// Load all indicators from MongoDB (simple, then composite).
    /// Returns the number of indicators successfully loaded.
    pub async fn load_from_mongo(&self) -> Result<usize, ServiceError> {
        let Some(persistence) = &self.persistence else {
            return Err(ServiceError::PersistenceError(
                "MongoDB persistence not enabled".to_string(),
            ));
        };

        let mut success_count = 0;

        match persistence.load_all_indicators().await {
            Ok(indicators) => {
                let total_count = indicators.len();
                for config in indicators {
                    if self.calculator.register_indicator(config).is_ok() {
                        success_count += 1;
                    }
                }
                tracing::info!(
                    "Loaded {} simple indicator(s) from MongoDB ({} registered)",
                    total_count,
                    success_count
                );
            }
            Err(e) => {
                tracing::warn!("Failed to load simple indicators from MongoDB: {}", e);
                return Err(ServiceError::PersistenceError(format!(
                    "Failed to load simple indicators: {}",
                    e
                )));
            }
        }

        let composite_before = success_count;
        match persistence.load_all_composites().await {
            Ok(composites) => {
                let total_count = composites.len();
                for config in composites {
                    if self.calculator.register_composite(config).is_ok() {
                        success_count += 1;
                    }
                }
                tracing::info!(
                    "Loaded {} composite indicator(s) from MongoDB ({} registered)",
                    total_count,
                    success_count - composite_before
                );
            }
            Err(e) => {
                tracing::warn!("Failed to load composite indicators from MongoDB: {}", e);
            }
        }

        match persistence.load_level_pricing().await {
            Ok(registry) => {
                *self.level_pricing.write() = registry;
                tracing::info!(
                    "Loaded {} level pricing binding(s) from MongoDB",
                    self.level_pricing.read().len()
                );
            }
            Err(e) => tracing::warn!("Failed to load level pricing from MongoDB: {}", e),
        }

        Ok(success_count)
    }

    /// Load indicators + level pricing from JSON file. Skips indicator IDs already registered (Mongo wins).
    pub fn load_from_config(&self, path: &Path) -> Result<usize, ServiceError> {
        let file = load_indicators_file(path).map_err(|e| match e {
            ConfigError::Io(err) if err.kind() == std::io::ErrorKind::NotFound => {
                ServiceError::ConfigNotFound(path.display().to_string())
            }
            other => ServiceError::ConfigError(other.to_string()),
        })?;

        Ok(self.register_from_file(&file, false))
    }

    /// Load only level pricing from file (when indicators come from MongoDB).
    pub fn load_level_pricing_from_config(&self, path: &Path) -> Result<usize, ServiceError> {
        let file = load_indicators_file(path).map_err(|e| match e {
            ConfigError::Io(err) if err.kind() == std::io::ErrorKind::NotFound => {
                ServiceError::ConfigNotFound(path.display().to_string())
            }
            other => ServiceError::ConfigError(other.to_string()),
        })?;
        Ok(self.apply_level_pricing(&file.level_pricing))
    }

    fn apply_level_pricing(&self, rules: &[LevelPricingRule]) -> usize {
        if rules.is_empty() {
            return 0;
        }
        match LevelPricingRegistry::from_rules(rules.to_vec()) {
            Ok(registry) => {
                let count = registry.len();
                *self.level_pricing.write() = registry;
                count
            }
            Err(e) => {
                tracing::warn!("Failed to apply level pricing rules: {}", e);
                0
            }
        }
    }

    fn register_from_file(&self, file: &IndicatorsFile, indicators_only: bool) -> usize {
        let mut registered = 0usize;

        for entry in file.indicators.clone() {
            if self.calculator.has_indicator(entry.id().as_str()) {
                tracing::debug!(
                    "Skipping indicator {} (already loaded from MongoDB)",
                    entry.id()
                );
                continue;
            }
            if let Some(config) = entry.into_simple_config() {
                if self.calculator.register_indicator(config).is_ok() {
                    registered += 1;
                }
            }
        }

        for entry in file.indicators.clone() {
            if self.calculator.has_indicator(entry.id().as_str()) {
                continue;
            }
            if let Some(config) = entry.into_composite_config() {
                match self.calculator.register_composite(config) {
                    Ok(()) => registered += 1,
                    Err(e) => tracing::warn!("Failed to register composite from config: {}", e),
                }
            }
        }

        if !indicators_only {
            self.apply_level_pricing(&file.level_pricing);
        }

        registered
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

    #[error("Config file not found: {0}")]
    ConfigNotFound(String),

    #[error("Config error: {0}")]
    ConfigError(String),
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
    async fn test_load_from_config_registers_indicators_and_level_pricing() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/indicators.json");
        if !path.exists() {
            return;
        }
        let service = IndicatorService::new();
        let registered = service.load_from_config(&path).expect("load config");
        assert!(registered >= 4, "expected at least 4 simple indicators");
        assert_eq!(service.level_pricing_registry().read().len(), 2);
        assert!(service
            .level_pricing_registry()
            .read()
            .get("buy_level_1")
            .is_some());
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
