use crate::data::{IndicatorValue, PricePoint};
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError};
use crate::indicators::{ExponentialMovingAverage, SimpleMovingAverage};
use parking_lot::RwLock;
use smol_str::SmolStr;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Maximum number of historical values to store per indicator
const MAX_HISTORY_SIZE: usize = 100;

/// Cache entry with current value and history
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Latest indicator value
    current: IndicatorValue,
    /// Historical values (most recent first, limited to MAX_HISTORY_SIZE)
    history: VecDeque<IndicatorValue>,
}

impl CacheEntry {
    fn new(value: IndicatorValue) -> Self {
        let mut history = VecDeque::with_capacity(MAX_HISTORY_SIZE);
        history.push_front(value.clone());
        Self {
            current: value,
            history,
        }
    }

    fn update(&mut self, value: IndicatorValue) {
        // Move value into history first, then clone for current
        self.history.push_front(value.clone());
        self.current = value;
        // Keep only the last MAX_HISTORY_SIZE values
        if self.history.len() > MAX_HISTORY_SIZE {
            self.history.pop_back();
        }
    }
}

pub struct IndicatorCalculator {
    indicators: Arc<RwLock<HashMap<SmolStr, Box<dyn Indicator + Send + Sync>>>>,
    /// Cache: indicator_id -> CacheEntry (current value + history)
    cache: Arc<RwLock<HashMap<SmolStr, CacheEntry>>>,
    /// Index: pair -> Vec<indicator_id> (for efficient pair-based queries)
    pair_index: Arc<RwLock<HashMap<SmolStr, Vec<SmolStr>>>>,
    /// Index: indicator_name -> Vec<indicator_id> (for efficient name-based queries)
    name_index: Arc<RwLock<HashMap<SmolStr, Vec<SmolStr>>>>,
}

