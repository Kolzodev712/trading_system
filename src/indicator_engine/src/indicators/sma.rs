use crate::data::price_point::PricePoint;
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError, PriceSource};
use rust_decimal::Decimal;
use std::collections::VecDeque;

/// Simple Moving Average (SMA) indicator
pub struct SimpleMovingAverage {
    config: IndicatorConfig,
    prices: VecDeque<Decimal>,
    current_sum: Decimal,
}

impl SimpleMovingAverage {
    /// Create a new SMA indicator
    pub fn new(config: IndicatorConfig) -> Result<Self, IndicatorError> {
        let period = config.period;
        if period == 0 {
            return Err(IndicatorError::InvalidPeriod { period });
        }

        Ok(Self {
            config,
            prices: VecDeque::with_capacity(period),
            current_sum: Decimal::ZERO,
        })
    }

    /// Get the price value based on the configured source
    fn get_price_value(&self, price_point: &PricePoint) -> Decimal {
        match self.config.source {
            PriceSource::Close => price_point.close,
            PriceSource::Open => price_point.open,
            PriceSource::High => price_point.high,
            PriceSource::Low => price_point.low,
            PriceSource::Volume => price_point.volume,
            PriceSource::TypicalPrice => {
                (price_point.high + price_point.low + price_point.close) / Decimal::from(3)
            }
            PriceSource::WeightedPrice => {
                (price_point.high + price_point.low + Decimal::from(2) * price_point.close)
                    / Decimal::from(4)
            }
        }
    }
}

impl Indicator for SimpleMovingAverage {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn pair(&self) -> &str {
        &self.config.pair
    }

    fn period(&self) -> usize {
        self.config.period
    }

    fn update(&mut self, price_point: PricePoint) -> Result<(), IndicatorError> {
        let price = self.get_price_value(&price_point);

        // If we have reached the period, remove the oldest value
        if self.prices.len() >= self.config.period {
            if let Some(old_price) = self.prices.pop_front() {
                self.current_sum -= old_price;
            }
        }

        // Add the new price
        self.prices.push_back(price);
        self.current_sum += price;

        Ok(())
    }

    fn value(&self) -> Option<Decimal> {
        if !self.is_ready() {
            return None;
        }

        Some(self.current_sum / Decimal::from(self.prices.len()))
    }

    fn is_ready(&self) -> bool {
        self.prices.len() >= self.config.period
    }

    fn reset(&mut self) {
        self.prices.clear();
        self.current_sum = Decimal::ZERO;
    }

    fn config(&self) -> IndicatorConfig {
        self.config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol_str::SmolStr;

    #[test]
    fn test_sma_new() {
        let config = IndicatorConfig {
            id: SmolStr::new("sma_20"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 20,
            source: PriceSource::Close,
        };
        let sma = SimpleMovingAverage::new(config).unwrap();
        assert_eq!(sma.period(), 20);
        assert!(!sma.is_ready());
    }

    #[test]
    fn test_sma_invalid_period() {
        let config = IndicatorConfig {
            id: SmolStr::new("sma_0"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 0,
            source: PriceSource::Close,
        };
        assert!(SimpleMovingAverage::new(config).is_err());
    }

    #[test]
    fn test_sma_update_and_value() {
        let config = IndicatorConfig {
            id: SmolStr::new("sma_3"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 3,
            source: PriceSource::Close,
        };
        let mut sma = SimpleMovingAverage::new(config).unwrap();

        // Add 3 prices: 10, 20, 30
        sma.update(PricePoint::from_trade(
            Decimal::from(10),
            Decimal::ONE,
            1000,
        ))
        .unwrap();
        sma.update(PricePoint::from_trade(
            Decimal::from(20),
            Decimal::ONE,
            2000,
        ))
        .unwrap();
        sma.update(PricePoint::from_trade(
            Decimal::from(30),
            Decimal::ONE,
            3000,
        ))
        .unwrap();

        assert!(sma.is_ready());
        let value = sma.value().unwrap();
        // Average of 10, 20, 30 = 20
        assert_eq!(value, Decimal::from(20));
    }

    #[test]
    fn test_sma_rolling_average() {
        let config = IndicatorConfig {
            id: SmolStr::new("sma_3"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 3,
            source: PriceSource::Close,
        };
        let mut sma = SimpleMovingAverage::new(config).unwrap();

        // Add 3 prices: 10, 20, 30 -> average = 20
        sma.update(PricePoint::from_trade(
            Decimal::from(10),
            Decimal::ONE,
            1000,
        ))
        .unwrap();
        sma.update(PricePoint::from_trade(
            Decimal::from(20),
            Decimal::ONE,
            2000,
        ))
        .unwrap();
        sma.update(PricePoint::from_trade(
            Decimal::from(30),
            Decimal::ONE,
            3000,
        ))
        .unwrap();
        assert_eq!(sma.value().unwrap(), Decimal::from(20));

        // Add 40, should remove 10 -> average of 20, 30, 40 = 30
        sma.update(PricePoint::from_trade(
            Decimal::from(40),
            Decimal::ONE,
            4000,
        ))
        .unwrap();
        assert_eq!(sma.value().unwrap(), Decimal::from(30));
    }

    #[test]
    fn test_sma_reset() {
        let config = IndicatorConfig {
            id: SmolStr::new("sma_3"),
            name: SmolStr::new("SMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 3,
            source: PriceSource::Close,
        };
        let mut sma = SimpleMovingAverage::new(config).unwrap();

        sma.update(PricePoint::from_trade(
            Decimal::from(10),
            Decimal::ONE,
            1000,
        ))
        .unwrap();
        sma.update(PricePoint::from_trade(
            Decimal::from(20),
            Decimal::ONE,
            2000,
        ))
        .unwrap();
        sma.update(PricePoint::from_trade(
            Decimal::from(30),
            Decimal::ONE,
            3000,
        ))
        .unwrap();
        assert!(sma.is_ready());

        sma.reset();
        assert!(!sma.is_ready());
        assert_eq!(sma.value(), None);
    }
}
