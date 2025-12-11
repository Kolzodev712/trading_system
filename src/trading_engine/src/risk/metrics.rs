//! Risk metrics - P&L tracking and calculations
use crate::state::{Position, TradingState};
use chrono::Utc;
use rust_decimal::Decimal;
use std::collections::HashMap;
use trading_core::types::common::Pair;

/// Risk metrics - tracks P&L and exposure
#[derive(Debug, Clone)]
pub struct RiskMetrics {
    /// Real-time unrealized P&L (from open positions)
    pub unrealized_pnl: Decimal,

    /// Realized P&L (from closed positions)
    pub realized_pnl: Decimal,

    /// Total P&L (unrealized + realized)
    pub total_pnl: Decimal,

    /// Daily P&L (tracked per day, resets at midnight)
    pub daily_pnl: Decimal,

    /// Total exposure (notional value of all positions)
    pub total_exposure: Decimal,

    /// Total position size (sum of all position quantities)
    pub total_position: Decimal,

    /// Last calculated timestamp
    pub timestamp: u64,
}

impl Default for RiskMetrics {
    fn default() -> Self {
        Self {
            unrealized_pnl: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            total_pnl: Decimal::ZERO,
            daily_pnl: Decimal::ZERO,
            total_exposure: Decimal::ZERO,
            total_position: Decimal::ZERO,
            timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }
}

/// Daily P&L tracker - tracks daily P&L separately
#[derive(Debug, Clone)]
pub struct DailyPnLTracker {
    /// Current day's P&L
    daily_pnl: Decimal,

    /// Last reset date (YYYY-MM-DD format)
    last_reset_date: String,

    /// Daily P&L history
    daily_history: Vec<DailyPnLEntry>,
}

#[derive(Debug, Clone)]
pub struct DailyPnLEntry {
    pub date: String,
    pub pnl: Decimal,
    pub realized: Decimal,
    pub unrealized: Decimal,
}

impl DailyPnLTracker {
    pub fn new() -> Self {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        Self {
            daily_pnl: Decimal::ZERO,
            last_reset_date: today,
            daily_history: Vec::new(),
        }
    }

    /// Update daily P&L
    pub fn update(&mut self, realized_pnl: Decimal, unrealized_pnl: Decimal) {
        self.reset_if_new_day();
        self.daily_pnl = realized_pnl + unrealized_pnl;
    }

    /// Reset for new day
    pub fn reset_if_new_day(&mut self) {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        if today != self.last_reset_date {
            // Save previous day's entry
            if self.daily_pnl != Decimal::ZERO {
                self.daily_history.push(DailyPnLEntry {
                    date: self.last_reset_date.clone(),
                    pnl: self.daily_pnl,
                    realized: Decimal::ZERO, // TODO: Track realized separately
                    unrealized: self.daily_pnl,
                });
            }

            // Reset for new day
            self.daily_pnl = Decimal::ZERO;
            self.last_reset_date = today;
        }
    }

    /// Get current daily P&L
    pub fn get_daily_pnl(&self) -> Decimal {
        self.daily_pnl
    }

    /// Get daily history
    pub fn get_history(&self) -> &[DailyPnLEntry] {
        &self.daily_history
    }
}

impl Default for DailyPnLTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Risk metrics calculator
pub struct RiskMetricsCalculator {
    daily_pnl_tracker: DailyPnLTracker,
}

impl RiskMetricsCalculator {
    pub fn new() -> Self {
        Self {
            daily_pnl_tracker: DailyPnLTracker::new(),
        }
    }

