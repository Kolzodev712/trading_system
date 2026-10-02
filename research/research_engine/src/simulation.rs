//! Offline execution simulator and futures PnL ledger.
//!
//! Decisions come from the existing replay. This module does not recompute
//! channels, place orders, or open a network connection.

use crate::daily_dataset::{DecisionLog, DecisionRow, EmittedTarget};
use chrono::{Datelike, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as FmtWrite;
use std::path::Path;

pub const MINUTE_MS: u64 = 60_000;
pub const HOUR_MS: u64 = 3_600_000;
pub const FROZEN_DATASET_HASH: &str =
    "sha256:bb642e2c001a0df039b15fbad7a7032cab61ae41f29eef785823facb55265236";
pub const FROZEN_CONFIG_HASH: &str =
    "sha256:8057b5024b6df11096bc7541e8f470e52794f5e2025503da45beedc0aab33114";
pub const FROZEN_EVALUATION_START_MS: u64 = 1_674_259_200_000;
pub const FROZEN_EVALUATION_END_MS: u64 = 1_767_225_600_000;

pub const RESULT_LABEL: &str = "prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency";
pub const FILL_ASSUMPTION: &str = "full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness";
const DRAWDOWN_SAMPLING: &str = "hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestError {
    message: String,
}

impl BacktestError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BacktestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BacktestError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Book {
    DonchianDailyV1,
    FixedLongBenchmark,
}

impl Book {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DonchianDailyV1 => "donchian_daily_v1",
            Self::FixedLongBenchmark => "fixed_long_0.001_btc",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::DonchianDailyV1 => {
                "DonchianDailyV1 decisions from the frozen replay. The simulator does not recompute channels."
            }
            Self::FixedLongBenchmark => {
                "Fixed +0.001 BTC long in the same linear perpetual. decision_time is the performance window start and there is no signal candle. Fees, execution adjustment, delay, funding, marks, and terminal treatment match the Donchian run. This comparison does not change the Donchian rules and is not a spot return."
            }
        }
    }
}