impl Default for IndicatorCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorCalculator {
    pub fn new() -> Self {
        Self {
            indicators: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            pair_index: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new indicator
    pub fn register_indicator(&self, config: IndicatorConfig) -> Result<(), IndicatorError> {
        // Extract values before moving config
        let id = config.id.clone();
        let pair = config.pair.clone();
        let name = config.name.clone();

        let mut indicators = self.indicators.write();

        let indicator: Box<dyn Indicator + Send + Sync> = match name.as_str() {
            "SMA" => Box::new(SimpleMovingAverage::new(config.clone())?),
            "EMA" => Box::new(ExponentialMovingAverage::new(config.clone())?),
            _ => {
                return Err(IndicatorError::CalculationError(SmolStr::new(format!(
                    "Unknown indicator: {}",
                    name
                ))))
            }
        };

        indicators.insert(id.clone(), indicator);
        drop(indicators);

        // Update indices (release indicators lock first)
        {
            let mut pair_index = self.pair_index.write();
            pair_index.entry(pair).or_default().push(id.clone());
        }
        {
            let mut name_index = self.name_index.write();
            name_index.entry(name).or_default().push(id);
        }

        Ok(())
    }

    /// Update all indicators with a new price point
    pub fn update(
        &self,
        pair: &str,
        price_point: PricePoint,
    ) -> Result<Vec<IndicatorValue>, IndicatorError> {
        let pair_smol = SmolStr::new(pair);

        // Use pair_index to get indicator IDs efficiently (read-only, no filtering needed)
        let pair_indicators: Vec<SmolStr> = {
            let pair_index = self.pair_index.read();
            pair_index.get(&pair_smol).cloned().unwrap_or_default()
        };

        if pair_indicators.is_empty() {
            return Ok(Vec::new());
        }

        let mut updated_values = Vec::new();

        // Update each indicator
        for indicator_id in pair_indicators {
            // Get indicator info and update (minimize lock time)
            // PricePoint is Copy, so no clone needed
            let (is_now_ready, value_opt, name, period) = {
                let mut indicators = self.indicators.write();
                let indicator = indicators.get_mut(&indicator_id).ok_or_else(|| {
                    IndicatorError::CalculationError(SmolStr::new("Indicator not found"))
                })?;

                let was_ready = indicator.is_ready();
                indicator.update(price_point)?;
                let is_now_ready = indicator.is_ready();
                let value_opt = if is_now_ready {
                    indicator.value()
                } else {
                    None
                };
                let name = indicator.name().to_string();
                let period = indicator.period();
                drop(indicators);

                // Log when indicator becomes ready for the first time
                if !was_ready && is_now_ready {
                    tracing::info!(
                        "Indicator {} ({}) is now ready (accumulated {} data points)",
                        indicator_id,
                        name,
                        period
                    );
                }

                (is_now_ready, value_opt, name, period)
            };

            // If indicator is ready, get its value and cache it
            if is_now_ready {
                if let Some(value) = value_opt {
                    let indicator_value = IndicatorValue {
                        indicator_id: indicator_id.clone(),
                        indicator_name: SmolStr::new(&name),
                        pair: pair_smol.clone(),
                        value,
                        timestamp: price_point.timestamp,
                        period,
                        is_composite: false,
                    };

                    // Update cache (clone only once for cache entry)
                    let indicator_value_for_cache = indicator_value.clone();
                    {
                        let mut cache = self.cache.write();
                        match cache.get_mut(&indicator_id) {
                            Some(entry) => {
                                entry.update(indicator_value_for_cache);
                                tracing::debug!(
                                    "Updated cached value for {}: {} = {}",
                                    indicator_id,
                                    name,
                                    indicator_value.value
                                );
                            }
                            None => {
                                cache.insert(
                                    indicator_id.clone(),
                                    CacheEntry::new(indicator_value_for_cache),
                                );
                                tracing::info!(
                                    "Cached new indicator value for {}: {} = {}",
                                    indicator_id,
                                    name,
                                    indicator_value.value
                                );
                            }
                        }
                    }

                    updated_values.push(indicator_value);
                } else {
                    tracing::warn!(
                        "Indicator {} ({}) is ready but value() returned None",
                        indicator_id,
                        name
                    );
                }
            }
        }

        Ok(updated_values)
    }

    /// Get cached indicator value (latest)
    pub fn get_value(&self, indicator_id: &str) -> Option<IndicatorValue> {
        let cache = self.cache.read();
        cache
            .get(&SmolStr::new(indicator_id))
            .map(|entry| entry.current.clone())
    }

    /// Get historical values for an indicator (most recent first)
    ///
    /// # Arguments
    /// * `indicator_id` - Indicator ID
    /// * `limit` - Maximum number of historical values to return (default: all)
    pub fn get_history(&self, indicator_id: &str, limit: Option<usize>) -> Vec<IndicatorValue> {
        let cache = self.cache.read();
        if let Some(entry) = cache.get(&SmolStr::new(indicator_id)) {
            if let Some(limit) = limit {
                entry.history.iter().take(limit).cloned().collect()
            } else {
                entry.history.iter().cloned().collect()
            }
        } else {
            Vec::new()
        }
    }

    /// Get all cached values for a pair (latest values only)
    pub fn get_values_for_pair(&self, pair: &str) -> Vec<IndicatorValue> {
        let pair_smol = SmolStr::new(pair);
        let pair_index = self.pair_index.read();
        let cache = self.cache.read();

        if let Some(indicator_ids) = pair_index.get(&pair_smol) {
            indicator_ids
                .iter()
                .filter_map(|id| cache.get(id).map(|entry| entry.current.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all cached values for an indicator name (e.g., all "SMA" indicators)
    pub fn get_values_by_name(&self, indicator_name: &str) -> Vec<IndicatorValue> {
        let name_smol = SmolStr::new(indicator_name);
        let name_index = self.name_index.read();
        let cache = self.cache.read();

        if let Some(indicator_ids) = name_index.get(&name_smol) {
            indicator_ids
                .iter()
                .filter_map(|id| cache.get(id).map(|entry| entry.current.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all current indicator values (latest per indicator)
    pub fn get_all_current_values(&self) -> Vec<IndicatorValue> {
        let cache = self.cache.read();
        cache.values().map(|entry| entry.current.clone()).collect()
    }

    /// Get indicator value by pair and name (e.g., "SMA" for "BTC/USDT")
    ///
    /// Returns the first matching indicator if multiple exist
    pub fn get_value_by_pair_and_name(
        &self,
        pair: &str,
        indicator_name: &str,
    ) -> Option<IndicatorValue> {
        let values = self.get_values_for_pair(pair);
        values
            .into_iter()
            .find(|v| v.indicator_name.as_str() == indicator_name)
    }

    /// Remove an indicator
    pub fn remove_indicator(&self, indicator_id: &str) {
        let id_smol = SmolStr::new(indicator_id);

        // Get indicator config to remove from indices
        let pair_opt = {
            let indicators = self.indicators.read();
            indicators
                .get(&id_smol)
                .map(|ind| (ind.pair().to_string(), ind.name().to_string()))
        };

        // Remove from indicators
        {
            let mut indicators = self.indicators.write();
            indicators.remove(&id_smol);
        }

        // Remove from cache
        {
            let mut cache = self.cache.write();
            cache.remove(&id_smol);
        }

        // Remove from indices
        if let Some((pair, name)) = pair_opt {
            {
                let mut pair_index = self.pair_index.write();
                if let Some(ids) = pair_index.get_mut(&SmolStr::new(&pair)) {
                    ids.retain(|id| id != &id_smol);
                    if ids.is_empty() {
                        pair_index.remove(&SmolStr::new(&pair));
                    }
                }
            }
            {
                let mut name_index = self.name_index.write();
                if let Some(ids) = name_index.get_mut(&SmolStr::new(&name)) {
                    ids.retain(|id| id != &id_smol);
                    if ids.is_empty() {
                        name_index.remove(&SmolStr::new(&name));
                    }
                }
            }
        }
    }

    /// Get access to cache for publisher (returns latest values only)
    /// Note: This creates a snapshot. For frequent access, consider using get_all_current_values() instead.
    pub fn cache(&self) -> Arc<parking_lot::RwLock<HashMap<SmolStr, IndicatorValue>>> {
        let cache = self.cache.read();
        let latest_values: HashMap<SmolStr, IndicatorValue> = cache
            .iter()
            .map(|(id, entry)| (id.clone(), entry.current.clone()))
            .collect();
        Arc::new(parking_lot::RwLock::new(latest_values))
    }

    /// Get all registered indicator IDs
    pub fn get_indicator_ids(&self) -> Vec<SmolStr> {
        let indicators = self.indicators.read();
        indicators.keys().cloned().collect()
    }

    /// Check if an indicator is registered
    pub fn has_indicator(&self, indicator_id: &str) -> bool {
        let indicators = self.indicators.read();
        indicators.contains_key(&SmolStr::new(indicator_id))
    }

    /// Check if any indicators are registered for a pair
    pub fn has_indicators_for_pair(&self, pair: &str) -> bool {
        let pair_index = self.pair_index.read();
        pair_index.contains_key(&SmolStr::new(pair))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::traits::PriceSource;
    use rust_decimal::Decimal;

    fn create_test_config(id: &str, name: &str, pair: &str, period: usize) -> IndicatorConfig {
        IndicatorConfig {
            id: SmolStr::new(id),
            name: SmolStr::new(name),
            pair: SmolStr::new(pair),
            period,
            source: PriceSource::Close,
        }
    }

    #[test]
    fn test_calculator_new() {
        let calc = IndicatorCalculator::new();
        assert!(calc.get_indicator_ids().is_empty());
    }

    #[test]
    fn test_calculator_register_indicator() {
        let calc = IndicatorCalculator::new();
        let config = create_test_config("sma_20", "SMA", "BTCUSDT", 20);

        assert!(calc.register_indicator(config).is_ok());
        assert!(calc.has_indicator("sma_20"));
        assert_eq!(calc.get_indicator_ids().len(), 1);
    }

    #[test]
    fn test_calculator_register_multiple_indicators() {
        let calc = IndicatorCalculator::new();

        let sma_config = create_test_config("sma_20", "SMA", "BTCUSDT", 20);
        let ema_config = create_test_config("ema_12", "EMA", "BTCUSDT", 12);

        assert!(calc.register_indicator(sma_config).is_ok());
        assert!(calc.register_indicator(ema_config).is_ok());

        assert_eq!(calc.get_indicator_ids().len(), 2);
        assert!(calc.has_indicator("sma_20"));
        assert!(calc.has_indicator("ema_12"));
    }

    #[test]
    fn test_calculator_register_unknown_indicator() {
        let calc = IndicatorCalculator::new();
        let config = create_test_config("unknown", "UNKNOWN", "BTCUSDT", 10);

        assert!(calc.register_indicator(config).is_err());
    }

    #[test]
    fn test_calculator_update_single_indicator() {
        let calc = IndicatorCalculator::new();
        let config = create_test_config("sma_3", "SMA", "BTCUSDT", 3);
        calc.register_indicator(config).unwrap();

        let price_point = PricePoint::from_trade(Decimal::from(10), Decimal::ONE, 1000);

        let updated = calc.update("BTCUSDT", price_point).unwrap();
        assert_eq!(updated.len(), 0); // Not ready yet

        // Add two more updates
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(20), Decimal::ONE, 2000),
        )
        .unwrap();
        let updated = calc
            .update(
                "BTCUSDT",
                PricePoint::from_trade(Decimal::from(30), Decimal::ONE, 3000),
            )
            .unwrap();

        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].indicator_id, SmolStr::new("sma_3"));
        assert_eq!(updated[0].value, Decimal::from(20)); // (10+20+30)/3
    }

    #[test]
    fn test_calculator_update_multiple_indicators() {
        let calc = IndicatorCalculator::new();
        let sma_config = create_test_config("sma_2", "SMA", "BTCUSDT", 2);
        let ema_config = create_test_config("ema_2", "EMA", "BTCUSDT", 2);

        calc.register_indicator(sma_config).unwrap();
        calc.register_indicator(ema_config).unwrap();

        // Update twice to make both ready
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(10), Decimal::ONE, 1000),
        )
        .unwrap();
        let updated = calc
            .update(
                "BTCUSDT",
                PricePoint::from_trade(Decimal::from(20), Decimal::ONE, 2000),
            )
            .unwrap();

        assert_eq!(updated.len(), 2); // Both should be ready
    }

    #[test]
    fn test_calculator_update_pair_filtering() {
        let calc = IndicatorCalculator::new();
        let btc_config = create_test_config("sma_btc", "SMA", "BTCUSDT", 2);
        let eth_config = create_test_config("sma_eth", "SMA", "ETHUSDT", 2);

        calc.register_indicator(btc_config).unwrap();
        calc.register_indicator(eth_config).unwrap();

        // Update BTC pair
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(10), Decimal::ONE, 1000),
        )
        .unwrap();
        let updated = calc
            .update(
                "BTCUSDT",
                PricePoint::from_trade(Decimal::from(20), Decimal::ONE, 2000),
            )
            .unwrap();

        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].pair, SmolStr::new("BTCUSDT"));

        // Update ETH pair
        calc.update(
            "ETHUSDT",
            PricePoint::from_trade(Decimal::from(100), Decimal::ONE, 3000),
        )
        .unwrap();
        let updated = calc
            .update(
                "ETHUSDT",
                PricePoint::from_trade(Decimal::from(200), Decimal::ONE, 4000),
            )
            .unwrap();

        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].pair, SmolStr::new("ETHUSDT"));
    }

    #[test]
    fn test_calculator_get_value() {
        let calc = IndicatorCalculator::new();
        let config = create_test_config("sma_2", "SMA", "BTCUSDT", 2);
        calc.register_indicator(config).unwrap();

        // Update to make ready
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(10), Decimal::ONE, 1000),
        )
        .unwrap();
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(20), Decimal::ONE, 2000),
        )
        .unwrap();

        let value = calc.get_value("sma_2").unwrap();
        assert_eq!(value.value, Decimal::from(15)); // (10+20)/2
        assert_eq!(value.indicator_id, SmolStr::new("sma_2"));
    }

    #[test]
    fn test_calculator_get_value_not_found() {
        let calc = IndicatorCalculator::new();
        assert_eq!(calc.get_value("nonexistent"), None);
    }

    #[test]
    fn test_calculator_get_values_for_pair() {
        let calc = IndicatorCalculator::new();
        let sma_config = create_test_config("sma_2", "SMA", "BTCUSDT", 2);
        let ema_config = create_test_config("ema_2", "EMA", "BTCUSDT", 2);

        calc.register_indicator(sma_config).unwrap();
        calc.register_indicator(ema_config).unwrap();

        // Update to make both ready
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(10), Decimal::ONE, 1000),
        )
        .unwrap();
        calc.update(
            "BTCUSDT",
            PricePoint::from_trade(Decimal::from(20), Decimal::ONE, 2000),
        )
        .unwrap();

        let values = calc.get_values_for_pair("BTCUSDT");
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn test_calculator_remove_indicator() {
        let calc = IndicatorCalculator::new();
        let config = create_test_config("sma_20", "SMA", "BTCUSDT", 20);
        calc.register_indicator(config).unwrap();

        assert!(calc.has_indicator("sma_20"));

        calc.remove_indicator("sma_20");
        assert!(!calc.has_indicator("sma_20"));
        assert_eq!(calc.get_value("sma_20"), None);
    }
}
