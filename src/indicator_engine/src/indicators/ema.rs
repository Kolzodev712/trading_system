use crate::data::price_point::PricePoint;
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError, PriceSource};
use rust_decimal::Decimal;

/// Exponential Moving Average (EMA) indicator
pub struct ExponentialMovingAverage {
    config: IndicatorConfig,
    current_ema: Option<Decimal>,
    multiplier: Decimal, // 2 / (period + 1)
    price_count: usize,
}

impl ExponentialMovingAverage {
    /// Create a new EMA indicator
    pub fn new(config: IndicatorConfig) -> Result<Self, IndicatorError> {
        let period = config.period;
        if period == 0 {
            return Err(IndicatorError::InvalidPeriod { period });
        }

        // Calculate multiplier: 2 / (period + 1)
        let multiplier = Decimal::from(2) / Decimal::from(period + 1);

        Ok(Self {
            config,
            current_ema: None,
            multiplier,
            price_count: 0,
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

impl Indicator for ExponentialMovingAverage {
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
        self.price_count += 1;

        match self.current_ema {
            None => {
                // First value: use the price directly
                self.current_ema = Some(price);
            }
            Some(prev_ema) => {
                // EMA = (Price - Previous EMA) * Multiplier + Previous EMA
                // Simplified: EMA = Price * Multiplier + Previous EMA * (1 - Multiplier)
                let new_ema = price * self.multiplier + prev_ema * (Decimal::ONE - self.multiplier);
                self.current_ema = Some(new_ema);
            }
        }

        Ok(())
    }

    fn value(&self) -> Option<Decimal> {
        self.current_ema
    }

    fn is_ready(&self) -> bool {
        self.price_count >= self.config.period
    }

    fn reset(&mut self) {
        self.current_ema = None;
        self.price_count = 0;
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
    fn test_ema_new() {
        let config = IndicatorConfig {
            id: SmolStr::new("ema_12"),
            name: SmolStr::new("EMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 12,
            source: PriceSource::Close,
        };
        let ema = ExponentialMovingAverage::new(config).unwrap();
        assert_eq!(ema.period(), 12);
        assert!(!ema.is_ready());
    }

    #[test]
    fn test_ema_invalid_period() {
        let config = IndicatorConfig {
            id: SmolStr::new("ema_0"),
            name: SmolStr::new("EMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 0,
            source: PriceSource::Close,
        };
        assert!(ExponentialMovingAverage::new(config).is_err());
    }

    #[test]
    fn test_ema_update_first_value() {
        let config = IndicatorConfig {
            id: SmolStr::new("ema_3"),
            name: SmolStr::new("EMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 3,
            source: PriceSource::Close,
        };
        let mut ema = ExponentialMovingAverage::new(config).unwrap();

        // First value should be used directly
        ema.update(PricePoint::from_trade(
            Decimal::from(10),
            Decimal::ONE,
            1000,
        ))
        .unwrap();
        assert_eq!(ema.value().unwrap(), Decimal::from(10));
    }

    #[test]
    fn test_ema_update_multiple() {
        let config = IndicatorConfig {
            id: SmolStr::new("ema_3"),
            name: SmolStr::new("EMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 3,
            source: PriceSource::Close,
        };
        let mut ema = ExponentialMovingAverage::new(config).unwrap();

        // First value: 10
        ema.update(PricePoint::from_trade(
            Decimal::from(10),
            Decimal::ONE,
            1000,
        ))
        .unwrap();
        assert_eq!(ema.value().unwrap(), Decimal::from(10));

        // Second value: 20
        // Multiplier = 2 / (3 + 1) = 0.5
        // EMA = 20 * 0.5 + 10 * 0.5 = 15
        ema.update(PricePoint::from_trade(
            Decimal::from(20),
            Decimal::ONE,
            2000,
        ))
        .unwrap();
        assert_eq!(ema.value().unwrap(), Decimal::from(15));

        // Third value: 30
        // EMA = 30 * 0.5 + 15 * 0.5 = 22.5
        ema.update(PricePoint::from_trade(
            Decimal::from(30),
            Decimal::ONE,
            3000,
        ))
        .unwrap();
        assert_eq!(ema.value().unwrap(), Decimal::from(225) / Decimal::from(10)); // 22.5
        assert!(ema.is_ready());
    }

    #[test]
    fn test_ema_reset() {
        let config = IndicatorConfig {
            id: SmolStr::new("ema_3"),
            name: SmolStr::new("EMA"),
            pair: SmolStr::new("BTCUSDT"),
            period: 3,
            source: PriceSource::Close,
        };
        let mut ema = ExponentialMovingAverage::new(config).unwrap();

        ema.update(PricePoint::from_trade(
            Decimal::from(10),
            Decimal::ONE,
            1000,
        ))
        .unwrap();
        assert!(ema.value().is_some());

        ema.reset();
        assert!(!ema.is_ready());
        assert_eq!(ema.value(), None);
    }
}