    /// Calculate all risk metrics
    pub fn calculate(
        &mut self,
        trading_state: &TradingState,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> RiskMetrics {
        // Calculate unrealized P&L
        let unrealized_pnl =
            self.calculate_unrealized_pnl(&trading_state.positions, current_prices);

        // Calculate total exposure
        let total_exposure = self.calculate_exposure(&trading_state.positions, current_prices);

        // Calculate total position size
        let total_position = self.calculate_total_position(&trading_state.positions);

        // Update daily P&L tracker
        self.daily_pnl_tracker.update(Decimal::ZERO, unrealized_pnl); // TODO: Track realized separately

        let daily_pnl = self.daily_pnl_tracker.get_daily_pnl();

        // For now, realized P&L is zero (we'll track this when positions are closed)
        let realized_pnl = Decimal::ZERO;
        let total_pnl = unrealized_pnl + realized_pnl;

        RiskMetrics {
            unrealized_pnl,
            realized_pnl,
            total_pnl,
            daily_pnl,
            total_exposure,
            total_position,
            timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }

    /// Calculate unrealized P&L from open positions
    fn calculate_unrealized_pnl(
        &self,
        positions: &HashMap<Pair, Position>,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> Decimal {
        positions
            .iter()
            .filter_map(|(pair, position)| {
                current_prices.get(pair).map(|current_price| {
                    // P&L = (current_price - avg_price) * quantity
                    let price_diff = *current_price - position.avg_price;
                    price_diff * position.quantity
                })
            })
            .sum()
    }

    /// Calculate total exposure (notional value)
    fn calculate_exposure(
        &self,
        positions: &HashMap<Pair, Position>,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> Decimal {
        positions
            .iter()
            .filter_map(|(pair, position)| {
                current_prices
                    .get(pair)
                    .map(|price| position.quantity.abs() * *price)
            })
            .sum()
    }

    /// Calculate total position size
    fn calculate_total_position(&self, positions: &HashMap<Pair, Position>) -> Decimal {
        positions.values().map(|p| p.quantity.abs()).sum()
    }

    /// Get daily P&L tracker
    pub fn get_daily_pnl_tracker(&self) -> &DailyPnLTracker {
        &self.daily_pnl_tracker
    }
}

impl Default for RiskMetricsCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TradingState;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::collections::HashMap;
    use trading_core::types::common::Pair;

    #[test]
    fn test_risk_metrics_default() {
        let metrics = RiskMetrics::default();
        assert_eq!(metrics.unrealized_pnl, Decimal::ZERO);
        assert_eq!(metrics.realized_pnl, Decimal::ZERO);
        assert_eq!(metrics.total_pnl, Decimal::ZERO);
        assert_eq!(metrics.daily_pnl, Decimal::ZERO);
        assert_eq!(metrics.total_exposure, Decimal::ZERO);
        assert_eq!(metrics.total_position, Decimal::ZERO);
    }

    #[test]
    fn test_daily_pnl_tracker_new() {
        let tracker = DailyPnLTracker::new();
        assert_eq!(tracker.get_daily_pnl(), Decimal::ZERO);
        assert!(tracker.get_history().is_empty());
    }

    #[test]
    fn test_daily_pnl_tracker_update() {
        let mut tracker = DailyPnLTracker::new();
        tracker.update(dec!(100), dec!(50));
        assert_eq!(tracker.get_daily_pnl(), dec!(150));
    }

    #[test]
    fn test_risk_metrics_calculator_new() {
        let _calc = RiskMetricsCalculator::new();
        // Should not panic
        assert!(true);
    }

    #[test]
    fn test_calculate_unrealized_pnl() {
        let calc = RiskMetricsCalculator::new();
        let mut positions = HashMap::new();
        let mut prices = HashMap::new();

        let pair = Pair::new("BTC", "USDT");
        positions.insert(
            pair.clone(),
            Position {
                symbol: pair.clone(),
                quantity: dec!(1),
                avg_price: dec!(50000),
            },
        );
        prices.insert(pair.clone(), dec!(51000)); // Price increased by 1000

        let pnl = calc.calculate_unrealized_pnl(&positions, &prices);
        assert_eq!(pnl, dec!(1000)); // (51000 - 50000) * 1
    }

    #[test]
    fn test_calculate_exposure() {
        let calc = RiskMetricsCalculator::new();
        let mut positions = HashMap::new();
        let mut prices = HashMap::new();

        let pair = Pair::new("BTC", "USDT");
        positions.insert(
            pair.clone(),
            Position {
                symbol: pair.clone(),
                quantity: dec!(2),
                avg_price: dec!(50000),
            },
        );
        prices.insert(pair.clone(), dec!(51000));

        let exposure = calc.calculate_exposure(&positions, &prices);
        assert_eq!(exposure, dec!(102000)); // 2 * 51000
    }

    #[test]
    fn test_calculate_total_position() {
        let calc = RiskMetricsCalculator::new();
        let mut positions = HashMap::new();

        let pair1 = Pair::new("BTC", "USDT");
        let pair2 = Pair::new("ETH", "USDT");

        positions.insert(
            pair1.clone(),
            Position {
                symbol: pair1,
                quantity: dec!(1),
                avg_price: dec!(50000),
            },
        );
        positions.insert(
            pair2.clone(),
            Position {
                symbol: pair2,
                quantity: dec!(10),
                avg_price: dec!(3000),
            },
        );

        let total = calc.calculate_total_position(&positions);
        assert_eq!(total, dec!(11)); // 1 + 10
    }

    #[test]
    fn test_calculate_metrics() {
        let mut calc = RiskMetricsCalculator::new();
        let trading_state = TradingState::new();
        let mut prices = HashMap::new();

        let pair = Pair::new("BTC", "USDT");
        prices.insert(pair, dec!(51000));

        let metrics = calc.calculate(&trading_state, &prices);

        // With no positions, all should be zero
        assert_eq!(metrics.unrealized_pnl, Decimal::ZERO);
        assert_eq!(metrics.total_exposure, Decimal::ZERO);
        assert_eq!(metrics.total_position, Decimal::ZERO);
    }
}
