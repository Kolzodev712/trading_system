use crate::data::{IndicatorValue, PricePoint};
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError};
use crate::indicators::{ExponentialMovingAverage, SimpleMovingAverage};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct IndicatorCalculator {
    indicators: Arc<RwLock<HashMap<SmolStr, Box<dyn Indicator + Send + Sync>>>>,
    cache: Arc<RwLock<HashMap<SmolStr, IndicatorValue>>>, // indicator_id -> latest value
}

impl IndicatorCalculator {
    pub fn new() -> Self {
        Self {
            indicators: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new indicator
    pub fn register_indicator(&self, config: IndicatorConfig) -> Result<(), IndicatorError> {
        let mut indicators = self.indicators.write().unwrap();

        let indicator: Box<dyn Indicator + Send + Sync> = match config.name.as_str() {
            "SMA" => Box::new(SimpleMovingAverage::new(config.clone())?),
            "EMA" => Box::new(ExponentialMovingAverage::new(config.clone())?),
            _ => {
                return Err(IndicatorError::CalculationError(SmolStr::new(format!(
                    "Unknown indicator: {}",
                    config.name
                ))))
            }
        };

        indicators.insert(config.id.clone(), indicator);
        Ok(())
    }

    /// Update all indicators with a new price point
    pub fn update(
        &self,
        pair: &str,
        price_point: PricePoint,
    ) -> Result<Vec<IndicatorValue>, IndicatorError> {
        let mut updated_values = Vec::new();
        let pair_smol = SmolStr::new(pair);

        // Get all indicators for this pair
        let indicators = self.indicators.read().unwrap();
        let pair_indicators: Vec<SmolStr> = indicators
            .iter()
            .filter(|(_, ind)| ind.pair() == pair)
            .map(|(id, _)| id.clone())
            .collect();
        drop(indicators);

        // Update each indicator
        for indicator_id in pair_indicators {
            let mut indicators = self.indicators.write().unwrap();
            let indicator = indicators.get_mut(&indicator_id).ok_or_else(|| {
                IndicatorError::CalculationError(SmolStr::new("Indicator not found"))
            })?;

            indicator.update(price_point)?;

            // If indicator is ready, get its value and cache it
            if indicator.is_ready() {
                if let Some(value) = indicator.value() {
                    let indicator_value = IndicatorValue {
                        indicator_id: indicator_id.clone(),
                        indicator_name: SmolStr::new(indicator.name()),
                        pair: pair_smol.clone(),
                        value,
                        timestamp: price_point.timestamp,
                        period: indicator.period(),
                    };

                    // Update cache
                    let mut cache = self.cache.write().unwrap();
                    cache.insert(indicator_id.clone(), indicator_value.clone());
                    drop(cache);

                    updated_values.push(indicator_value);
                }
            }
            drop(indicators);
        }

        Ok(updated_values)
    }

    /// Get cached indicator value
    pub fn get_value(&self, indicator_id: &str) -> Option<IndicatorValue> {
        let cache = self.cache.read().unwrap();
        cache.get(&SmolStr::new(indicator_id)).cloned()
    }

    /// Get all cached values for a pair
    pub fn get_values_for_pair(&self, pair: &str) -> Vec<IndicatorValue> {
        let pair_smol = SmolStr::new(pair);
        let cache = self.cache.read().unwrap();
        cache
            .values()
            .filter(|v| v.pair == pair_smol)
            .cloned()
            .collect()
    }

    /// Remove an indicator
    pub fn remove_indicator(&self, indicator_id: &str) {
        let id_smol = SmolStr::new(indicator_id);
        let mut indicators = self.indicators.write().unwrap();
        indicators.remove(&id_smol);

        let mut cache = self.cache.write().unwrap();
        cache.remove(&id_smol);
    }

    /// Get access to cache for publisher
    pub fn cache(&self) -> Arc<RwLock<HashMap<SmolStr, IndicatorValue>>> {
        self.cache.clone()
    }

    /// Get all registered indicator IDs
    pub fn get_indicator_ids(&self) -> Vec<SmolStr> {
        let indicators = self.indicators.read().unwrap();
        indicators.keys().cloned().collect()
    }

    /// Check if an indicator is registered
    pub fn has_indicator(&self, indicator_id: &str) -> bool {
        let indicators = self.indicators.read().unwrap();
        indicators.contains_key(&SmolStr::new(indicator_id))
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
