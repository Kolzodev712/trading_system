//! Predeclared development-screen gates for the 28-day primary configuration.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::donchian_daily_v1::backtest::Simulation;

pub const BENCHMARK_COST_A_NET: &str = "48.360542973936";
pub const BENCHMARK_COST_A_DRAWDOWN: &str = "44.3887151092231";

pub const MIN_PROFIT_FRACTION: &str = "0.8";
pub const MAX_DRAWDOWN_FRACTION: &str = "0.7";
pub const MIN_COMPLETED_ROUND_TRIPS: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct GateOutcome {
    pub gate: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenResult {
    pub primary_lookback: usize,
    pub disposition: String,
    pub gates: Vec<GateOutcome>,
}

pub fn notional_exposure(samples: &[(Decimal, Decimal)]) -> Decimal {
    samples
        .iter()
        .map(|(position, mark)| position.abs() * mark)
        .sum()
}

pub fn evaluate_primary_screen(
    primary_cost_a: &Simulation,
    primary_higher: &Simulation,
    primary_delayed: &Simulation,
    diagnostic_14_cost_a: &Simulation,
    diagnostic_70_cost_a: &Simulation,
    benchmark_cost_a: &Simulation,
) -> ScreenResult {
    let bench_net = parse_decimal(BENCHMARK_COST_A_NET);
    let bench_dd = parse_decimal(BENCHMARK_COST_A_DRAWDOWN);
    let min_net = bench_net * parse_decimal(MIN_PROFIT_FRACTION);
    let max_dd = bench_dd * parse_decimal(MAX_DRAWDOWN_FRACTION);

    let candidate_samples: Vec<_> = primary_cost_a
        .pnl
        .iter()
        .map(|s| (s.position, s.mark_price))
        .collect();
    let benchmark_samples: Vec<_> = benchmark_cost_a
        .pnl
        .iter()
        .map(|s| (s.position, s.mark_price))
        .collect();
    let f = notional_exposure(&candidate_samples) / notional_exposure(&benchmark_samples);
    let scaled_net = benchmark_cost_a.summary.net_pnl * f;

    let gates = vec![
        gate(
            "cost_a_net_positive",
            primary_cost_a.summary.net_pnl > Decimal::ZERO,
            format!("net {}", primary_cost_a.summary.net_pnl),
        ),
        gate(
            "cost_a_profit_retention_80pct",
            primary_cost_a.summary.net_pnl >= min_net,
            format!(
                "net {} requires >= {}",
                primary_cost_a.summary.net_pnl, min_net
            ),
        ),
        gate(
            "cost_a_drawdown_70pct_of_benchmark",
            primary_cost_a.summary.max_drawdown_usdt <= max_dd,
            format!(
                "drawdown {} requires <= {}",
                primary_cost_a.summary.max_drawdown_usdt, max_dd
            ),
        ),
        gate(
            "beats_notional_matched_passive",
            primary_cost_a.summary.net_pnl > scaled_net,
            format!(
                "net {} vs scaled passive {} (f={})",
                primary_cost_a.summary.net_pnl, scaled_net, f
            ),
        ),
        gate(
            "higher_cost_net_positive",
            primary_higher.summary.net_pnl > Decimal::ZERO,
            format!("net {}", primary_higher.summary.net_pnl),
        ),
        gate(
            "delayed_execution_net_positive",
            primary_delayed.summary.net_pnl > Decimal::ZERO,
            format!("net {}", primary_delayed.summary.net_pnl),
        ),
        gate(
            "completed_round_trips_at_least_20",
            primary_cost_a.summary.completed_trades >= MIN_COMPLETED_ROUND_TRIPS,
            format!(
                "completed trades {}",
                primary_cost_a.summary.completed_trades
            ),
        ),
        gate(
            "diagnostic_14_cost_a_positive",
            diagnostic_14_cost_a.summary.net_pnl > Decimal::ZERO,
            format!("net {}", diagnostic_14_cost_a.summary.net_pnl),
        ),
        gate(
            "diagnostic_70_cost_a_positive",
            diagnostic_70_cost_a.summary.net_pnl > Decimal::ZERO,
            format!("net {}", diagnostic_70_cost_a.summary.net_pnl),
        ),
    ];

    let all_pass = gates.iter().all(|g| g.passed);
    let disposition = if all_pass {
        "eligible_for_holdout_review".to_string()
    } else {
        "reject".to_string()
    };
    ScreenResult {
        primary_lookback: 28,
        disposition,
        gates,
    }
}

fn gate(name: &str, passed: bool, detail: String) -> GateOutcome {
    GateOutcome {
        gate: name.to_string(),
        passed,
        detail,
    }
}

fn parse_decimal(value: &str) -> Decimal {
    value.parse().expect("valid decimal literal")
}
