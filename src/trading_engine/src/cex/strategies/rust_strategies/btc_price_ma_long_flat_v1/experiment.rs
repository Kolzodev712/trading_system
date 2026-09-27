//! Score BTCPriceMALongFlatV1 through the existing offline simulator.

use crate::cex::strategies::rust_strategies::donchian_daily_v1::offline::DecisionLog;
use super::position_qty;
use crate::cex::strategies::rust_strategies::donchian_daily_v1::backtest::{
    FROZEN_EVALUATION_END_MS, FROZEN_EVALUATION_START_MS,
};
use rust_decimal::Decimal;

use crate::cex::strategies::rust_strategies::donchian_daily_v1::backtest::{
    changes_for, required_execution_minutes_for_target_changes, research_scenarios, simulate,
    target_changes, validate_coverage, BacktestError, Book, MarketInputs, SimulateRequest,
    Simulation, FILL_ASSUMPTION, RESULT_LABEL,
};

pub fn book_label(lookback: usize) -> String {
    format!("btc_price_ma_long_flat_v1_n{lookback}")
}

pub fn book_note(lookback: usize) -> String {
    format!(
        "BTCPriceMALongFlatV1 with N={lookback} calendar-day geometric mean. Decisions come from offline replay; the simulator does not recompute the signal."
    )
}

pub fn score_lookback(
    log: &DecisionLog,
    lookback: usize,
    market: &MarketInputs,
) -> Result<Vec<Simulation>, BacktestError> {
    let start = FROZEN_EVALUATION_START_MS;
    let end = FROZEN_EVALUATION_END_MS;
    let qty = position_qty();
    let changes = target_changes(&log.decisions)?;
    for change in &changes {
        let size = change.target_qty.abs();
        if size != Decimal::ZERO && size != qty {
            return Err(BacktestError::new(format!(
                "target {} is not 0 or {qty}",
                change.target_qty
            )));
        }
    }
    let required = required_execution_minutes_for_target_changes(&changes, start, end, qty)?;
    validate_coverage(start, end, &required, market)?;
    let label = book_label(lookback);
    let note = book_note(lookback);
    let mut out = Vec::new();
    for scenario in research_scenarios(start, end) {
        let simulation = simulate(&SimulateRequest {
            book: &label,
            book_note: &note,
            model_direction_at_end: &log.run.final_direction,
            signal_config_hash: &log.run.config_hash,
            changes: &changes,
            scenario: &scenario,
            minutes: &market.minutes,
            funding: &market.funding,
            hourly_marks: &market.hourly_marks,
            finer_marks: &market.finer_marks,
        })?;
        out.push(simulation);
    }
    Ok(out)
}

pub fn score_fixed_long(market: &MarketInputs) -> Result<Vec<Simulation>, BacktestError> {
    let start = FROZEN_EVALUATION_START_MS;
    let end = FROZEN_EVALUATION_END_MS;
    let qty = position_qty();
    let changes = changes_for(Book::FixedLongBenchmark, &[], start, qty)?;
    let required = required_execution_minutes_for_target_changes(&changes, start, end, qty)?;
    validate_coverage(start, end, &required, market)?;
    let mut out = Vec::new();
    for scenario in research_scenarios(start, end) {
        let simulation = simulate(&SimulateRequest {
            book: Book::FixedLongBenchmark.as_str(),
            book_note: "Fixed +0.001 BTC long in the same linear perpetual. decision_time is the performance window start; there is no signal candle. Fees, execution adjustment, delay, funding, marks, and terminal treatment match the candidate run.",
            model_direction_at_end: "long",
            signal_config_hash: "benchmark-fixed-long",
            changes: &changes,
            scenario: &scenario,
            minutes: &market.minutes,
            funding: &market.funding,
            hourly_marks: &market.hourly_marks,
            finer_marks: &market.finer_marks,
        })?;
        out.push(simulation);
    }
    Ok(out)
}

pub fn scenario_by_name<'a>(
    simulations: &'a [Simulation],
    name: &str,
) -> Result<&'a Simulation, BacktestError> {
    simulations
        .iter()
        .find(|sim| sim.summary.scenario_name == name)
        .ok_or_else(|| BacktestError::new(format!("missing scenario {name}")))
}

pub const RESULT_AND_FILL: (&str, &str) = (RESULT_LABEL, FILL_ASSUMPTION);
