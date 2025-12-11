//! Risk metrics tests

use rust_decimal_macros::dec;
use trading_core::types::common::Pair;
use trading_engine::risk::metrics::{DailyPnLTracker, RiskMetricsCalculator};
use trading_engine::state::TradingState;

#[tokio::test]
async fn test_risk_metrics_calculation() {
    let mut calculator = RiskMetricsCalculator::new();
    let trading_state = TradingState::new();
    let mut prices = std::collections::HashMap::new();

    prices.insert(Pair::new("BTC", "USDT"), dec!(51000));

    let metrics = calculator.calculate(&trading_state, &prices);

    // With no positions, all should be zero
    assert_eq!(metrics.unrealized_pnl, dec!(0));
    assert_eq!(metrics.total_exposure, dec!(0));
    assert_eq!(metrics.total_position, dec!(0));
}

#[tokio::test]
async fn test_daily_pnl_tracker() {
    let mut tracker = DailyPnLTracker::new();

    tracker.update(dec!(100), dec!(50));
    assert_eq!(tracker.get_daily_pnl(), dec!(150));

    tracker.update(dec!(200), dec!(100));
    assert_eq!(tracker.get_daily_pnl(), dec!(300));
}

#[tokio::test]
async fn test_risk_metrics_with_positions() {
    let mut calculator = RiskMetricsCalculator::new();
    let mut trading_state = TradingState::new();
    let mut prices = std::collections::HashMap::new();

    // Add position
    trading_state.update_position(Pair::new("BTC", "USDT"), dec!(0.001), dec!(50000));

    prices.insert(Pair::new("BTC", "USDT"), dec!(51000));

    let metrics = calculator.calculate(&trading_state, &prices);

    // Should have unrealized P&L
    assert!(metrics.unrealized_pnl > dec!(0));
    assert!(metrics.total_exposure > dec!(0));
    assert_eq!(metrics.total_position, dec!(0.001));
}