pub fn frozen_position_qty() -> Decimal {
    Decimal::new(1, 3)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionScenario {
    pub name: String,
    pub fee_bps: Decimal,
    pub execution_adjustment_bps: Decimal,
    pub decision_delay_ms: u64,
    pub performance_start_ms: u64,
    pub performance_end_exclusive_ms: u64,
}

pub fn research_scenarios(
    performance_start_ms: u64,
    performance_end_exclusive_ms: u64,
) -> Vec<ExecutionScenario> {
    [
        ("zero_cost_diagnostic", 0, 0, 10),
        ("cost_a", 6, 2, 10),
        ("higher_cost", 10, 10, 10),
        ("delayed_execution", 6, 2, 300),
    ]
    .into_iter()
    .map(
        |(name, fee_bps, execution_adjustment_bps, delay_seconds)| ExecutionScenario {
            name: name.to_string(),
            fee_bps: Decimal::from(fee_bps),
            execution_adjustment_bps: Decimal::from(execution_adjustment_bps),
            decision_delay_ms: delay_seconds * 1_000,
            performance_start_ms,
            performance_end_exclusive_ms,
        },
    )
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetChange {
    pub decision_id: String,
    pub source_start_ms: Option<u64>,
    pub source_end_ms: Option<u64>,
    pub decision_time_ms: u64,
    pub target_qty: Decimal,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingRecord {
    pub settlement_time_ms: u64,
    pub funding_rate: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageNote {
    pub series: String,
    pub dataset_hash: String,
    pub source_endpoint: String,
    pub environment: String,
    pub count: usize,
    pub actual_start_ms: Option<u64>,
    pub actual_end_ms: Option<u64>,
    pub pages_fetched: u32,
    pub gaps: usize,
    pub pagination_complete: bool,
    pub observed_intervals_ms: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketInputs {
    pub minutes: BTreeMap<u64, Decimal>,
    pub funding: Vec<FundingRecord>,
    pub hourly_marks: BTreeMap<u64, Decimal>,
    pub finer_marks: BTreeMap<u64, Decimal>,
    pub coverage: Vec<CoverageNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportedRatio {
    pub value: Option<Decimal>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YearPnl {
    pub year: i32,
    pub partial: bool,
    pub partial_reason: Option<String>,
    pub segment_start_ms: u64,
    pub segment_end_ms: u64,
    pub net_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingExecution {
    pub decision_id: String,
    pub source_start_ms: Option<u64>,
    pub source_end_ms: Option<u64>,
    pub decision_time_ms: u64,
    pub available_at_ms: u64,
    pub blocked_fill_time_ms: u64,
    pub target_qty: Decimal,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FillRecord {
    pub decision_id: String,
    pub source_start_ms: Option<u64>,
    pub source_end_ms: Option<u64>,
    pub decision_time_ms: u64,
    pub available_at_ms: u64,
    pub fill_time_ms: u64,
    pub side: String,
    pub quantity: Decimal,
    pub reference_open: Decimal,
    pub fill_price: Decimal,
    pub fee: Decimal,
    pub position_before: Decimal,
    pub position_after: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FundingCashflow {
    pub settlement_time_ms: u64,
    pub signed_position: Decimal,
    pub funding_rate: Decimal,
    pub mark_price: Decimal,
    pub mark_time_ms: u64,
    pub mark_method: String,
    pub cashflow: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TradeRecord {
    pub entry_decision_id: String,
    pub exit_decision_id: String,
    pub side: String,
    pub quantity: Decimal,
    pub entry_time_ms: u64,
    pub exit_time_ms: u64,
    pub holding_period_ms: u64,
    pub entry_fill: Decimal,
    pub exit_fill: Decimal,
    pub price_pnl: Decimal,
    pub fees: Decimal,
    pub funding: Decimal,
    pub net_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PnlSample {
    pub observation_time_ms: u64,
    pub mark_time_ms: u64,
    pub mark_price: Decimal,
    pub mark_source: String,
    pub position: Decimal,
    pub realized_price_pnl: Decimal,
    pub unrealized_price_pnl: Decimal,
    pub cumulative_fees: Decimal,
    pub cumulative_funding: Decimal,
    pub cumulative_net_pnl: Decimal,
    pub running_peak: Decimal,
    pub drawdown_usdt: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub book: String,
    pub book_note: String,
    pub result_label: String,
    pub fill_assumption: String,
    pub scenario_name: String,
    pub scenario_hash: String,
    pub signal_config_hash: String,
    pub daily_dataset_hash: String,
    pub target_fingerprint: String,
    pub fee_bps: Decimal,
    pub execution_adjustment_bps: Decimal,
    pub decision_delay_seconds: u64,
    pub decision_delay_ms: u64,
    pub performance_start_ms: u64,
    pub performance_end_exclusive_ms: u64,
    pub coverage: Vec<CoverageNote>,
    pub limitations: Vec<String>,
    pub signal_changes: usize,
    pub fills: usize,
    pub completed_trades: usize,
    pub pending: Vec<PendingExecution>,
    pub model_direction_at_end: String,
    pub simulated_position_at_end: Decimal,
    pub gross_price_pnl: Decimal,
    pub realized_price_pnl: Decimal,
    pub unrealized_price_pnl: Decimal,
    pub fees: Decimal,
    pub funding: Decimal,
    pub funding_paid: Decimal,
    pub funding_received: Decimal,
    pub net_pnl: Decimal,
    pub estimated_close_cost_usdt: Decimal,
    pub estimated_close_cost_note: String,
    pub max_drawdown_usdt: Decimal,
    pub max_drawdown_time_ms: u64,
    pub max_drawdown_peak_time_ms: u64,
    pub drawdown_sampling: String,
    pub time_in_market_ms: u64,
    pub time_in_market_fraction: Decimal,
    pub max_notional_usdt: Decimal,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub breakeven_trades: usize,
    pub win_rate: ReportedRatio,
    pub average_win: ReportedRatio,
    pub average_loss: ReportedRatio,
    pub profit_factor: ReportedRatio,
    pub largest_losing_trade: Option<Decimal>,
    pub long_net_pnl: Decimal,
    pub short_net_pnl: Decimal,
    pub open_mark_to_market: Decimal,
    pub calendar_years: Vec<YearPnl>,
    pub largest_winner_net: Option<Decimal>,
    pub largest_three_winners_net: Option<Decimal>,
    pub largest_winner_share_of_net: ReportedRatio,
    pub largest_three_winners_share_of_net: ReportedRatio,
    pub benchmark_entry_time_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Simulation {
    pub summary: Summary,
    pub fills: Vec<FillRecord>,
    pub funding_rows: Vec<FundingCashflow>,
    pub trades: Vec<TradeRecord>,
    pub pnl: Vec<PnlSample>,
}

pub struct SimulateRequest<'a> {
    pub book: &'a str,
    pub book_note: &'a str,
    pub model_direction_at_end: &'a str,
    pub signal_config_hash: &'a str,
    pub changes: &'a [TargetChange],
    pub scenario: &'a ExecutionScenario,
    pub minutes: &'a BTreeMap<u64, Decimal>,
    pub funding: &'a [FundingRecord],
    pub hourly_marks: &'a BTreeMap<u64, Decimal>,
    pub finer_marks: &'a BTreeMap<u64, Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioPair {
    pub scenario: ExecutionScenario,
    pub donchian: Simulation,
    pub fixed_long: Simulation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub daily_dataset_hash: String,
    pub signal_config_hash: String,
    pub pairs: Vec<ScenarioPair>,
}

pub struct EvaluateRequest<'a> {
    pub decisions: &'a [DecisionRow],
    pub final_direction: &'a str,
    pub signal_config_hash: &'a str,
    pub daily_dataset_hash: &'a str,
    pub position_qty: Decimal,
    pub market: &'a MarketInputs,
}

pub fn load_decision_log(path: &Path) -> Result<DecisionLog, BacktestError> {
    let body = std::fs::read(path)
        .map_err(|err| BacktestError::new(format!("read {}: {err}", path.display())))?;
    serde_json::from_slice(&body)
        .map_err(|err| BacktestError::new(format!("decode {}: {err}", path.display())))
}

pub fn target_changes(decisions: &[DecisionRow]) -> Result<Vec<TargetChange>, BacktestError> {
    let mut requested = Decimal::ZERO;
    let mut changes = Vec::new();
    for row in decisions {
        match &row.target {
            EmittedTarget::NotReady | EmittedTarget::NoChange => {}
            EmittedTarget::Suspended => {
                return Err(BacktestError::new(
                    "suspended decision in the replay; refusing to score performance",
                ));
            }
            EmittedTarget::Quantity { quantity } => {
                if *quantity != requested {
                    changes.push(TargetChange {
                        decision_id: format!("{}-{}", row.start_time_ms, row.end_time_exclusive_ms),
                        source_start_ms: Some(row.start_time_ms),
                        source_end_ms: Some(row.end_time_exclusive_ms),
                        decision_time_ms: row.end_time_exclusive_ms,
                        target_qty: *quantity,
                        reason: row.reason.clone(),
                    });
                    requested = *quantity;
                }
            }
        }
    }
    Ok(changes)
}

pub fn changes_for(
    book: Book,
    decisions: &[DecisionRow],
    performance_start_ms: u64,
    position_qty: Decimal,
) -> Result<Vec<TargetChange>, BacktestError> {
    match book {
        Book::DonchianDailyV1 => {
            let changes = target_changes(decisions)?;
            for change in &changes {
                let size = change.target_qty.abs();
                if size != Decimal::ZERO && size != position_qty {
                    return Err(BacktestError::new(format!(
                        "target {} is not the frozen position size {position_qty}",
                        change.target_qty
                    )));
                }
            }
            Ok(changes)
        }
        Book::FixedLongBenchmark => Ok(vec![TargetChange {
            decision_id: "benchmark-fixed-long".to_string(),
            source_start_ms: None,
            source_end_ms: None,
            decision_time_ms: performance_start_ms,
            target_qty: position_qty,
            reason: "fixed long benchmark; decision_time is the performance window start; there is no signal candle".to_string(),
        }]),
    }
}

pub fn next_minute_start_strictly_after(available_at_ms: u64) -> Result<u64, BacktestError> {
    let next = if available_at_ms.is_multiple_of(MINUTE_MS) {
        available_at_ms.checked_add(MINUTE_MS)
    } else {
        let boundary = available_at_ms - (available_at_ms % MINUTE_MS);
        boundary.checked_add(MINUTE_MS)
    };
    next.ok_or_else(|| BacktestError::new("minute timestamp overflowed"))
}

pub fn required_execution_minutes(
    changes: &[TargetChange],
    scenarios: &[ExecutionScenario],
) -> Result<BTreeSet<u64>, BacktestError> {
    let mut minutes = BTreeSet::new();
    for scenario in scenarios {
        for change in changes {
            if let Some(minute) = execution_minute(change, scenario)? {
                minutes.insert(minute);
            }
        }
    }
    Ok(minutes)
}

pub fn required_execution_minutes_for_experiment(
    decisions: &[DecisionRow],
    performance_start_ms: u64,
    performance_end_exclusive_ms: u64,
    position_qty: Decimal,
) -> Result<BTreeSet<u64>, BacktestError> {
    let strategy = changes_for(
        Book::DonchianDailyV1,
        decisions,
        performance_start_ms,
        position_qty,
    )?;
    required_execution_minutes_for_target_changes(
        &strategy,
        performance_start_ms,
        performance_end_exclusive_ms,
        position_qty,
    )
}

/// Minutes needed for a strategy's target changes plus the fixed-long benchmark.
pub fn required_execution_minutes_for_target_changes(
    strategy_changes: &[TargetChange],
    performance_start_ms: u64,
    performance_end_exclusive_ms: u64,
    position_qty: Decimal,
) -> Result<BTreeSet<u64>, BacktestError> {
    let scenarios = research_scenarios(performance_start_ms, performance_end_exclusive_ms);
    let mut minutes = required_execution_minutes(strategy_changes, &scenarios)?;
    let benchmark = changes_for(
        Book::FixedLongBenchmark,
        &[],
        performance_start_ms,
        position_qty,
    )?;
    minutes.extend(required_execution_minutes(&benchmark, &scenarios)?);
    Ok(minutes)
}

pub fn validate_coverage(
    performance_start_ms: u64,
    performance_end_exclusive_ms: u64,
    required_minutes: &BTreeSet<u64>,
    market: &MarketInputs,
) -> Result<(), BacktestError> {
    ensure_hourly_grid(
        &market.hourly_marks,
        performance_start_ms,
        performance_end_exclusive_ms,
    )?;
    for minute in required_minutes {
        if !market.minutes.contains_key(minute) {
            return Err(BacktestError::new(format!(
                "missing execution observation at {minute}"
            )));
        }
    }
    if market.funding.is_empty() {
        return Err(BacktestError::new(
            "funding coverage is incomplete: no settlements were supplied",
        ));
    }
    let mut funding = market.funding.clone();
    funding.sort_by_key(|row| row.settlement_time_ms);
    for pair in funding.windows(2) {
        if pair[0].settlement_time_ms == pair[1].settlement_time_ms {
            return Err(BacktestError::new(format!(
                "duplicate funding settlement at {}",
                pair[0].settlement_time_ms
            )));
        }
    }
    let first = funding[0].settlement_time_ms;
    let last = funding[funding.len() - 1].settlement_time_ms;
    if first > performance_start_ms || last < performance_end_exclusive_ms {
        return Err(BacktestError::new(format!(
            "funding coverage is incomplete: settlements span {first}..{last}, performance window is {performance_start_ms}..{performance_end_exclusive_ms}"
        )));
    }
    for row in &funding {
        if row.settlement_time_ms < performance_start_ms
            || row.settlement_time_ms >= performance_end_exclusive_ms
        {
            continue;
        }
        settlement_mark(
            &market.hourly_marks,
            &market.finer_marks,
            row.settlement_time_ms,
        )?;
    }
    Ok(())
}

pub fn evaluate_experiment(request: &EvaluateRequest<'_>) -> Result<Evaluation, BacktestError> {
    if request.position_qty != frozen_position_qty() {
        return Err(BacktestError::new(
            "evaluation position size is not the frozen 0.001 BTC",
        ));
    }
    if request.signal_config_hash != FROZEN_CONFIG_HASH {
        return Err(BacktestError::new(
            "evaluation signal hash is not the frozen configuration",
        ));
    }
    let start = FROZEN_EVALUATION_START_MS;
    let end = FROZEN_EVALUATION_END_MS;
    let last = request
        .decisions
        .last()
        .ok_or_else(|| BacktestError::new("decision log has no rows"))?;
    if last.direction != request.final_direction {
        return Err(BacktestError::new(
            "final model direction does not match the last decision",
        ));
    }
    for note in &request.market.coverage {
        if !note.pagination_complete {
            return Err(BacktestError::new(format!(
                "{} pagination is incomplete",
                note.series
            )));
        }
    }
    for series in ["trade_1m", "funding", "mark_60"] {
        if !request
            .market
            .coverage
            .iter()
            .any(|note| note.series == series)
        {
            return Err(BacktestError::new(format!(
                "missing coverage note for {series}"
            )));
        }
    }
    let required = required_execution_minutes_for_experiment(
        request.decisions,
        start,
        end,
        request.position_qty,
    )?;
    validate_coverage(start, end, &required, request.market)?;
    let scenarios = research_scenarios(start, end);
    let mut pairs = Vec::new();
    for scenario in scenarios {
        let donchian_changes = changes_for(
            Book::DonchianDailyV1,
            request.decisions,
            start,
            request.position_qty,
        )?;
        let benchmark_changes = changes_for(
            Book::FixedLongBenchmark,
            request.decisions,
            start,
            request.position_qty,
        )?;
        let mut donchian = simulate(&SimulateRequest {
            book: Book::DonchianDailyV1.as_str(),
            book_note: Book::DonchianDailyV1.note(),
            model_direction_at_end: request.final_direction,
            signal_config_hash: request.signal_config_hash,
            changes: &donchian_changes,
            scenario: &scenario,
            minutes: &request.market.minutes,
            funding: &request.market.funding,
            hourly_marks: &request.market.hourly_marks,
            finer_marks: &request.market.finer_marks,
        })?;
        let mut fixed_long = simulate(&SimulateRequest {
            book: Book::FixedLongBenchmark.as_str(),
            book_note: Book::FixedLongBenchmark.note(),
            model_direction_at_end: "long",
            signal_config_hash: request.signal_config_hash,
            changes: &benchmark_changes,
            scenario: &scenario,
            minutes: &request.market.minutes,
            funding: &request.market.funding,
            hourly_marks: &request.market.hourly_marks,
            finer_marks: &request.market.finer_marks,
        })?;
        for simulation in [&mut donchian, &mut fixed_long] {
            simulation.summary.daily_dataset_hash = request.daily_dataset_hash.to_string();
            simulation.summary.coverage = request.market.coverage.clone();
        }
        pairs.push(ScenarioPair {
            scenario,
            donchian,
            fixed_long,
        });
    }
    Ok(Evaluation {
        daily_dataset_hash: request.daily_dataset_hash.to_string(),
        signal_config_hash: request.signal_config_hash.to_string(),
        pairs,
    })
}

pub fn simulate(request: &SimulateRequest<'_>) -> Result<Simulation, BacktestError> {
    let scenario = request.scenario;
    if scenario.fee_bps < Decimal::ZERO || scenario.execution_adjustment_bps < Decimal::ZERO {
        return Err(BacktestError::new(
            "fee and execution adjustment must be non-negative",
        ));
    }
    ensure_hourly_grid(
        request.hourly_marks,
        scenario.performance_start_ms,
        scenario.performance_end_exclusive_ms,
    )?;
    let (scheduled, pending) = schedule_fills(request.changes, scenario, request.minutes)?;
    let funding_rows = in_window_funding(
        request.funding,
        scenario.performance_start_ms,
        scenario.performance_end_exclusive_ms,
    )?;

    let mut fill_by_time = BTreeMap::new();
    for fill in scheduled {
        if fill_by_time.insert(fill.fill_time_ms, fill).is_some() {
            return Err(BacktestError::new(
                "two fills share a timestamp; refusing to invent an execution order",
            ));
        }
    }
    let mut funding_by_time = BTreeMap::new();
    for row in &funding_rows {
        if funding_by_time
            .insert(row.settlement_time_ms, row.clone())
            .is_some()
        {
            return Err(BacktestError::new(format!(
                "duplicate funding settlement at {}",
                row.settlement_time_ms
            )));
        }
    }

    let mut times = BTreeSet::new();
    let mut cursor = scenario.performance_start_ms;
    loop {
        times.insert(cursor);
        if cursor == scenario.performance_end_exclusive_ms {
            break;
        }
        cursor = cursor
            .checked_add(HOUR_MS)
            .ok_or_else(|| BacktestError::new("hour grid overflowed"))?;
    }
    times.extend(funding_by_time.keys().copied());
    times.extend(fill_by_time.keys().copied());

    let mut book = PositionBook::default();
    let mut funding_cashflows = Vec::new();
    let mut path = MarkPath {
        samples: Vec::new(),
        peak: Peak::new(scenario.performance_start_ms),
    };

    for time in times {
        let mut sampled = false;
        if let Some(row) = funding_by_time.get(&time).cloned() {
            let (mark_price, mark_time, mark_method) =
                settlement_mark(request.hourly_marks, request.finer_marks, time)?;
            let cashflow = -book.account.position * mark_price * row.funding_rate;
            if let Some(open) = book.account.open.as_mut() {
                open.funding += cashflow;
            }
            book.account.funding += cashflow;
            funding_cashflows.push(FundingCashflow {
                settlement_time_ms: time,
                signed_position: book.account.position,
                funding_rate: row.funding_rate,
                mark_price,
                mark_time_ms: mark_time,
                mark_method: mark_method.clone(),
                cashflow,
            });
            push_sample(
                &mut path,
                &mut book,
                time,
                mark_time,
                mark_price,
                &mark_method,
            );
            sampled = true;
        }
        if let Some(scheduled_fill) = fill_by_time.remove(&time) {
            apply_fill(&mut book, &scheduled_fill, scenario)?;
            let (mark_time, mark_price) = mark_at_or_before(request.hourly_marks, time)?;
            let source = if mark_time == time {
                "hourly_mark_open"
            } else {
                "hourly_mark_open_carried_forward"
            };
            push_sample(&mut path, &mut book, time, mark_time, mark_price, source);
            sampled = true;
        }
        if time.is_multiple_of(HOUR_MS) && !sampled {
            let mark_price = *request
                .hourly_marks
                .get(&time)
                .ok_or_else(|| BacktestError::new(format!("missing hourly mark at {time}")))?;
            push_sample(
                &mut path,
                &mut book,
                time,
                time,
                mark_price,
                "hourly_mark_open",
            );
        }
    }
    book.accrue_open_interval(scenario.performance_end_exclusive_ms)?;
    let PositionBook {
        account,
        fills,
        trades,
        held_ms,
        max_notional,
        opened_at: _,
    } = book;
    let MarkPath { samples: pnl, peak } = path;

    let end_mark = *request
        .hourly_marks
        .get(&scenario.performance_end_exclusive_ms)
        .ok_or_else(|| BacktestError::new("missing terminal hourly mark"))?;
    let realized = account.realized;
    let unrealized = unrealized_pnl(&account, end_mark);
    let net = realized + unrealized - account.fees + account.funding;
    let last = pnl
        .last()
        .ok_or_else(|| BacktestError::new("no pnl samples"))?;
    if last.observation_time_ms != scenario.performance_end_exclusive_ms
        || last.cumulative_net_pnl != net
    {
        return Err(BacktestError::new(
            "terminal pnl sample does not match the ledger",
        ));
    }
    let sampled_drawdown = pnl
        .iter()
        .map(|sample| sample.drawdown_usdt)
        .max()
        .unwrap_or(Decimal::ZERO);
    if sampled_drawdown != peak.max_drawdown {
        return Err(BacktestError::new(
            "drawdown tracker does not match the pnl samples",
        ));
    }

    let funding_paid = funding_cashflows
        .iter()
        .filter(|row| row.cashflow < Decimal::ZERO)
        .map(|row| -row.cashflow)
        .sum();
    let funding_received = funding_cashflows
        .iter()
        .filter(|row| row.cashflow > Decimal::ZERO)
        .map(|row| row.cashflow)
        .sum();
    if funding_received - funding_paid != account.funding {
        return Err(BacktestError::new(
            "funding paid and received do not match the ledger",
        ));
    }

    let (long_net, short_net) = side_contribution(&trades)?;
    let open_mark_to_market = match &account.open {
        Some(open) => unrealized - open.entry_fee + open.funding,
        None => Decimal::ZERO,
    };
    if long_net + short_net + open_mark_to_market != net {
        return Err(BacktestError::new(
            "long, short, and open mark-to-market do not sum to net pnl",
        ));
    }
    let years = calendar_years(
        scenario.performance_start_ms,
        scenario.performance_end_exclusive_ms,
        &pnl,
    )?;
    let year_sum: Decimal = years.iter().map(|year| year.net_pnl).sum();
    if year_sum != net {
        return Err(BacktestError::new(format!(
            "calendar-year pnl {year_sum} does not sum to net pnl {net}"
        )));
    }

    let window_ms = scenario.performance_end_exclusive_ms - scenario.performance_start_ms;
    let stats = trade_stats(&trades, net)?;
    let entry_time = if request.book == Book::FixedLongBenchmark.as_str() {
        fills.first().map(|fill| fill.fill_time_ms)
    } else {
        None
    };
    let fingerprint = target_fingerprint(request.changes)?;
    let scenario_hash = scenario_hash(scenario, request.signal_config_hash)?;
    let summary = Summary {
        book: request.book.to_string(),
        book_note: request.book_note.to_string(),
        result_label: RESULT_LABEL.to_string(),
        fill_assumption: FILL_ASSUMPTION.to_string(),
        scenario_name: scenario.name.clone(),
        scenario_hash,
        signal_config_hash: request.signal_config_hash.to_string(),
        daily_dataset_hash: String::new(),
        target_fingerprint: fingerprint,
        fee_bps: scenario.fee_bps,
        execution_adjustment_bps: scenario.execution_adjustment_bps,
        decision_delay_seconds: scenario.decision_delay_ms / 1_000,
        decision_delay_ms: scenario.decision_delay_ms,
        performance_start_ms: scenario.performance_start_ms,
        performance_end_exclusive_ms: scenario.performance_end_exclusive_ms,
        coverage: Vec::new(),
        limitations: standard_limitations(),
        signal_changes: request.changes.len(),
        fills: fills.len(),
        completed_trades: trades.len(),
        pending,
        model_direction_at_end: request.model_direction_at_end.to_string(),
        simulated_position_at_end: account.position,
        gross_price_pnl: realized + unrealized,
        realized_price_pnl: realized,
        unrealized_price_pnl: unrealized,
        fees: account.fees,
        funding: account.funding,
        funding_paid,
        funding_received,
        net_pnl: net,
        estimated_close_cost_usdt: estimated_close_cost(
            account.position,
            end_mark,
            scenario.execution_adjustment_bps,
            scenario.fee_bps,
        ),
        estimated_close_cost_note:
            "Disclosed only. Not included in net PnL. A remaining position is not force-closed."
                .to_string(),
        max_drawdown_usdt: peak.max_drawdown,
        max_drawdown_time_ms: peak.max_drawdown_time_ms,
        max_drawdown_peak_time_ms: peak.max_drawdown_peak_time_ms,
        drawdown_sampling: DRAWDOWN_SAMPLING.to_string(),
        time_in_market_ms: held_ms,
        time_in_market_fraction: Decimal::from(held_ms) / Decimal::from(window_ms),
        max_notional_usdt: max_notional,
        winning_trades: stats.winners,
        losing_trades: stats.losers,
        breakeven_trades: stats.breakeven,
        win_rate: stats.win_rate,
        average_win: stats.average_win,
        average_loss: stats.average_loss,
        profit_factor: stats.profit_factor,
        largest_losing_trade: stats.largest_loss,
        long_net_pnl: long_net,
        short_net_pnl: short_net,
        open_mark_to_market,
        calendar_years: years,
        largest_winner_net: stats.largest_winner,
        largest_three_winners_net: stats.largest_three,
        largest_winner_share_of_net: stats.winner_share,
        largest_three_winners_share_of_net: stats.three_share,
        benchmark_entry_time_ms: entry_time,
    };
    Ok(Simulation {
        summary,
        fills,
        funding_rows: funding_cashflows,
        trades,
        pnl,
    })
}

pub fn render_report(simulation: &Simulation) -> String {
    let summary = &simulation.summary;
    let mut out = String::new();
    let _ = writeln!(out, "# {} — {}", summary.book, summary.scenario_name);
    let _ = writeln!(out);
    let _ = writeln!(out, "**{}**", summary.result_label);
    let _ = writeln!(out);
    let _ = writeln!(out, "**Fill assumption.** {}", summary.fill_assumption);
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", summary.book_note);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Configuration");
    let _ = writeln!(out, "- scenario hash: `{}`", summary.scenario_hash);
    let _ = writeln!(
        out,
        "- signal config hash: `{}`",
        summary.signal_config_hash
    );
    let _ = writeln!(
        out,
        "- daily dataset hash: `{}`",
        summary.daily_dataset_hash
    );
    let _ = writeln!(
        out,
        "- target fingerprint: `{}`",
        summary.target_fingerprint
    );
    let _ = writeln!(out, "- fee_bps: {} (synthetic)", summary.fee_bps);
    let _ = writeln!(
        out,
        "- execution_adjustment_bps: {} (synthetic combined spread/slippage; already inside fill prices)",
        summary.execution_adjustment_bps
    );
    let _ = writeln!(
        out,
        "- decision_delay_seconds: {} (synthetic)",
        summary.decision_delay_seconds
    );
    let _ = writeln!(
        out,
        "- performance window: {} inclusive start, {} exclusive end",
        format_ms(summary.performance_start_ms),
        format_ms(summary.performance_end_exclusive_ms)
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Coverage");
    if summary.coverage.is_empty() {
        let _ = writeln!(out, "No auxiliary coverage notes on this run.");
    }
    for note in &summary.coverage {
        let _ = writeln!(
            out,
            "- {} hash `{}`, endpoint `{}`, environment {}, count {}, pages {}, gaps {}, pagination_complete {}, span {} .. {}",
            note.series,
            note.dataset_hash,
            note.source_endpoint,
            note.environment,
            note.count,
            note.pages_fetched,
            note.gaps,
            note.pagination_complete,
            note.actual_start_ms.map(format_ms).unwrap_or_else(|| "none".to_string()),
            note.actual_end_ms.map(format_ms).unwrap_or_else(|| "none".to_string()),
        );
        if !note.observed_intervals_ms.is_empty() {
            let _ = writeln!(
                out,
                "  observed intervals ms: {:?}",
                note.observed_intervals_ms
            );
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Activity");
    let _ = writeln!(out, "- signal changes: {}", summary.signal_changes);
    let _ = writeln!(out, "- fills: {}", summary.fills);
    let _ = writeln!(out, "- completed trades: {}", summary.completed_trades);
    let _ = writeln!(out, "- pending executions: {}", summary.pending.len());
    let _ = writeln!(
        out,
        "- model direction at end: {}",
        summary.model_direction_at_end
    );
    let _ = writeln!(
        out,
        "- simulated position at end: {}",
        summary.simulated_position_at_end
    );
    for pending in &summary.pending {
        let _ = writeln!(
            out,
            "- pending {} target {} available {} blocked fill minute {}: {}",
            pending.decision_id,
            pending.target_qty,
            format_ms(pending.available_at_ms),
            format_ms(pending.blocked_fill_time_ms),
            pending.reason
        );
    }
    if let Some(entry) = summary.benchmark_entry_time_ms {
        let _ = writeln!(out, "- benchmark entry fill time: {}", format_ms(entry));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## PnL (USDT)");
    let _ = writeln!(out, "- gross price PnL: {}", summary.gross_price_pnl);
    let _ = writeln!(out, "- realized price PnL: {}", summary.realized_price_pnl);
    let _ = writeln!(
        out,
        "- unrealized price PnL: {}",
        summary.unrealized_price_pnl
    );
    let _ = writeln!(out, "- fees: {}", summary.fees);
    let _ = writeln!(out, "- funding paid: {}", summary.funding_paid);
    let _ = writeln!(out, "- funding received: {}", summary.funding_received);
    let _ = writeln!(out, "- signed funding: {}", summary.funding);
    let _ = writeln!(out, "- net PnL: {}", summary.net_pnl);
    let _ = writeln!(
        out,
        "- estimated close cost (not in net PnL): {}",
        summary.estimated_close_cost_usdt
    );
    let _ = writeln!(out, "- {}", summary.estimated_close_cost_note);
    let _ = writeln!(
        out,
        "- max sampled drawdown: {} USDT, trough {}, peak {}",
        summary.max_drawdown_usdt,
        format_ms(summary.max_drawdown_time_ms),
        format_ms(summary.max_drawdown_peak_time_ms)
    );
    let _ = writeln!(out, "- drawdown sampling: {}", summary.drawdown_sampling);
    let _ = writeln!(
        out,
        "- time in market: {} ms ({})",
        summary.time_in_market_ms, summary.time_in_market_fraction
    );
    let _ = writeln!(
        out,
        "- maximum marked or fill notional: {} USDT",
        summary.max_notional_usdt
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Trades");
    let _ = writeln!(
        out,
        "- winning / losing / breakeven: {} / {} / {}",
        summary.winning_trades, summary.losing_trades, summary.breakeven_trades
    );
    let _ = writeln!(out, "- win rate: {}", format_ratio(&summary.win_rate));
    let _ = writeln!(out, "- average win: {}", format_ratio(&summary.average_win));
    let _ = writeln!(
        out,
        "- average loss: {}",
        format_ratio(&summary.average_loss)
    );
    let _ = writeln!(
        out,
        "- profit factor: {}",
        format_ratio(&summary.profit_factor)
    );
    let _ = writeln!(
        out,
        "- largest losing trade: {}",
        summary
            .largest_losing_trade
            .map(|value| value.to_string())
            .unwrap_or_else(|| "undefined_no_losing_trades".to_string())
    );
    let _ = writeln!(out, "- long net PnL: {}", summary.long_net_pnl);
    let _ = writeln!(out, "- short net PnL: {}", summary.short_net_pnl);
    let _ = writeln!(
        out,
        "- open mark-to-market residual: {}",
        summary.open_mark_to_market
    );
    let _ = writeln!(
        out,
        "- largest winning trade: {}",
        summary
            .largest_winner_net
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    let _ = writeln!(
        out,
        "- largest three winning trades: {}",
        summary
            .largest_three_winners_net
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    let _ = writeln!(
        out,
        "- largest winner share of net: {}",
        format_ratio(&summary.largest_winner_share_of_net)
    );
    let _ = writeln!(
        out,
        "- largest three winners share of net: {}",
        format_ratio(&summary.largest_three_winners_share_of_net)
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Calendar-year PnL");
    let _ = writeln!(
        out,
        "Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL."
    );
    for year in &summary.calendar_years {
        let label = if year.partial { "partial" } else { "full" };
        let _ = writeln!(
            out,
            "- {} ({label}): {} USDT, {} .. {}",
            year.year,
            year.net_pnl,
            format_ms(year.segment_start_ms),
            format_ms(year.segment_end_ms)
        );
        if let Some(reason) = &year.partial_reason {
            let _ = writeln!(out, "  {reason}");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Limitations");
    for limitation in &summary.limitations {
        let _ = writeln!(out, "- {limitation}");
    }
    out
}

pub fn render_comparison(evaluation: &Evaluation) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# DonchianDailyV1 compared with a fixed +0.001 BTC perpetual long"
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "**{RESULT_LABEL}**");
    let _ = writeln!(out);
    let _ = writeln!(out, "**{FILL_ASSUMPTION}**");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Both books use the same historical funding, fees, execution adjustment, delay, marks, and terminal treatment. The fixed long is not a spot-price return and does not change the Donchian rules. No percentage account return is reported because no capital is defined."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| scenario | book | net PnL USDT | max drawdown USDT | drawdown trough | time in market ms | max notional USDT | fees | funding | ending position | entry |"
    );
    let _ = writeln!(
        out,
        "| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |"
    );
    for pair in &evaluation.pairs {
        for simulation in [&pair.donchian, &pair.fixed_long] {
            let summary = &simulation.summary;
            let entry = summary
                .benchmark_entry_time_ms
                .map(format_ms)
                .unwrap_or_else(|| "-".to_string());
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                summary.scenario_name,
                summary.book,
                summary.net_pnl,
                summary.max_drawdown_usdt,
                format_ms(summary.max_drawdown_time_ms),
                summary.time_in_market_ms,
                summary.max_notional_usdt,
                summary.fees,
                summary.funding,
                summary.simulated_position_at_end,
                entry
            );
        }
    }
    out
}

pub fn write_simulation(dir: &Path, simulation: &Simulation) -> Result<(), BacktestError> {
    std::fs::create_dir_all(dir)
        .map_err(|err| BacktestError::new(format!("create {}: {err}", dir.display())))?;
    write_fills(&dir.join("fills.csv"), &simulation.fills)?;
    write_funding(&dir.join("funding.csv"), &simulation.funding_rows)?;
    write_trades(&dir.join("trades.csv"), &simulation.trades)?;
    write_pnl(&dir.join("pnl.csv"), &simulation.pnl)?;
    let summary = serde_json::to_vec_pretty(&simulation.summary)
        .map_err(|err| BacktestError::new(format!("encode summary: {err}")))?;
    std::fs::write(dir.join("summary.json"), summary)
        .map_err(|err| BacktestError::new(format!("write summary: {err}")))?;
    std::fs::write(dir.join("report.md"), render_report(simulation))
        .map_err(|err| BacktestError::new(format!("write report: {err}")))?;
    Ok(())
}

pub fn write_comparison(dir: &Path, evaluation: &Evaluation) -> Result<(), BacktestError> {
    std::fs::create_dir_all(dir)
        .map_err(|err| BacktestError::new(format!("create {}: {err}", dir.display())))?;
    #[derive(Serialize)]
    struct Row<'a> {
        scenario: &'a str,
        scenario_hash: &'a str,
        book: &'a str,
        net_pnl: Decimal,
        max_drawdown_usdt: Decimal,
        max_drawdown_time_ms: u64,
        time_in_market_ms: u64,
        max_notional_usdt: Decimal,
        fees: Decimal,
        funding: Decimal,
        ending_position: Decimal,
        benchmark_entry_time_ms: Option<u64>,
        result_label: &'a str,
        fill_assumption: &'a str,
    }
    let mut rows = Vec::new();
    for pair in &evaluation.pairs {
        for simulation in [&pair.donchian, &pair.fixed_long] {
            let summary = &simulation.summary;
            rows.push(Row {
                scenario: &summary.scenario_name,
                scenario_hash: &summary.scenario_hash,
                book: &summary.book,
                net_pnl: summary.net_pnl,
                max_drawdown_usdt: summary.max_drawdown_usdt,
                max_drawdown_time_ms: summary.max_drawdown_time_ms,
                time_in_market_ms: summary.time_in_market_ms,
                max_notional_usdt: summary.max_notional_usdt,
                fees: summary.fees,
                funding: summary.funding,
                ending_position: summary.simulated_position_at_end,
                benchmark_entry_time_ms: summary.benchmark_entry_time_ms,
                result_label: RESULT_LABEL,
                fill_assumption: FILL_ASSUMPTION,
            });
        }
    }
    let body = serde_json::to_vec_pretty(&rows)
        .map_err(|err| BacktestError::new(format!("encode comparison: {err}")))?;
    std::fs::write(dir.join("comparison.json"), body)
        .map_err(|err| BacktestError::new(format!("write comparison: {err}")))?;
    std::fs::write(dir.join("comparison.md"), render_comparison(evaluation))
        .map_err(|err| BacktestError::new(format!("write comparison report: {err}")))?;
    Ok(())
}

struct ScheduledFill {
    change: TargetChange,
    available_at_ms: u64,
    fill_time_ms: u64,
    reference_open: Decimal,
}

struct OpenTrade {
    entry_decision_id: String,
    side: String,
    quantity: Decimal,
    entry_time_ms: u64,
    entry_fill: Decimal,
    entry_fee: Decimal,
    funding: Decimal,
}

#[derive(Default)]
struct Account {
    position: Decimal,
    open: Option<OpenTrade>,
    realized: Decimal,
    fees: Decimal,
    funding: Decimal,
}

#[derive(Default)]
struct PositionBook {
    account: Account,
    fills: Vec<FillRecord>,
    trades: Vec<TradeRecord>,
    opened_at: Option<u64>,
    held_ms: u64,
    max_notional: Decimal,
}

struct MarkPath {
    samples: Vec<PnlSample>,
    peak: Peak,
}

impl PositionBook {
    fn accrue_open_interval(&mut self, end_ms: u64) -> Result<(), BacktestError> {
        let Some(opened) = self.opened_at else {
            return Ok(());
        };
        let elapsed = end_ms
            .checked_sub(opened)
            .ok_or_else(|| BacktestError::new("holding period underflowed"))?;
        self.held_ms = self
            .held_ms
            .checked_add(elapsed)
            .ok_or_else(|| BacktestError::new("holding period overflowed"))?;
        Ok(())
    }
}

struct Peak {
    peak: Decimal,
    peak_time_ms: u64,
    max_drawdown: Decimal,
    max_drawdown_time_ms: u64,
    max_drawdown_peak_time_ms: u64,
}

impl Peak {
    fn new(start_ms: u64) -> Self {
        Self {
            peak: Decimal::ZERO,
            peak_time_ms: start_ms,
            max_drawdown: Decimal::ZERO,
            max_drawdown_time_ms: start_ms,
            max_drawdown_peak_time_ms: start_ms,
        }
    }

    fn observe(&mut self, time_ms: u64, net: Decimal) -> (Decimal, Decimal) {
        if net > self.peak {
            self.peak = net;
            self.peak_time_ms = time_ms;
        }
        let drawdown = self.peak - net;
        if drawdown > self.max_drawdown {
            self.max_drawdown = drawdown;
            self.max_drawdown_time_ms = time_ms;
            self.max_drawdown_peak_time_ms = self.peak_time_ms;
        }
        (self.peak, drawdown)
    }
}

struct TradeStats {
    winners: usize,
    losers: usize,
    breakeven: usize,
    win_rate: ReportedRatio,
    average_win: ReportedRatio,
    average_loss: ReportedRatio,
    profit_factor: ReportedRatio,
    largest_loss: Option<Decimal>,
    largest_winner: Option<Decimal>,
    largest_three: Option<Decimal>,
    winner_share: ReportedRatio,
    three_share: ReportedRatio,
}

fn schedule_fills(
    changes: &[TargetChange],
    scenario: &ExecutionScenario,
    minutes: &BTreeMap<u64, Decimal>,
) -> Result<(Vec<ScheduledFill>, Vec<PendingExecution>), BacktestError> {
    let mut scheduled = Vec::new();
    let mut pending = Vec::new();
    let mut saw_pending = false;
    for (index, change) in changes.iter().enumerate() {
        if saw_pending {
            return Err(BacktestError::new(
                "pending execution overlaps a later decision",
            ));
        }
        if change.decision_time_ms < scenario.performance_start_ms {
            return Err(BacktestError::new(
                "target change is before the performance window",
            ));
        }
        if change.decision_time_ms > scenario.performance_end_exclusive_ms {
            return Err(BacktestError::new(
                "target change is after the performance window",
            ));
        }
        let available_at_ms = change
            .decision_time_ms
            .checked_add(scenario.decision_delay_ms)
            .ok_or_else(|| BacktestError::new("availability timestamp overflowed"))?;
        let fill_time_ms = next_minute_start_strictly_after(available_at_ms)?;
        let outside = change.decision_time_ms == scenario.performance_end_exclusive_ms
            || available_at_ms >= scenario.performance_end_exclusive_ms
            || fill_time_ms >= scenario.performance_end_exclusive_ms;
        if outside {
            pending.push(PendingExecution {
                decision_id: change.decision_id.clone(),
                source_start_ms: change.source_start_ms,
                source_end_ms: change.source_end_ms,
                decision_time_ms: change.decision_time_ms,
                available_at_ms,
                blocked_fill_time_ms: fill_time_ms,
                target_qty: change.target_qty,
                reason: "available_at or the next one-minute open is outside the performance window; no fill was invented".to_string(),
            });
            saw_pending = true;
            continue;
        }
        if let Some(next) = changes.get(index + 1) {
            if fill_time_ms >= next.decision_time_ms {
                return Err(BacktestError::new(
                    "fill would overlap the next decision; refusing to invent an execution order",
                ));
            }
        }
        let reference_open = *minutes.get(&fill_time_ms).ok_or_else(|| {
            BacktestError::new(format!("missing execution observation at {fill_time_ms}"))
        })?;
        scheduled.push(ScheduledFill {
            change: change.clone(),
            available_at_ms,
            fill_time_ms,
            reference_open,
        });
    }
    Ok((scheduled, pending))
}

fn apply_fill(
    book: &mut PositionBook,
    scheduled: &ScheduledFill,
    scenario: &ExecutionScenario,
) -> Result<(), BacktestError> {
    let before = book.account.position;
    let delta = scheduled.change.target_qty - before;
    if delta == Decimal::ZERO {
        return Err(BacktestError::new(
            "scheduled target does not change the simulated position",
        ));
    }
    let after = before + delta;
    if before != Decimal::ZERO && after != Decimal::ZERO {
        return Err(BacktestError::new(
            "fill is not a full open or full close; this ledger does not model reversals or adds",
        ));
    }
    let buy = delta > Decimal::ZERO;
    let fill_price = adjusted_fill(
        scheduled.reference_open,
        buy,
        scenario.execution_adjustment_bps,
    );
    let fee = delta.abs() * fill_price * scenario.fee_bps / Decimal::from(10_000);
    book.account.fees += fee;
    let side = if buy { "buy" } else { "sell" };
    book.fills.push(FillRecord {
        decision_id: scheduled.change.decision_id.clone(),
        source_start_ms: scheduled.change.source_start_ms,
        source_end_ms: scheduled.change.source_end_ms,
        decision_time_ms: scheduled.change.decision_time_ms,
        available_at_ms: scheduled.available_at_ms,
        fill_time_ms: scheduled.fill_time_ms,
        side: side.to_string(),
        quantity: delta,
        reference_open: scheduled.reference_open,
        fill_price,
        fee,
        position_before: before,
        position_after: after,
    });
    let fill_notional = after.abs() * fill_price;
    if fill_notional > book.max_notional {
        book.max_notional = fill_notional;
    }
    if before == Decimal::ZERO {
        book.account.open = Some(OpenTrade {
            entry_decision_id: scheduled.change.decision_id.clone(),
            side: if after > Decimal::ZERO {
                "long"
            } else {
                "short"
            }
            .to_string(),
            quantity: after,
            entry_time_ms: scheduled.fill_time_ms,
            entry_fill: fill_price,
            entry_fee: fee,
            funding: Decimal::ZERO,
        });
        book.account.position = after;
        book.opened_at = Some(scheduled.fill_time_ms);
        return Ok(());
    }
    let open = book
        .account
        .open
        .take()
        .ok_or_else(|| BacktestError::new("close without an open trade"))?;
    let price_pnl = open.quantity * (fill_price - open.entry_fill);
    book.account.realized += price_pnl;
    let trade_fees = open.entry_fee + fee;
    book.trades.push(TradeRecord {
        entry_decision_id: open.entry_decision_id,
        exit_decision_id: scheduled.change.decision_id.clone(),
        side: open.side,
        quantity: open.quantity,
        entry_time_ms: open.entry_time_ms,
        exit_time_ms: scheduled.fill_time_ms,
        holding_period_ms: scheduled
            .fill_time_ms
            .checked_sub(open.entry_time_ms)
            .ok_or_else(|| BacktestError::new("exit is before entry"))?,
        entry_fill: open.entry_fill,
        exit_fill: fill_price,
        price_pnl,
        fees: trade_fees,
        funding: open.funding,
        net_pnl: price_pnl - trade_fees + open.funding,
    });
    book.account.position = Decimal::ZERO;
    if let Some(opened) = book.opened_at.take() {
        book.held_ms += scheduled
            .fill_time_ms
            .checked_sub(opened)
            .ok_or_else(|| BacktestError::new("holding period underflowed"))?;
    }
    Ok(())
}

fn execution_minute(
    change: &TargetChange,
    scenario: &ExecutionScenario,
) -> Result<Option<u64>, BacktestError> {
    if change.decision_time_ms < scenario.performance_start_ms
        || change.decision_time_ms > scenario.performance_end_exclusive_ms
    {
        return Ok(None);
    }
    let available = change
        .decision_time_ms
        .checked_add(scenario.decision_delay_ms)
        .ok_or_else(|| BacktestError::new("availability timestamp overflowed"))?;
    let minute = next_minute_start_strictly_after(available)?;
    if change.decision_time_ms == scenario.performance_end_exclusive_ms
        || available >= scenario.performance_end_exclusive_ms
        || minute >= scenario.performance_end_exclusive_ms
    {
        return Ok(None);
    }
    Ok(Some(minute))
}

fn in_window_funding(
    funding: &[FundingRecord],
    start: u64,
    end: u64,
) -> Result<Vec<FundingRecord>, BacktestError> {
    let mut rows: Vec<_> = funding
        .iter()
        .filter(|row| row.settlement_time_ms >= start && row.settlement_time_ms < end)
        .cloned()
        .collect();
    rows.sort_by_key(|row| row.settlement_time_ms);
    for pair in rows.windows(2) {
        if pair[0].settlement_time_ms == pair[1].settlement_time_ms {
            return Err(BacktestError::new(format!(
                "duplicate funding settlement at {}",
                pair[0].settlement_time_ms
            )));
        }
    }
    Ok(rows)
}

fn ensure_hourly_grid(
    marks: &BTreeMap<u64, Decimal>,
    start: u64,
    end: u64,
) -> Result<(), BacktestError> {
    if !start.is_multiple_of(HOUR_MS) || !end.is_multiple_of(HOUR_MS) {
        return Err(BacktestError::new(
            "performance window must align to hourly marks",
        ));
    }
    if start >= end {
        return Err(BacktestError::new("performance window is empty"));
    }
    let mut cursor = start;
    loop {
        if !marks.contains_key(&cursor) {
            return Err(BacktestError::new(format!(
                "missing hourly mark at {cursor}"
            )));
        }
        if cursor == end {
            break;
        }
        cursor = cursor
            .checked_add(HOUR_MS)
            .ok_or_else(|| BacktestError::new("hour grid overflowed"))?;
    }
    Ok(())
}

fn settlement_mark(
    hourly: &BTreeMap<u64, Decimal>,
    finer: &BTreeMap<u64, Decimal>,
    timestamp_ms: u64,
) -> Result<(Decimal, u64, String), BacktestError> {
    if timestamp_ms.is_multiple_of(HOUR_MS) {
        return match hourly.get(&timestamp_ms) {
            Some(price) => Ok((
                *price,
                timestamp_ms,
                "hourly_mark_open_at_settlement".to_string(),
            )),
            None => Err(BacktestError::new(format!(
                "missing mark valuation at funding timestamp {timestamp_ms}"
            ))),
        };
    }
    match finer.get(&timestamp_ms) {
        Some(price) => Ok((*price, timestamp_ms, "finer_mark_at_settlement".to_string())),
        None => Err(BacktestError::new(format!(
            "missing mark valuation at funding timestamp {timestamp_ms}"
        ))),
    }
}

fn mark_at_or_before(
    marks: &BTreeMap<u64, Decimal>,
    timestamp_ms: u64,
) -> Result<(u64, Decimal), BacktestError> {
    match marks.range(..=timestamp_ms).next_back() {
        Some((time, price)) => Ok((*time, *price)),
        None => Err(BacktestError::new(format!(
            "missing mark at or before {timestamp_ms}"
        ))),
    }
}

fn adjusted_fill(reference_open: Decimal, buy: bool, adjustment_bps: Decimal) -> Decimal {
    let ratio = adjustment_bps / Decimal::from(10_000);
    if buy {
        reference_open * (Decimal::ONE + ratio)
    } else {
        reference_open * (Decimal::ONE - ratio)
    }
}

fn estimated_close_cost(
    position: Decimal,
    mark: Decimal,
    adjustment_bps: Decimal,
    fee_bps: Decimal,
) -> Decimal {
    if position == Decimal::ZERO {
        return Decimal::ZERO;
    }
    let close_price = adjusted_fill(mark, position < Decimal::ZERO, adjustment_bps);
    let adjustment = (close_price - mark).abs() * position.abs();
    let fee = position.abs() * close_price * fee_bps / Decimal::from(10_000);
    adjustment + fee
}

fn unrealized_pnl(account: &Account, mark: Decimal) -> Decimal {
    match &account.open {
        Some(open) => account.position * (mark - open.entry_fill),
        None => Decimal::ZERO,
    }
}

fn push_sample(
    path: &mut MarkPath,
    book: &mut PositionBook,
    observation_time_ms: u64,
    mark_time_ms: u64,
    mark_price: Decimal,
    mark_source: &str,
) {
    let unrealized = unrealized_pnl(&book.account, mark_price);
    let realized = book.account.realized;
    let fees = book.account.fees;
    let funding = book.account.funding;
    let position = book.account.position;
    let net = realized + unrealized - fees + funding;
    let (running_peak, drawdown) = path.peak.observe(observation_time_ms, net);
    if position != Decimal::ZERO {
        let notional = position.abs() * mark_price;
        if notional > book.max_notional {
            book.max_notional = notional;
        }
    }
    path.samples.push(PnlSample {
        observation_time_ms,
        mark_time_ms,
        mark_price,
        mark_source: mark_source.to_string(),
        position,
        realized_price_pnl: realized,
        unrealized_price_pnl: unrealized,
        cumulative_fees: fees,
        cumulative_funding: funding,
        cumulative_net_pnl: net,
        running_peak,
        drawdown_usdt: drawdown,
    });
}

fn side_contribution(trades: &[TradeRecord]) -> Result<(Decimal, Decimal), BacktestError> {
    let mut long_net = Decimal::ZERO;
    let mut short_net = Decimal::ZERO;
    for trade in trades {
        match trade.side.as_str() {
            "long" => long_net += trade.net_pnl,
            "short" => short_net += trade.net_pnl,
            _ => {
                return Err(BacktestError::new(format!(
                    "unknown trade side {}",
                    trade.side
                )))
            }
        }
    }
    Ok((long_net, short_net))
}

fn trade_stats(trades: &[TradeRecord], net: Decimal) -> Result<TradeStats, BacktestError> {
    let mut winning = Vec::new();
    let mut losing = Vec::new();
    let mut breakeven = 0usize;
    for trade in trades {
        if trade.net_pnl > Decimal::ZERO {
            winning.push(trade.net_pnl);
        } else if trade.net_pnl < Decimal::ZERO {
            losing.push(trade.net_pnl);
        } else {
            breakeven += 1;
        }
    }
    winning.sort_by_key(|value| std::cmp::Reverse(*value));
    let win_rate = if trades.is_empty() {
        undefined("undefined_no_trades")
    } else {
        defined(Decimal::from(winning.len()) / Decimal::from(trades.len()))
    };
    let average_win = if winning.is_empty() {
        undefined("undefined_no_winning_trades")
    } else {
        defined(winning.iter().copied().sum::<Decimal>() / Decimal::from(winning.len()))
    };
    let average_loss = if losing.is_empty() {
        undefined("undefined_no_losing_trades")
    } else {
        defined(losing.iter().copied().sum::<Decimal>() / Decimal::from(losing.len()))
    };
    let profit_factor = if trades.is_empty() {
        undefined("undefined_no_trades")
    } else if losing.is_empty() {
        undefined("undefined_no_losing_trades")
    } else if winning.is_empty() {
        defined(Decimal::ZERO)
    } else {
        let gross_loss = losing.iter().copied().sum::<Decimal>().abs();
        defined(winning.iter().copied().sum::<Decimal>() / gross_loss)
    };
    let largest_winner = winning.first().copied();
    let largest_three = if winning.is_empty() {
        None
    } else {
        Some(winning.iter().take(3).copied().sum())
    };
    let winner_share = share_of_net(largest_winner, net, winning.is_empty())?;
    let three_share = share_of_net(largest_three, net, winning.is_empty())?;
    Ok(TradeStats {
        winners: winning.len(),
        losers: losing.len(),
        breakeven,
        win_rate,
        average_win,
        average_loss,
        profit_factor,
        largest_loss: losing.iter().copied().min(),
        largest_winner,
        largest_three,
        winner_share,
        three_share,
    })
}

fn share_of_net(
    part: Option<Decimal>,
    net: Decimal,
    no_winners: bool,
) -> Result<ReportedRatio, BacktestError> {
    if no_winners {
        return Ok(undefined("undefined_no_winning_trades"));
    }
    if net == Decimal::ZERO {
        return Ok(undefined("undefined_zero_net_pnl"));
    }
    let part = part.ok_or_else(|| BacktestError::new("missing winner contribution"))?;
    Ok(defined(part / net))
}

fn calendar_years(
    start: u64,
    end: u64,
    samples: &[PnlSample],
) -> Result<Vec<YearPnl>, BacktestError> {
    let start_year = year_of(start)?;
    let end_year = year_of(end - 1)?;
    let mut years = Vec::new();
    for year in start_year..=end_year {
        let year_start = jan1(year)?;
        let next_year = jan1(year + 1)?;
        let segment_start = start.max(year_start);
        let segment_end = end.min(next_year);
        let end_cum = if segment_end == end {
            cum_inclusive(samples, segment_end)
        } else {
            cum_exclusive(samples, segment_end)
        };
        let net_pnl = end_cum - cum_exclusive(samples, segment_start);
        let partial = segment_start != year_start || segment_end != next_year;
        years.push(YearPnl {
            year,
            partial,
            partial_reason: partial.then(|| {
                format!(
                    "coverage is {} through {}, not the full calendar year",
                    format_ms(segment_start),
                    format_ms(segment_end)
                )
            }),
            segment_start_ms: segment_start,
            segment_end_ms: segment_end,
            net_pnl,
        });
    }
    Ok(years)
}

fn cum_inclusive(samples: &[PnlSample], timestamp_ms: u64) -> Decimal {
    samples
        .iter()
        .rev()
        .find(|sample| sample.observation_time_ms <= timestamp_ms)
        .map(|sample| sample.cumulative_net_pnl)
        .unwrap_or(Decimal::ZERO)
}

fn cum_exclusive(samples: &[PnlSample], timestamp_ms: u64) -> Decimal {
    samples
        .iter()
        .rev()
        .find(|sample| sample.observation_time_ms < timestamp_ms)
        .map(|sample| sample.cumulative_net_pnl)
        .unwrap_or(Decimal::ZERO)
}

fn defined(value: Decimal) -> ReportedRatio {
    ReportedRatio {
        value: Some(value),
        status: "defined".to_string(),
    }
}

fn undefined(status: &str) -> ReportedRatio {
    ReportedRatio {
        value: None,
        status: status.to_string(),
    }
}

fn standard_limitations() -> Vec<String> {
    vec![
        FILL_ASSUMPTION.to_string(),
        RESULT_LABEL.to_string(),
        "No capital, borrowing, margin, or liquidation model. Figures are futures PnL in USDT, not percentage account returns and not validated leveraged performance.".to_string(),
        "The execution-price adjustment is included in the fill price and is not subtracted again.".to_string(),
        "Equal timestamps apply funding to the pre-fill position, then the fill, then the mark sample. A fill does not pay a settlement timestamped before it.".to_string(),
        "An hourly funding settlement uses that hourly mark candle's open. This is the labelled settlement-mark proxy hourly_mark_open_at_settlement, not a trade print.".to_string(),
        "A position still open at the cutoff is marked to market. estimated_close_cost_usdt is disclosed and is not included in net PnL. The window is not extended to finish the trade.".to_string(),
        "No-change keeps the existing target and does not flatten or submit another entry.".to_string(),
        "The fixed long comparison uses the same perpetual, funding, and execution assumptions. It is not an unrelated spot return.".to_string(),
    ]
}

fn target_fingerprint(changes: &[TargetChange]) -> Result<String, BacktestError> {
    #[derive(Serialize)]
    struct Row<'a> {
        decision_id: &'a str,
        decision_time_ms: u64,
        target_qty: Decimal,
    }
    let rows: Vec<_> = changes
        .iter()
        .map(|change| Row {
            decision_id: &change.decision_id,
            decision_time_ms: change.decision_time_ms,
            target_qty: change.target_qty,
        })
        .collect();
    let bytes = serde_json::to_vec(&rows)
        .map_err(|err| BacktestError::new(format!("fingerprint: {err}")))?;
    Ok(sha256_prefixed(&bytes))
}

fn scenario_hash(
    scenario: &ExecutionScenario,
    signal_config_hash: &str,
) -> Result<String, BacktestError> {
    #[derive(Serialize)]
    struct Payload<'a> {
        fee_bps: Decimal,
        execution_adjustment_bps: Decimal,
        decision_delay_ms: u64,
        performance_start_ms: u64,
        performance_end_exclusive_ms: u64,
        fill_assumption: &'a str,
        result_label: &'a str,
        signal_config_hash: &'a str,
    }
    let payload = Payload {
        fee_bps: scenario.fee_bps,
        execution_adjustment_bps: scenario.execution_adjustment_bps,
        decision_delay_ms: scenario.decision_delay_ms,
        performance_start_ms: scenario.performance_start_ms,
        performance_end_exclusive_ms: scenario.performance_end_exclusive_ms,
        fill_assumption: FILL_ASSUMPTION,
        result_label: RESULT_LABEL,
        signal_config_hash,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| BacktestError::new(format!("scenario hash: {err}")))?;
    Ok(sha256_prefixed(&bytes))
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn year_of(millis: u64) -> Result<i32, BacktestError> {
    Utc.timestamp_millis_opt(millis as i64)
        .single()
        .map(|time| time.year())
        .ok_or_else(|| BacktestError::new(format!("invalid timestamp {millis}")))
}

fn jan1(year: i32) -> Result<u64, BacktestError> {
    let millis = Utc
        .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| BacktestError::new(format!("invalid year {year}")))?
        .timestamp_millis();
    u64::try_from(millis).map_err(|_| BacktestError::new("year is before the unix epoch"))
}

fn format_ms(millis: u64) -> String {
    Utc.timestamp_millis_opt(millis as i64)
        .single()
        .map(|time| time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| millis.to_string())
}

fn format_ratio(ratio: &ReportedRatio) -> String {
    match (&ratio.value, ratio.status.as_str()) {
        (Some(value), "defined") => value.to_string(),
        (_, status) => status.to_string(),
    }
}

fn write_fills(path: &Path, rows: &[FillRecord]) -> Result<(), BacktestError> {
    let mut out = String::from("decision_id,source_start_ms,source_end_ms,decision_time_ms,available_at_ms,fill_time_ms,side,quantity,reference_open,fill_price,fee,position_before,position_after\n");
    for row in rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            row.decision_id,
            optional_ms(row.source_start_ms),
            optional_ms(row.source_end_ms),
            row.decision_time_ms,
            row.available_at_ms,
            row.fill_time_ms,
            row.side,
            row.quantity,
            row.reference_open,
            row.fill_price,
            row.fee,
            row.position_before,
            row.position_after
        );
    }
    std::fs::write(path, out).map_err(|err| BacktestError::new(format!("write fills: {err}")))
}

fn write_funding(path: &Path, rows: &[FundingCashflow]) -> Result<(), BacktestError> {
    let mut out = String::from(
        "settlement_time_ms,signed_position,funding_rate,mark_price,mark_time_ms,mark_method,cashflow\n",
    );
    for row in rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{}",
            row.settlement_time_ms,
            row.signed_position,
            row.funding_rate,
            row.mark_price,
            row.mark_time_ms,
            row.mark_method,
            row.cashflow
        );
    }
    std::fs::write(path, out).map_err(|err| BacktestError::new(format!("write funding: {err}")))
}

fn write_trades(path: &Path, rows: &[TradeRecord]) -> Result<(), BacktestError> {
    let mut out = String::from("entry_decision_id,exit_decision_id,side,quantity,entry_time_ms,exit_time_ms,holding_period_ms,entry_fill,exit_fill,price_pnl,fees,funding,net_pnl\n");
    for row in rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            row.entry_decision_id,
            row.exit_decision_id,
            row.side,
            row.quantity,
            row.entry_time_ms,
            row.exit_time_ms,
            row.holding_period_ms,
            row.entry_fill,
            row.exit_fill,
            row.price_pnl,
            row.fees,
            row.funding,
            row.net_pnl
        );
    }
    std::fs::write(path, out).map_err(|err| BacktestError::new(format!("write trades: {err}")))
}

fn write_pnl(path: &Path, rows: &[PnlSample]) -> Result<(), BacktestError> {
    let mut out = String::from("observation_time_ms,mark_time_ms,mark_price,mark_source,position,realized_price_pnl,unrealized_price_pnl,cumulative_fees,cumulative_funding,cumulative_net_pnl,running_peak,drawdown_usdt\n");
    for row in rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            row.observation_time_ms,
            row.mark_time_ms,
            row.mark_price,
            row.mark_source,
            row.position,
            row.realized_price_pnl,
            row.unrealized_price_pnl,
            row.cumulative_fees,
            row.cumulative_funding,
            row.cumulative_net_pnl,
            row.running_peak,
            row.drawdown_usdt
        );
    }
    std::fs::write(path, out).map_err(|err| BacktestError::new(format!("write pnl: {err}")))
}

fn optional_ms(value: Option<u64>) -> String {
    value.map(|ms| ms.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn qty(sign: i64) -> Decimal {
        Decimal::new(sign, 3)
    }

    fn change(time: u64, target: Decimal) -> TargetChange {
        TargetChange {
            decision_id: format!("d{time}"),
            source_start_ms: Some(time),
            source_end_ms: Some(time),
            decision_time_ms: time,
            target_qty: target,
            reason: "fixture".to_string(),
        }
    }

    fn scenario(fee: i64, adjustment: i64, delay_ms: u64, end: u64) -> ExecutionScenario {
        ExecutionScenario {
            name: "fixture".to_string(),
            fee_bps: Decimal::from(fee),
            execution_adjustment_bps: Decimal::from(adjustment),
            decision_delay_ms: delay_ms,
            performance_start_ms: 0,
            performance_end_exclusive_ms: end,
        }
    }

    fn hourly(end: u64, price: i64) -> BTreeMap<u64, Decimal> {
        let mut marks = BTreeMap::new();
        let mut time = 0;
        while time <= end {
            marks.insert(time, Decimal::from(price));
            time += HOUR_MS;
        }
        marks
    }

    fn run_changes(
        changes: &[TargetChange],
        scenario: &ExecutionScenario,
        minutes: &BTreeMap<u64, Decimal>,
        marks: &BTreeMap<u64, Decimal>,
        funding: &[FundingRecord],
    ) -> Simulation {
        simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "flat",
            signal_config_hash: "sha256:fixture",
            changes,
            scenario,
            minutes,
            funding,
            hourly_marks: marks,
            finer_marks: &BTreeMap::new(),
        })
        .unwrap()
    }

    fn round_trip(sign: i64, entry: i64, exit_px: i64, fee: i64, adjustment: i64) -> Simulation {
        let end = 10_800_000;
        let changes = vec![change(0, qty(sign)), change(7_140_000, Decimal::ZERO)];
        let mut minutes = BTreeMap::new();
        minutes.insert(60_000, Decimal::from(entry));
        minutes.insert(7_200_000, Decimal::from(exit_px));
        run_changes(
            &changes,
            &scenario(fee, adjustment, 10_000, end),
            &minutes,
            &hourly(end, entry),
            &[],
        )
    }

    #[test]
    fn research_scenarios_are_the_prescribed_synthetic_inputs() {
        let scenarios = research_scenarios(10, 20);
        assert_eq!(scenarios.len(), 4);
        assert_eq!(scenarios[0].name, "zero_cost_diagnostic");
        assert_eq!(scenarios[0].fee_bps, Decimal::ZERO);
        assert_eq!(scenarios[0].execution_adjustment_bps, Decimal::ZERO);
        assert_eq!(scenarios[0].decision_delay_ms, 10_000);
        assert_eq!(scenarios[1].name, "cost_a");
        assert_eq!(scenarios[1].fee_bps, Decimal::from(6));
        assert_eq!(scenarios[1].execution_adjustment_bps, Decimal::from(2));
        assert_eq!(scenarios[2].fee_bps, Decimal::from(10));
        assert_eq!(scenarios[2].execution_adjustment_bps, Decimal::from(10));
        assert_eq!(scenarios[3].name, "delayed_execution");
        assert_eq!(scenarios[3].decision_delay_ms, 300_000);
        assert_eq!(scenarios[3].fee_bps, Decimal::from(6));
        assert_eq!(scenarios[3].execution_adjustment_bps, Decimal::from(2));
        let hashes: BTreeSet<_> = scenarios
            .iter()
            .map(|scenario| scenario_hash(scenario, FROZEN_CONFIG_HASH).unwrap())
            .collect();
        assert_eq!(hashes.len(), 4);
    }

    #[test]
    fn next_minute_is_strictly_after_availability() {
        assert_eq!(next_minute_start_strictly_after(10_000).unwrap(), 60_000);
        assert_eq!(next_minute_start_strictly_after(300_000).unwrap(), 360_000);
        assert_eq!(next_minute_start_strictly_after(0).unwrap(), 60_000);
        assert_eq!(next_minute_start_strictly_after(60_001).unwrap(), 120_000);
    }

    #[test]
    fn profitable_and_losing_longs_and_shorts_match_hand_pnl() {
        let profitable_long = round_trip(1, 10_000, 11_000, 0, 0);
        assert_eq!(profitable_long.trades[0].price_pnl, Decimal::ONE);
        assert_eq!(profitable_long.summary.net_pnl, Decimal::ONE);
        assert_eq!(profitable_long.trades[0].side, "long");

        let losing_long = round_trip(1, 10_000, 9_000, 0, 0);
        assert_eq!(losing_long.trades[0].price_pnl, Decimal::NEGATIVE_ONE);
        assert_eq!(losing_long.summary.net_pnl, Decimal::NEGATIVE_ONE);

        let profitable_short = round_trip(-1, 10_000, 9_000, 0, 0);
        assert_eq!(profitable_short.trades[0].side, "short");
        assert_eq!(profitable_short.trades[0].quantity, qty(-1));
        assert_eq!(profitable_short.trades[0].price_pnl, Decimal::ONE);
        assert_eq!(profitable_short.summary.net_pnl, Decimal::ONE);

        let losing_short = round_trip(-1, 10_000, 11_000, 0, 0);
        assert_eq!(losing_short.trades[0].price_pnl, Decimal::NEGATIVE_ONE);
        assert_eq!(losing_short.summary.net_pnl, Decimal::NEGATIVE_ONE);
    }

    #[test]
    fn entry_and_exit_fees_are_charged_once() {
        // entry fee = 0.001 * 10000 * 10 / 10000 = 0.01
        // exit fee = 0.001 * 11000 * 10 / 10000 = 0.011
        // price pnl = 1, net = 0.979
        let simulation = round_trip(1, 10_000, 11_000, 10, 0);
        assert_eq!(simulation.fills[0].fee, dec!(0.01));
        assert_eq!(simulation.fills[1].fee, dec!(0.011));
        assert_eq!(simulation.trades[0].price_pnl, Decimal::ONE);
        assert_eq!(simulation.trades[0].fees, dec!(0.021));
        assert_eq!(simulation.summary.fees, dec!(0.021));
        assert_eq!(simulation.summary.gross_price_pnl, Decimal::ONE);
        assert_eq!(simulation.summary.net_pnl, dec!(0.979));
        assert_eq!(
            simulation.summary.net_pnl,
            simulation.summary.gross_price_pnl - simulation.summary.fees
                + simulation.summary.funding
        );
    }

    #[test]
    fn adverse_adjustment_applies_to_all_four_directions_and_is_not_subtracted_again() {
        let end = 14_400_000;
        let changes = vec![
            change(0, qty(1)),
            change(3_540_000, Decimal::ZERO),
            change(7_140_000, qty(-1)),
            change(10_740_000, Decimal::ZERO),
        ];
        let mut minutes = BTreeMap::new();
        for time in [60_000, 3_600_000, 7_200_000, 10_800_000] {
            minutes.insert(time, Decimal::from(10_000));
        }
        let simulation = run_changes(
            &changes,
            &scenario(0, 2, 10_000, end),
            &minutes,
            &hourly(end, 10_000),
            &[],
        );
        assert_eq!(simulation.fills[0].side, "buy");
        assert_eq!(simulation.fills[0].fill_price, dec!(10002));
        assert_eq!(simulation.fills[1].side, "sell");
        assert_eq!(simulation.fills[1].fill_price, dec!(9998));
        assert_eq!(simulation.fills[2].side, "sell");
        assert_eq!(simulation.fills[2].fill_price, dec!(9998));
        assert_eq!(simulation.fills[3].side, "buy");
        assert_eq!(simulation.fills[3].fill_price, dec!(10002));
        assert_eq!(simulation.summary.fees, Decimal::ZERO);
        assert_eq!(simulation.summary.net_pnl, dec!(-0.008));
        assert_eq!(simulation.summary.gross_price_pnl, dec!(-0.008));

        // entry 10002, exit 11000 * 0.9998 = 10997.8, pnl = 0.001 * 995.8 = 0.9958
        let adjusted = round_trip(1, 10_000, 11_000, 0, 2);
        assert_eq!(adjusted.fills[0].fill_price, dec!(10002));
        assert_eq!(adjusted.fills[1].reference_open, Decimal::from(11_000));
        assert_eq!(adjusted.fills[1].fill_price, dec!(10997.8));
        assert_eq!(adjusted.trades[0].price_pnl, dec!(0.9958));
        assert_eq!(adjusted.summary.net_pnl, dec!(0.9958));
        assert_eq!(adjusted.summary.fees, Decimal::ZERO);
    }

    #[test]
    fn funding_sign_follows_position_and_is_zero_when_flat() {
        let cases = [
            (1, dec!(0.0001), dec!(-0.001)),
            (1, dec!(-0.0001), dec!(0.001)),
            (-1, dec!(0.0001), dec!(0.001)),
            (-1, dec!(-0.0001), dec!(-0.001)),
        ];
        for (sign, rate, expected) in cases {
            let end = 10_800_000;
            let changes = vec![change(0, qty(sign)), change(7_140_000, Decimal::ZERO)];
            let mut minutes = BTreeMap::new();
            minutes.insert(60_000, Decimal::from(10_000));
            minutes.insert(7_200_000, Decimal::from(10_000));
            let funding = [FundingRecord {
                settlement_time_ms: 3_600_000,
                funding_rate: rate,
            }];
            let simulation = run_changes(
                &changes,
                &scenario(0, 0, 10_000, end),
                &minutes,
                &hourly(end, 10_000),
                &funding,
            );
            assert_eq!(simulation.funding_rows.len(), 1);
            assert_eq!(simulation.funding_rows[0].signed_position, qty(sign));
            assert_eq!(simulation.funding_rows[0].cashflow, expected);
            assert_eq!(
                simulation.funding_rows[0].mark_method,
                "hourly_mark_open_at_settlement"
            );
            assert_eq!(simulation.trades[0].price_pnl, Decimal::ZERO);
            assert_eq!(simulation.trades[0].funding, expected);
            assert_eq!(simulation.summary.net_pnl, expected);
        }

        let end = 7_200_000;
        let flat = run_changes(
            &[],
            &scenario(0, 0, 10_000, end),
            &BTreeMap::new(),
            &hourly(end, 10_000),
            &[FundingRecord {
                settlement_time_ms: 3_600_000,
                funding_rate: dec!(0.0001),
            }],
        );
        assert!(flat.trades.is_empty());
        assert_eq!(flat.funding_rows[0].signed_position, Decimal::ZERO);
        assert_eq!(flat.funding_rows[0].cashflow, Decimal::ZERO);
        assert_eq!(flat.summary.net_pnl, Decimal::ZERO);
    }

    #[test]
    fn funding_at_the_fill_timestamp_uses_the_pre_fill_position() {
        // Entry fill lands on 3_600_000. Exit fill lands on 7_200_000.
        // Funding at the entry timestamp sees a flat book. Funding at the exit
        // timestamp still sees the open long: -0.001 * 10000 * 0.0001 = -0.001.
        // Price pnl = 0.001 * (11000 - 10000) = 1. Net = 0.999.
        let end = 10_800_000;
        let changes = vec![change(3_540_000, qty(1)), change(7_140_000, Decimal::ZERO)];
        let mut minutes = BTreeMap::new();
        minutes.insert(3_600_000, Decimal::from(10_000));
        minutes.insert(7_200_000, Decimal::from(11_000));
        let funding = [
            FundingRecord {
                settlement_time_ms: 0,
                funding_rate: dec!(0.0001),
            },
            FundingRecord {
                settlement_time_ms: 3_600_000,
                funding_rate: dec!(0.01),
            },
            FundingRecord {
                settlement_time_ms: 7_200_000,
                funding_rate: dec!(0.0001),
            },
        ];
        let simulation = run_changes(
            &changes,
            &scenario(0, 0, 10_000, end),
            &minutes,
            &hourly(end, 10_000),
            &funding,
        );
        assert_eq!(simulation.funding_rows[0].signed_position, Decimal::ZERO);
        assert_eq!(simulation.funding_rows[0].cashflow, Decimal::ZERO);
        assert_eq!(simulation.funding_rows[1].settlement_time_ms, 3_600_000);
        assert_eq!(simulation.funding_rows[1].signed_position, Decimal::ZERO);
        assert_eq!(simulation.funding_rows[1].cashflow, Decimal::ZERO);
        assert_eq!(simulation.funding_rows[2].signed_position, qty(1));
        assert_eq!(simulation.funding_rows[2].cashflow, dec!(-0.001));
        assert_eq!(simulation.fills[0].fill_time_ms, 3_600_000);
        assert_eq!(simulation.fills[1].fill_time_ms, 7_200_000);
        assert_eq!(simulation.trades[0].price_pnl, Decimal::ONE);
        assert_eq!(simulation.trades[0].funding, dec!(-0.001));
        assert_eq!(simulation.summary.net_pnl, dec!(0.999));
    }

    #[test]
    fn nonzero_delay_does_not_fill_at_the_signal_close() {
        let row = DecisionRow {
            venue: "bybit".to_string(),
            product: "linear".to_string(),
            instrument: "BTCUSDT".to_string(),
            interval: "D".to_string(),
            start_time_ms: 0,
            end_time_exclusive_ms: 0,
            close: Decimal::from(99_999),
            entry_high: None,
            entry_low: None,
            exit_high: None,
            exit_low: None,
            previous_direction: "flat".to_string(),
            direction: "long".to_string(),
            readiness: "ready".to_string(),
            suspension: None,
            reason: "fixture".to_string(),
            target: EmittedTarget::Quantity { quantity: qty(1) },
        };
        let changes = target_changes(&[row]).unwrap();
        let end = 3_600_000;
        let mut minutes = BTreeMap::new();
        minutes.insert(0, Decimal::ONE);
        minutes.insert(60_000, Decimal::from(50_000));
        let simulation = run_changes(
            &changes,
            &scenario(0, 0, 10_000, end),
            &minutes,
            &hourly(end, 50_000),
            &[],
        );
        assert_eq!(simulation.fills.len(), 1);
        assert_eq!(simulation.fills[0].available_at_ms, 10_000);
        assert_eq!(simulation.fills[0].fill_time_ms, 60_000);
        assert_eq!(simulation.fills[0].reference_open, Decimal::from(50_000));
        assert_eq!(simulation.fills[0].fill_price, Decimal::from(50_000));
        assert_ne!(simulation.fills[0].fill_price, Decimal::from(99_999));
    }

    #[test]
    fn no_change_does_not_flatten_or_add_a_fill() {
        let rows = [
            decision_row(0, EmittedTarget::Quantity { quantity: qty(1) }, "long"),
            decision_row(86_400_000, EmittedTarget::NoChange, "long"),
            decision_row(
                172_800_000,
                EmittedTarget::Quantity {
                    quantity: Decimal::ZERO,
                },
                "flat",
            ),
        ];
        let changes = target_changes(&rows).unwrap();
        assert_eq!(changes.len(), 2);
        let end = 172_800_000 + 3_600_000;
        let mut minutes = BTreeMap::new();
        minutes.insert(60_000, Decimal::from(10_000));
        minutes.insert(172_860_000, Decimal::from(10_000));
        let mut marks = BTreeMap::new();
        let mut time = 0;
        while time <= end {
            marks.insert(time, Decimal::from(10_000));
            time += HOUR_MS;
        }
        let simulation = run_changes(
            &changes,
            &scenario(0, 0, 10_000, end),
            &minutes,
            &marks,
            &[],
        );
        assert_eq!(simulation.fills.len(), 2);
        assert_eq!(simulation.summary.simulated_position_at_end, Decimal::ZERO);
    }

    #[test]
    fn pending_execution_at_the_cutoff_is_not_a_fill() {
        let end = 3_600_000;
        let marks = hourly(end, 10_000);
        let mut minutes = BTreeMap::new();
        minutes.insert(3_660_000, Decimal::from(1));
        let at_cutoff = run_changes(
            &[change(end, qty(1))],
            &scenario(0, 0, 10_000, end),
            &minutes,
            &marks,
            &[],
        );
        assert!(at_cutoff.fills.is_empty());
        assert_eq!(at_cutoff.summary.pending.len(), 1);
        assert_eq!(at_cutoff.summary.simulated_position_at_end, Decimal::ZERO);
        assert_eq!(at_cutoff.summary.model_direction_at_end, "flat");

        let before_cutoff = simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "long",
            signal_config_hash: "sha256:fixture",
            changes: &[change(3_590_000, qty(1))],
            scenario: &scenario_with_delay(5_000, end),
            minutes: &minutes,
            funding: &[],
            hourly_marks: &marks,
            finer_marks: &BTreeMap::new(),
        })
        .unwrap();
        assert!(before_cutoff.fills.is_empty());
        assert_eq!(before_cutoff.summary.pending.len(), 1);
        assert_eq!(
            before_cutoff.summary.simulated_position_at_end,
            Decimal::ZERO
        );
        assert_eq!(before_cutoff.summary.model_direction_at_end, "long");
        assert!(before_cutoff
            .summary
            .fill_assumption
            .contains("entire required quantity"));
        assert!(before_cutoff.summary.result_label.contains("synthetic"));
    }

    #[test]
    fn unfinished_trade_keeps_unrealized_pnl_and_a_separate_close_estimate() {
        let end = 3_600_000;
        let mut minutes = BTreeMap::new();
        minutes.insert(60_000, Decimal::from(100));
        let mut marks = BTreeMap::new();
        marks.insert(0, Decimal::from(100));
        marks.insert(end, Decimal::from(90));
        let simulation = run_changes(
            &[change(0, qty(1))],
            &scenario(0, 0, 10_000, end),
            &minutes,
            &marks,
            &[],
        );
        assert!(simulation.trades.is_empty());
        assert_eq!(simulation.summary.simulated_position_at_end, qty(1));
        assert_eq!(simulation.summary.unrealized_price_pnl, dec!(-0.01));
        assert_eq!(simulation.summary.realized_price_pnl, Decimal::ZERO);
        assert_eq!(simulation.summary.net_pnl, dec!(-0.01));
        assert_eq!(simulation.summary.estimated_close_cost_usdt, Decimal::ZERO);
        assert!(simulation.summary.pending.is_empty());
    }

    #[test]
    fn drawdown_is_visible_while_the_losing_position_is_still_open() {
        let end = 10_800_000;
        let changes = vec![change(0, qty(1)), change(7_140_000, Decimal::ZERO)];
        let mut minutes = BTreeMap::new();
        minutes.insert(60_000, Decimal::from(100));
        minutes.insert(7_200_000, Decimal::from(100));
        let mut marks = hourly(end, 100);
        marks.insert(3_600_000, Decimal::from(50));
        let simulation = run_changes(
            &changes,
            &scenario(0, 0, 10_000, end),
            &minutes,
            &marks,
            &[],
        );
        let dipped = simulation
            .pnl
            .iter()
            .find(|sample| sample.observation_time_ms == 3_600_000)
            .unwrap();
        assert_eq!(dipped.position, qty(1));
        assert_eq!(dipped.unrealized_price_pnl, dec!(-0.05));
        assert_eq!(dipped.drawdown_usdt, dec!(0.05));
        assert_eq!(simulation.summary.max_drawdown_usdt, dec!(0.05));
        assert_eq!(simulation.summary.max_drawdown_time_ms, 3_600_000);
        assert!(simulation.summary.max_drawdown_time_ms < simulation.fills[1].fill_time_ms);
        assert_eq!(simulation.summary.net_pnl, Decimal::ZERO);
    }

    #[test]
    fn missing_execution_mark_or_funding_coverage_fails() {
        let end = 3_600_000;
        let marks = hourly(end, 100);
        let mut later = BTreeMap::new();
        later.insert(120_000, Decimal::from(1));
        let missing_minute = simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "long",
            signal_config_hash: "sha256:fixture",
            changes: &[change(0, qty(1))],
            scenario: &scenario(0, 0, 10_000, end),
            minutes: &later,
            funding: &[],
            hourly_marks: &marks,
            finer_marks: &BTreeMap::new(),
        })
        .unwrap_err();
        assert!(missing_minute
            .to_string()
            .contains("missing execution observation"));
        assert!(!missing_minute.to_string().contains("120000"));

        let mut incomplete_marks = marks.clone();
        incomplete_marks.remove(&end);
        let missing_mark = simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "flat",
            signal_config_hash: "sha256:fixture",
            changes: &[],
            scenario: &scenario(0, 0, 10_000, end),
            minutes: &BTreeMap::new(),
            funding: &[],
            hourly_marks: &incomplete_marks,
            finer_marks: &BTreeMap::new(),
        })
        .unwrap_err();
        assert!(missing_mark.to_string().contains("missing hourly mark"));

        let missing_valuation = simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "flat",
            signal_config_hash: "sha256:fixture",
            changes: &[],
            scenario: &scenario(0, 0, 10_000, end),
            minutes: &BTreeMap::new(),
            funding: &[FundingRecord {
                settlement_time_ms: 60_000,
                funding_rate: dec!(0.0001),
            }],
            hourly_marks: &marks,
            finer_marks: &BTreeMap::new(),
        })
        .unwrap_err();
        assert!(missing_valuation
            .to_string()
            .contains("missing mark valuation"));

        let market = MarketInputs {
            minutes: BTreeMap::new(),
            funding: vec![FundingRecord {
                settlement_time_ms: 3_600_000,
                funding_rate: dec!(0.0001),
            }],
            hourly_marks: marks,
            finer_marks: BTreeMap::new(),
            coverage: Vec::new(),
        };
        let err = validate_coverage(0, end, &BTreeSet::from([60_000]), &market).unwrap_err();
        assert!(err.to_string().contains("missing execution observation"));
        let err = validate_coverage(0, end, &BTreeSet::new(), &market).unwrap_err();
        assert!(err.to_string().contains("funding coverage is incomplete"));
    }

    #[test]
    fn repeated_runs_match_and_scenario_changes_do_not_change_decisions() {
        let first = round_trip(1, 10_000, 11_000, 6, 2);
        let second = round_trip(1, 10_000, 11_000, 6, 2);
        assert_eq!(first, second);

        let mut rows = vec![
            decision_row(0, EmittedTarget::Quantity { quantity: qty(1) }, "long"),
            decision_row(86_400_000, EmittedTarget::NoChange, "long"),
            decision_row(
                172_800_000,
                EmittedTarget::Quantity {
                    quantity: Decimal::ZERO,
                },
                "flat",
            ),
        ];
        let snapshot = rows.clone();
        let changes = target_changes(&rows).unwrap();
        let end = 172_800_000 + 3_600_000;
        let mut minutes = BTreeMap::new();
        minutes.insert(60_000, Decimal::from(10_000));
        minutes.insert(172_860_000, Decimal::from(11_000));
        let mut marks = BTreeMap::new();
        let mut time = 0;
        while time <= end {
            marks.insert(time, Decimal::from(10_000));
            time += HOUR_MS;
        }
        let cheap = run_changes(
            &changes,
            &scenario(0, 0, 10_000, end),
            &minutes,
            &marks,
            &[],
        );
        let costly = run_changes(
            &changes,
            &scenario(10, 0, 10_000, end),
            &minutes,
            &marks,
            &[],
        );
        rows[0].reason = "mutated".to_string();
        rows[0].reason = snapshot[0].reason.clone();
        assert_eq!(rows, snapshot);
        assert_eq!(
            cheap.summary.target_fingerprint,
            costly.summary.target_fingerprint
        );
        assert_ne!(cheap.summary.scenario_hash, costly.summary.scenario_hash);
        assert!(costly.summary.net_pnl < cheap.summary.net_pnl);
        assert_eq!(cheap.fills[0].fill_time_ms, costly.fills[0].fill_time_ms);
        assert_eq!(cheap.fills[1].fill_time_ms, costly.fills[1].fill_time_ms);
        assert!(cheap
            .summary
            .limitations
            .iter()
            .any(|item| item.contains("not validated leveraged")));
    }

    #[test]
    fn calendar_year_pnl_puts_the_january_settlement_in_the_new_year() {
        let start = utc(2023, 12, 31, 22);
        let end = utc(2024, 1, 1, 2);
        let boundary = utc(2024, 1, 1, 0);
        let mut marks = BTreeMap::new();
        let mut time = start;
        while time <= end {
            marks.insert(time, Decimal::from(10_000));
            time += HOUR_MS;
        }
        let mut minutes = BTreeMap::new();
        minutes.insert(start + 60_000, Decimal::from(10_000));
        let simulation = simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "long",
            signal_config_hash: "sha256:fixture",
            changes: &[change(start, qty(1))],
            scenario: &ExecutionScenario {
                name: "fixture".to_string(),
                fee_bps: Decimal::ZERO,
                execution_adjustment_bps: Decimal::ZERO,
                decision_delay_ms: 10_000,
                performance_start_ms: start,
                performance_end_exclusive_ms: end,
            },
            minutes: &minutes,
            funding: &[FundingRecord {
                settlement_time_ms: boundary,
                funding_rate: dec!(0.0001),
            }],
            hourly_marks: &marks,
            finer_marks: &BTreeMap::new(),
        })
        .unwrap();
        assert_eq!(simulation.summary.net_pnl, dec!(-0.001));
        assert_eq!(simulation.summary.calendar_years.len(), 2);
        assert_eq!(simulation.summary.calendar_years[0].year, 2023);
        assert!(simulation.summary.calendar_years[0].partial);
        assert_eq!(simulation.summary.calendar_years[0].net_pnl, Decimal::ZERO);
        assert_eq!(simulation.summary.calendar_years[1].year, 2024);
        assert!(simulation.summary.calendar_years[1].partial);
        assert_eq!(simulation.summary.calendar_years[1].net_pnl, dec!(-0.001));
        assert!(simulation
            .summary
            .calendar_years
            .iter()
            .all(|year| year.year != 2026));
    }

    #[test]
    fn fills_that_overlap_the_next_decision_are_rejected() {
        let end = 3_600_000;
        let err = simulate(&SimulateRequest {
            book: "fixture",
            book_note: "fixture",
            model_direction_at_end: "flat",
            signal_config_hash: "sha256:fixture",
            changes: &[change(0, qty(1)), change(30_000, Decimal::ZERO)],
            scenario: &scenario(0, 0, 10_000, end),
            minutes: &BTreeMap::from([(60_000, Decimal::from(100))]),
            funding: &[],
            hourly_marks: &hourly(end, 100),
            finer_marks: &BTreeMap::new(),
        })
        .unwrap_err();
        assert!(err.to_string().contains("overlap"));
    }

    fn scenario_with_delay(delay_ms: u64, end: u64) -> ExecutionScenario {
        scenario(0, 0, delay_ms, end)
    }

    fn decision_row(start: u64, target: EmittedTarget, direction: &str) -> DecisionRow {
        DecisionRow {
            venue: "bybit".to_string(),
            product: "linear".to_string(),
            instrument: "BTCUSDT".to_string(),
            interval: "D".to_string(),
            start_time_ms: start,
            end_time_exclusive_ms: start,
            close: Decimal::ZERO,
            entry_high: None,
            entry_low: None,
            exit_high: None,
            exit_low: None,
            previous_direction: "flat".to_string(),
            direction: direction.to_string(),
            readiness: "ready".to_string(),
            suspension: None,
            reason: "fixture".to_string(),
            target,
        }
    }

    fn utc(year: i32, month: u32, day: u32, hour: u32) -> u64 {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis() as u64
    }
}
