//! Primary experiment: four scenarios, placebo, passive report, and the kill gates.
//!
//! Robustness cells are scored only after the primary promotion checks pass,
//! and only when their execution minutes are already in the frozen minute file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use chrono::{Datelike, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use strategy_core::btc_liquidation_shock_v1::{
    config_hash, position_qty, Params, SignMode, STRATEGY_NAME,
};

use crate::donchian_daily_v1::aux::{
    download_trade_minutes_for, load_funding_file, load_price_series, market_inputs_from_files,
    AuxDownloadRequest, PriceSeriesFile,
};
use crate::donchian_daily_v1::backtest::{
    changes_for, required_execution_minutes, research_scenarios, simulate, write_simulation, Book,
    ExecutionScenario, MarketInputs, Simulation, TargetChange, TradeRecord, HOUR_MS,
};
use crate::donchian_daily_v1::offline::MarketDataEnvironment;

use super::coverage::{require_funding_grid, FROZEN_FUNDING_HASH, FROZEN_MARK_60_HASH};
use super::dataset::{
    sha256_file, HourlyDataset, PERFORMANCE_END_EXCLUSIVE_MS, PERFORMANCE_START_MS,
};
use super::replay::{replay, DecisionLog, DecisionRow};
use super::{load_experiment, ShockError};

const CANDIDATE_2_COST_A_NET: &str = "20.6120701293059";
const YEAR_MS: u64 = 31_557_600_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeStatus {
    pub revision: String,
    pub dirty: bool,
}

pub fn inspect_worktree(start: &Path) -> Result<WorktreeStatus, ShockError> {
    let revision = git(start, &["rev-parse", "HEAD"])?;
    let porcelain = git(start, &["status", "--porcelain"])?;
    Ok(WorktreeStatus {
        revision,
        dirty: !porcelain.is_empty(),
    })
}

pub fn require_clean(status: &WorktreeStatus) -> Result<(), ShockError> {
    if status.dirty {
        return Err(ShockError::new(
            "performance run requires a clean git worktree so the manifest names one revision",
        ));
    }
    if status.revision.len() != 40 {
        return Err(ShockError::new("git revision was not a full commit sha"));
    }
    Ok(())
}

fn git(start: &Path, args: &[&str]) -> Result<String, ShockError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(start)
        .args(args)
        .output()
        .map_err(|err| ShockError::new(format!("git: {err}")))?;
    if !output.status.success() {
        return Err(ShockError::new(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub struct RunRequest<'a> {
    pub experiment: &'a Path,
    pub human_spec: &'a Path,
    pub hourly: &'a Path,
    pub decisions: &'a Path,
    pub placebo_decisions: &'a Path,
    pub minutes: &'a Path,
    pub mark: &'a Path,
    pub funding: &'a Path,
    pub output: &'a Path,
    pub worktree: &'a Path,
}

pub fn run_primary(request: &RunRequest<'_>) -> Result<String, ShockError> {
    let worktree = inspect_worktree(request.worktree)?;
    require_clean(&worktree)?;
    let spec_sha = sha256_file(request.experiment)?;
    let human_sha = sha256_file(request.human_spec)?;
    let experiment = load_experiment(request.experiment)?;
    let hourly = super::dataset::load_hourly_dataset(request.hourly)?;
    let reversal = load_matching_log(
        request.decisions,
        &hourly,
        &spec_sha,
        &human_sha,
        SignMode::Reversal,
        experiment.primary.sigma_multiple.as_str(),
        experiment.primary.hold_completed_hours,
    )?;
    let placebo = load_matching_log(
        request.placebo_decisions,
        &hourly,
        &spec_sha,
        &human_sha,
        SignMode::Continuation,
        experiment.primary.sigma_multiple.as_str(),
        experiment.primary.hold_completed_hours,
    )?;
    let market = load_market(request.minutes, request.mark, request.funding)?;
    let scenarios = research_scenarios(PERFORMANCE_START_MS, PERFORMANCE_END_EXCLUSIVE_MS);
    let reversal_changes = to_changes(&reversal.decisions);
    let placebo_changes = to_changes(&placebo.decisions);
    let passive_changes = changes_for(
        Book::FixedLongBenchmark,
        &[],
        PERFORMANCE_START_MS,
        position_qty(),
    )
    .map_err(|err| ShockError::new(err.to_string()))?;

    std::fs::create_dir_all(request.output)
        .map_err(|err| ShockError::new(format!("create output: {err}")))?;

    let mut reversal_sims = Vec::new();
    let mut placebo_sims = Vec::new();
    let mut passive_sims = Vec::new();
    for scenario in &scenarios {
        reversal_sims.push(score(
            request.output,
            "reversal",
            &reversal,
            &reversal_changes,
            scenario,
            &market,
        )?);
        placebo_sims.push(score(
            request.output,
            "placebo",
            &placebo,
            &placebo_changes,
            scenario,
            &market,
        )?);
        passive_sims.push(score_passive(
            request.output,
            &passive_changes,
            scenario,
            &market,
            &hourly.metadata.dataset_hash,
        )?);
    }
    let cost_a = find_sim(&reversal_sims, "cost_a")?;
    let higher = find_sim(&reversal_sims, "higher_cost")?;
    let delayed = find_sim(&reversal_sims, "delayed_execution")?;
    let placebo_cost_a = find_sim(&placebo_sims, "cost_a")?;
    let passive_cost_a = find_sim(&passive_sims, "cost_a")?;
    let entry_times = decision_times(&reversal.decisions);
    let year_nets = entry_year_nets(&cost_a.trades, &entry_times)?;
    let largest_winner = cost_a
        .trades
        .iter()
        .filter(|trade| trade.net_pnl > Decimal::ZERO)
        .map(|trade| trade.net_pnl)
        .max();
    let negative_years = [2023, 2024, 2025]
        .into_iter()
        .filter(|year| year_nets.get(year).copied().unwrap_or(Decimal::ZERO) < Decimal::ZERO)
        .count();
    let gates = evaluate_promotion(&PromotionInput {
        completed_trades: cost_a.summary.completed_trades,
        cost_a_net: cost_a.summary.net_pnl,
        higher_net: higher.summary.net_pnl,
        delayed_net: delayed.summary.net_pnl,
        placebo_cost_a_net: placebo_cost_a.summary.net_pnl,
        largest_winner,
        negative_entry_years: negative_years,
    });
    let promotion_passed = gates.iter().all(|gate| gate.passed);
    let (robustness, robustness_note) = if promotion_passed {
        match score_robustness(&hourly, &spec_sha, &human_sha, &market, request.output)? {
            RobustnessOutcome::Scored(cells) => (Some(cells), None),
            RobustnessOutcome::MissingMinutes { count } => (
                None,
                Some(format!(
                    "primary promotion passed; {count} robustness execution minutes are absent, so the neighborhood was not scored"
                )),
            ),
        }
    } else {
        (
            None,
            Some(
                "primary promotion failed; the robustness neighborhood was not opened".to_string(),
            ),
        )
    };
    let robustness_passed = robustness.as_ref().map(|cells| neighborhood_passes(cells));
    let disposition = match (promotion_passed, robustness_passed) {
        (false, _) => "reject",
        (true, Some(true)) => "provisional_pass",
        (true, Some(false)) => "reject",
        (true, None) => "promotion_pass_robustness_not_scored",
    };
    let terciles = volatility_terciles(&cost_a.trades, &reversal.decisions);
    let report = SummaryReport {
        candidate_id: STRATEGY_NAME.to_string(),
        disposition: disposition.to_string(),
        revision: worktree.revision.clone(),
        spec_sha256: spec_sha,
        human_spec_sha256: human_sha,
        dataset_hash: hourly.metadata.dataset_hash.clone(),
        gates,
        robustness_note,
        robustness,
        terciles,
        reversal_cost_a: book_line(cost_a),
        placebo_cost_a: book_line(placebo_cost_a),
        passive_cost_a: book_line(passive_cost_a),
        passive_scaled_net: scaled_passive(cost_a, passive_cost_a),
        candidate_2_cost_a_net_context: CANDIDATE_2_COST_A_NET.to_string(),
        entry_year_nets: year_nets
            .iter()
            .map(|(year, net)| YearNet {
                year: *year,
                net: *net,
            })
            .collect(),
    };
    write_json(&request.output.join("gates.json"), &report.gates)?;
    write_json(&request.output.join("summary.json"), &report)?;
    std::fs::write(request.output.join("report.md"), render_report(&report))
        .map_err(|err| ShockError::new(format!("write report: {err}")))?;
    let manifest = Manifest {
        candidate_id: STRATEGY_NAME.to_string(),
        candidate_version: experiment.candidate_version,
        revision: worktree.revision,
        dirty: false,
        disposition: disposition.to_string(),
        spec_sha256: report.spec_sha256.clone(),
        human_spec_sha256: report.human_spec_sha256.clone(),
        config_hash: reversal.config_hash,
        dataset_hash: hourly.metadata.dataset_hash,
        trade_1m_hash: market_hash(&market, "trade_1m")?,
        mark_hash: FROZEN_MARK_60_HASH.to_string(),
        funding_hash: FROZEN_FUNDING_HASH.to_string(),
        performance_start_ms: PERFORMANCE_START_MS,
        performance_end_exclusive_ms: PERFORMANCE_END_EXCLUSIVE_MS,
    };
    write_json(&request.output.join("manifest.json"), &manifest)?;
    Ok(disposition.to_string())
}

#[derive(Debug, Clone, Serialize)]
struct Manifest {
    candidate_id: String,
    candidate_version: String,
    revision: String,
    dirty: bool,
    disposition: String,
    spec_sha256: String,
    human_spec_sha256: String,
    config_hash: String,
    dataset_hash: String,
    trade_1m_hash: String,
    mark_hash: String,
    funding_hash: String,
    performance_start_ms: u64,
    performance_end_exclusive_ms: u64,
}

fn market_hash(market: &MarketInputs, series: &str) -> Result<String, ShockError> {
    market
        .coverage
        .iter()
        .find(|note| note.series == series)
        .map(|note| note.dataset_hash.clone())
        .ok_or_else(|| ShockError::new(format!("market inputs have no {series} hash")))
}

pub struct PromotionInput {
    pub completed_trades: usize,
    pub cost_a_net: Decimal,
    pub higher_net: Decimal,
    pub delayed_net: Decimal,
    pub placebo_cost_a_net: Decimal,
    pub largest_winner: Option<Decimal>,
    pub negative_entry_years: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateOutcome {
    pub gate: String,
    pub passed: bool,
    pub detail: String,
}

pub fn evaluate_promotion(input: &PromotionInput) -> Vec<GateOutcome> {
    let mut gates = vec![
        gate(
            "completed_trades_at_least_20",
            input.completed_trades >= 20,
            format!("completed trades {}", input.completed_trades),
        ),
        gate(
            "cost_a_net_positive",
            input.cost_a_net > Decimal::ZERO,
            format!("net {}", input.cost_a_net),
        ),
        gate(
            "higher_cost_net_positive",
            input.higher_net > Decimal::ZERO,
            format!("net {}", input.higher_net),
        ),
        gate(
            "delayed_net_positive",
            input.delayed_net > Decimal::ZERO,
            format!("net {}", input.delayed_net),
        ),
        gate(
            "cost_a_beats_placebo",
            input.cost_a_net > input.placebo_cost_a_net,
            format!(
                "reversal {} vs continuation {}",
                input.cost_a_net, input.placebo_cost_a_net
            ),
        ),
    ];
    let concentration_passed = match input.largest_winner {
        Some(winner) => input.cost_a_net - winner > Decimal::ZERO,
        None => true,
    };
    let concentration_detail = match input.largest_winner {
        Some(winner) => format!(
            "net {} after removing winner {} is {}",
            input.cost_a_net,
            winner,
            input.cost_a_net - winner
        ),
        None => "no winning trade".to_string(),
    };
    gates.push(gate(
        "not_one_trade",
        concentration_passed,
        concentration_detail,
    ));
    gates.push(gate(
        "not_two_negative_entry_years",
        input.negative_entry_years < 2,
        format!(
            "{} of 2023/2024/2025 are negative",
            input.negative_entry_years
        ),
    ));
    gates
}

#[derive(Debug, Clone, Serialize)]
struct CellScore {
    sigma_multiple: String,
    hold_completed_hours: u32,
    completed_trades: usize,
    cost_a_net: Decimal,
    placebo_cost_a_net: Decimal,
    supports: bool,
    single_knob: bool,
}

enum RobustnessOutcome {
    Scored(Vec<CellScore>),
    MissingMinutes { count: usize },
}

fn score_robustness(
    hourly: &HourlyDataset,
    spec_sha: &str,
    human_sha: &str,
    market: &MarketInputs,
    output: &Path,
) -> Result<RobustnessOutcome, ShockError> {
    let multiples = ["2.5", "3", "3.5"];
    let holds = [3u32, 6, 12];
    let mut cells = Vec::new();
    let mut missing = BTreeSet::new();
    let scenarios = research_scenarios(PERFORMANCE_START_MS, PERFORMANCE_END_EXCLUSIVE_MS);
    let cost_a = scenarios
        .iter()
        .find(|scenario| scenario.name == "cost_a")
        .ok_or_else(|| ShockError::new("Cost A scenario is missing"))?;
    for multiple in multiples {
        for hold in holds {
            if multiple == "3" && hold == 6 {
                continue;
            }
            let reversal_params = Params::new(multiple, hold, SignMode::Reversal)
                .map_err(|err| ShockError::new(err.to_string()))?;
            let placebo_params = Params::new(multiple, hold, SignMode::Continuation)
                .map_err(|err| ShockError::new(err.to_string()))?;
            let (reversal, _) = replay(hourly, reversal_params, spec_sha, human_sha)?;
            let (placebo, _) = replay(hourly, placebo_params, spec_sha, human_sha)?;
            let reversal_changes = to_changes(&reversal.decisions);
            let placebo_changes = to_changes(&placebo.decisions);
            let mut combined = reversal_changes.clone();
            combined.extend(placebo_changes.iter().cloned());
            let needed = required_execution_minutes(&combined, &scenarios)
                .map_err(|err| ShockError::new(err.to_string()))?;
            for minute in &needed {
                if !market.minutes.contains_key(minute) {
                    missing.insert(*minute);
                }
            }
            cells.push((
                multiple.to_string(),
                hold,
                reversal,
                placebo,
                reversal_changes,
                placebo_changes,
            ));
        }
    }
    if !missing.is_empty() {
        let count = missing.len();
        let times: Vec<u64> = missing.into_iter().collect();
        write_json(&output.join("robustness_minutes_missing.json"), &times)?;
        return Ok(RobustnessOutcome::MissingMinutes { count });
    }
    let mut scores = Vec::new();
    for (multiple, hold, reversal, placebo, reversal_changes, placebo_changes) in cells {
        let reversal_sim = score(
            output,
            &format!("robustness/s{multiple}_h{hold}/reversal"),
            &reversal,
            &reversal_changes,
            cost_a,
            market,
        )?;
        let placebo_sim = score(
            output,
            &format!("robustness/s{multiple}_h{hold}/placebo"),
            &placebo,
            &placebo_changes,
            cost_a,
            market,
        )?;
        let supports = reversal_sim.summary.completed_trades >= 20
            && reversal_sim.summary.net_pnl > Decimal::ZERO
            && reversal_sim.summary.net_pnl > placebo_sim.summary.net_pnl;
        let single_knob = (multiple.as_str() == "2.5" && hold == 6)
            || (multiple.as_str() == "3.5" && hold == 6)
            || (multiple.as_str() == "3" && hold == 3)
            || (multiple.as_str() == "3" && hold == 12);
        scores.push(CellScore {
            sigma_multiple: multiple,
            hold_completed_hours: hold,
            completed_trades: reversal_sim.summary.completed_trades,
            cost_a_net: reversal_sim.summary.net_pnl,
            placebo_cost_a_net: placebo_sim.summary.net_pnl,
            supports,
            single_knob,
        });
    }
    Ok(RobustnessOutcome::Scored(scores))
}

fn neighborhood_passes(cells: &[CellScore]) -> bool {
    let supporters = cells
        .iter()
        .filter(|cell| cell.single_knob && cell.supports)
        .count();
    supporters >= 3
}

fn load_matching_log(
    path: &Path,
    hourly: &HourlyDataset,
    spec_sha: &str,
    human_sha: &str,
    sign: SignMode,
    multiple: &str,
    hold: u32,
) -> Result<DecisionLog, ShockError> {
    let log = super::replay::load_decision_log(path)?;
    let params =
        Params::new(multiple, hold, sign).map_err(|err| ShockError::new(err.to_string()))?;
    let expected_hash = config_hash(params).map_err(|err| ShockError::new(err.to_string()))?;
    if log.dataset_hash != hourly.metadata.dataset_hash
        || log.spec_sha256 != spec_sha
        || log.human_spec_sha256 != human_sha
        || log.sign != sign.as_str()
        || log.config_hash != expected_hash
    {
        return Err(ShockError::new(format!(
            "{} does not match the hourly dataset, spec, or frozen primary config",
            path.display()
        )));
    }
    Ok(log)
}

fn load_market(minutes: &Path, mark: &Path, funding: &Path) -> Result<MarketInputs, ShockError> {
    let trades = load_price_series(minutes).map_err(|err| ShockError::new(err.to_string()))?;
    let marks = load_price_series(mark).map_err(|err| ShockError::new(err.to_string()))?;
    let funding_file =
        load_funding_file(funding).map_err(|err| ShockError::new(err.to_string()))?;
    if marks.metadata.dataset_hash != FROZEN_MARK_60_HASH {
        return Err(ShockError::new(
            "mark file is not the frozen research mark set",
        ));
    }
    if funding_file.metadata.dataset_hash != FROZEN_FUNDING_HASH {
        return Err(ShockError::new(
            "funding file is not the frozen research funding set",
        ));
    }
    if !trades.metadata.gaps.is_empty() {
        return Err(ShockError::new(
            "execution minute file records gaps; refusing to simulate",
        ));
    }
    let times: Vec<u64> = funding_file
        .records
        .iter()
        .map(|row| row.settlement_time_ms)
        .collect();
    require_funding_grid(&times, PERFORMANCE_START_MS, PERFORMANCE_END_EXCLUSIVE_MS)?;
    market_inputs_from_files(&trades, &funding_file, &marks, None)
        .map_err(|err| ShockError::new(err.to_string()))
}

fn score(
    output: &Path,
    folder: &str,
    log: &DecisionLog,
    changes: &[TargetChange],
    scenario: &ExecutionScenario,
    market: &MarketInputs,
) -> Result<Simulation, ShockError> {
    let direction = log
        .decisions
        .last()
        .map(|row| {
            if row.target_qty > Decimal::ZERO {
                "long"
            } else if row.target_qty < Decimal::ZERO {
                "short"
            } else {
                "flat"
            }
        })
        .unwrap_or("flat");
    let mut simulation = simulate(&crate::donchian_daily_v1::backtest::SimulateRequest {
        book: log.sign.as_str(),
        book_note: "BTCLiquidationShockV1 target changes from the frozen hourly rule",
        model_direction_at_end: direction,
        signal_config_hash: &log.config_hash,
        changes,
        scenario,
        minutes: &market.minutes,
        funding: &market.funding,
        hourly_marks: &market.hourly_marks,
        finer_marks: &market.finer_marks,
    })
    .map_err(|err| ShockError::new(err.to_string()))?;
    simulation.summary.daily_dataset_hash = log.dataset_hash.clone();
    simulation.summary.coverage = market.coverage.clone();
    write_simulation(&output.join(folder).join(&scenario.name), &simulation)
        .map_err(|err| ShockError::new(err.to_string()))?;
    Ok(simulation)
}

fn score_passive(
    output: &Path,
    changes: &[TargetChange],
    scenario: &ExecutionScenario,
    market: &MarketInputs,
    dataset_hash: &str,
) -> Result<Simulation, ShockError> {
    let mut simulation = simulate(&crate::donchian_daily_v1::backtest::SimulateRequest {
        book: Book::FixedLongBenchmark.as_str(),
        book_note: "always-on long 0.001 BTC, exposure-scaled in the report",
        model_direction_at_end: "long",
        signal_config_hash: "fixed_long_0.001_btc",
        changes,
        scenario,
        minutes: &market.minutes,
        funding: &market.funding,
        hourly_marks: &market.hourly_marks,
        finer_marks: &market.finer_marks,
    })
    .map_err(|err| ShockError::new(err.to_string()))?;
    simulation.summary.daily_dataset_hash = dataset_hash.to_string();
    simulation.summary.coverage = market.coverage.clone();
    write_simulation(&output.join("passive").join(&scenario.name), &simulation)
        .map_err(|err| ShockError::new(err.to_string()))?;
    Ok(simulation)
}

fn to_changes(decisions: &[DecisionRow]) -> Vec<TargetChange> {
    decisions
        .iter()
        .map(|row| TargetChange {
            decision_id: row.decision_id.clone(),
            source_start_ms: Some(row.open_time_ms),
            source_end_ms: Some(row.decision_time_ms),
            decision_time_ms: row.decision_time_ms,
            target_qty: row.target_qty,
            reason: row.reason.clone(),
        })
        .collect()
}

pub fn execution_minutes_for(logs: &[DecisionLog]) -> Result<BTreeSet<u64>, ShockError> {
    let scenarios = research_scenarios(PERFORMANCE_START_MS, PERFORMANCE_END_EXCLUSIVE_MS);
    let mut changes = Vec::new();
    for log in logs {
        changes.extend(to_changes(&log.decisions));
    }
    changes.extend(
        changes_for(
            Book::FixedLongBenchmark,
            &[],
            PERFORMANCE_START_MS,
            position_qty(),
        )
        .map_err(|err| ShockError::new(err.to_string()))?,
    );
    required_execution_minutes(&changes, &scenarios).map_err(|err| ShockError::new(err.to_string()))
}

pub async fn download_execution_minutes(
    required: &BTreeSet<u64>,
    output: &Path,
    retrieved_at_ms: u64,
) -> Result<PriceSeriesFile, ShockError> {
    let request = AuxDownloadRequest {
        environment: MarketDataEnvironment::Mainnet,
        performance_start_ms: PERFORMANCE_START_MS,
        performance_end_exclusive_ms: PERFORMANCE_END_EXCLUSIVE_MS,
        retrieved_at_ms,
        position_qty: position_qty(),
    };
    let file = download_trade_minutes_for(
        &request,
        required.clone(),
        "btc_liquidation_shock_v1_trade_1m",
    )
    .await
    .map_err(|err| ShockError::new(err.to_string()))?;
    if !file.metadata.gaps.is_empty() {
        return Err(ShockError::new(format!(
            "missing {} execution minutes; the minute file was not written",
            file.metadata.gaps.len()
        )));
    }
    write_json(output, &file)?;
    Ok(file)
}

fn find_sim<'a>(sims: &'a [Simulation], name: &str) -> Result<&'a Simulation, ShockError> {
    sims.iter()
        .find(|simulation| simulation.summary.scenario_name == name)
        .ok_or_else(|| ShockError::new(format!("missing scenario {name}")))
}

fn decision_times(decisions: &[DecisionRow]) -> BTreeMap<String, u64> {
    decisions
        .iter()
        .map(|row| (row.decision_id.clone(), row.decision_time_ms))
        .collect()
}

fn entry_year_nets(
    trades: &[TradeRecord],
    decision_times: &BTreeMap<String, u64>,
) -> Result<BTreeMap<i32, Decimal>, ShockError> {
    let mut nets = BTreeMap::new();
    for trade in trades {
        let time = decision_times
            .get(&trade.entry_decision_id)
            .ok_or_else(|| {
                ShockError::new(format!(
                    "trade entry {} has no decision timestamp",
                    trade.entry_decision_id
                ))
            })?;
        let year = Utc
            .timestamp_millis_opt(*time as i64)
            .single()
            .map(|time| time.year())
            .ok_or_else(|| ShockError::new(format!("invalid entry timestamp {time}")))?;
        *nets.entry(year).or_insert(Decimal::ZERO) += trade.net_pnl;
    }
    Ok(nets)
}

fn gate(name: &str, passed: bool, detail: String) -> GateOutcome {
    GateOutcome {
        gate: name.to_string(),
        passed,
        detail,
    }
}

fn scaled_passive(candidate: &Simulation, passive: &Simulation) -> Decimal {
    let candidate_exposure: Decimal = candidate
        .pnl
        .iter()
        .map(|sample| sample.position.abs() * sample.mark_price)
        .sum();
    let passive_exposure: Decimal = passive
        .pnl
        .iter()
        .map(|sample| sample.position.abs() * sample.mark_price)
        .sum();
    if passive_exposure == Decimal::ZERO {
        Decimal::ZERO
    } else {
        passive.summary.net_pnl * candidate_exposure / passive_exposure
    }
}

#[derive(Debug, Clone, Serialize)]
struct BookLine {
    net_pnl: Decimal,
    annual_net_pnl: Decimal,
    completed_trades: usize,
    win_rate: String,
    mean_trade_pnl: Decimal,
    median_trade_pnl: Decimal,
    max_drawdown_usdt: Decimal,
    gross_turnover_usdt: Decimal,
    time_in_market_ms: u64,
    average_abs_notional_usdt: Decimal,
    long_net_pnl: Decimal,
    short_net_pnl: Decimal,
    fees: Decimal,
    funding: Decimal,
}

fn book_line(simulation: &Simulation) -> BookLine {
    let mut trade_nets: Vec<Decimal> = simulation
        .trades
        .iter()
        .map(|trade| trade.net_pnl)
        .collect();
    let mean = if trade_nets.is_empty() {
        Decimal::ZERO
    } else {
        trade_nets.iter().copied().sum::<Decimal>() / Decimal::from(trade_nets.len())
    };
    let median = median_decimal(&mut trade_nets);
    let turnover = simulation
        .fills
        .iter()
        .map(|fill| fill.quantity.abs() * fill.fill_price)
        .sum();
    let mut notionals = BTreeMap::new();
    for sample in &simulation.pnl {
        if sample.observation_time_ms.is_multiple_of(HOUR_MS) {
            notionals.insert(
                sample.observation_time_ms,
                sample.position.abs() * sample.mark_price,
            );
        }
    }
    let average_notional = if notionals.is_empty() {
        Decimal::ZERO
    } else {
        notionals.values().copied().sum::<Decimal>() / Decimal::from(notionals.len())
    };
    let window = PERFORMANCE_END_EXCLUSIVE_MS - PERFORMANCE_START_MS;
    BookLine {
        net_pnl: simulation.summary.net_pnl,
        annual_net_pnl: simulation.summary.net_pnl * Decimal::from(YEAR_MS) / Decimal::from(window),
        completed_trades: simulation.summary.completed_trades,
        win_rate: simulation
            .summary
            .win_rate
            .value
            .map(|value| value.to_string())
            .unwrap_or_else(|| simulation.summary.win_rate.status.clone()),
        mean_trade_pnl: mean,
        median_trade_pnl: median,
        max_drawdown_usdt: simulation.summary.max_drawdown_usdt,
        gross_turnover_usdt: turnover,
        time_in_market_ms: simulation.summary.time_in_market_ms,
        average_abs_notional_usdt: average_notional,
        long_net_pnl: simulation.summary.long_net_pnl,
        short_net_pnl: simulation.summary.short_net_pnl,
        fees: simulation.summary.fees,
        funding: simulation.summary.funding,
    }
}

fn median_decimal(values: &mut [Decimal]) -> Decimal {
    if values.is_empty() {
        return Decimal::ZERO;
    }
    values.sort();
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / Decimal::from(2)
    }
}

#[derive(Debug, Clone, Serialize)]
struct YearNet {
    year: i32,
    net: Decimal,
}

#[derive(Debug, Clone, Serialize)]
struct TercileSplit {
    low_net: Decimal,
    medium_net: Decimal,
    high_net: Decimal,
    low_max_sigma: f64,
    medium_max_sigma: f64,
}

fn volatility_terciles(trades: &[TradeRecord], decisions: &[DecisionRow]) -> Option<TercileSplit> {
    let sigma_by_id: BTreeMap<&str, f64> = decisions
        .iter()
        .map(|row| (row.decision_id.as_str(), row.sigma))
        .collect();
    let mut rows: Vec<(f64, Decimal)> = trades
        .iter()
        .filter_map(|trade| {
            sigma_by_id
                .get(trade.entry_decision_id.as_str())
                .copied()
                .map(|sigma| (sigma, trade.net_pnl))
        })
        .collect();
    if rows.len() < 3 {
        return None;
    }
    rows.sort_by(|left, right| left.0.total_cmp(&right.0));
    let cut_1 = rows.len() / 3;
    let cut_2 = 2 * rows.len() / 3;
    let low_max = rows[cut_1.saturating_sub(1)].0;
    let medium_max = rows[cut_2.saturating_sub(1)].0;
    let mut low = Decimal::ZERO;
    let mut medium = Decimal::ZERO;
    let mut high = Decimal::ZERO;
    for (sigma, net) in rows {
        if sigma <= low_max {
            low += net;
        } else if sigma <= medium_max {
            medium += net;
        } else {
            high += net;
        }
    }
    Some(TercileSplit {
        low_net: low,
        medium_net: medium,
        high_net: high,
        low_max_sigma: low_max,
        medium_max_sigma: medium_max,
    })
}

#[derive(Debug, Clone, Serialize)]
struct SummaryReport {
    candidate_id: String,
    disposition: String,
    revision: String,
    spec_sha256: String,
    human_spec_sha256: String,
    dataset_hash: String,
    gates: Vec<GateOutcome>,
    robustness_note: Option<String>,
    robustness: Option<Vec<CellScore>>,
    terciles: Option<TercileSplit>,
    reversal_cost_a: BookLine,
    placebo_cost_a: BookLine,
    passive_cost_a: BookLine,
    passive_scaled_net: Decimal,
    candidate_2_cost_a_net_context: String,
    entry_year_nets: Vec<YearNet>,
}

fn render_report(report: &SummaryReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# BTCLiquidationShockV1\n\nDisposition: {}\n\nRevision: {}\n\n",
        report.disposition, report.revision
    ));
    out.push_str(
        "Cost A is the synthetic 6 bps + 2 bps + 10 s research case, not a Bybit fee quote.\n\n",
    );
    out.push_str(&format!(
        "Reversal Cost A net {} (annualized {} over the window), trades {}, drawdown {}, turnover {}, time in market {} ms, average absolute notional {}.\n\n",
        report.reversal_cost_a.net_pnl,
        report.reversal_cost_a.annual_net_pnl,
        report.reversal_cost_a.completed_trades,
        report.reversal_cost_a.max_drawdown_usdt,
        report.reversal_cost_a.gross_turnover_usdt,
        report.reversal_cost_a.time_in_market_ms,
        report.reversal_cost_a.average_abs_notional_usdt
    ));
    out.push_str(&format!(
        "Long-shock net {} / short-shock net {} under Cost A. Mean trade {} / median {}.\n\n",
        report.reversal_cost_a.long_net_pnl,
        report.reversal_cost_a.short_net_pnl,
        report.reversal_cost_a.mean_trade_pnl,
        report.reversal_cost_a.median_trade_pnl
    ));
    out.push_str(&format!(
        "Placebo Cost A net {}. Exposure-scaled passive net {}. Candidate #2 primary Cost A net {} is context only.\n\n",
        report.placebo_cost_a.net_pnl, report.passive_scaled_net, report.candidate_2_cost_a_net_context
    ));
    for year in &report.entry_year_nets {
        out.push_str(&format!("Entry year {}: {}\n", year.year, year.net));
    }
    out.push('\n');
    for gate in &report.gates {
        out.push_str(&format!(
            "- {}: {} ({})\n",
            gate.gate,
            if gate.passed { "pass" } else { "fail" },
            gate.detail
        ));
    }
    if let Some(note) = &report.robustness_note {
        out.push_str(&format!("\n{note}\n"));
    }
    if let Some(cells) = &report.robustness {
        out.push_str("\nRobustness:\n");
        for cell in cells {
            out.push_str(&format!(
                "- {}σ/{}h trades {} net {} placebo {} support {}\n",
                cell.sigma_multiple,
                cell.hold_completed_hours,
                cell.completed_trades,
                cell.cost_a_net,
                cell.placebo_cost_a_net,
                cell.supports
            ));
        }
    }
    if let Some(terciles) = &report.terciles {
        out.push_str(&format!(
            "\nTrailing-sigma terciles (descriptive): low {} medium {} high {}.\n",
            terciles.low_net, terciles.medium_net, terciles.high_net
        ));
    }
    out
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), ShockError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| ShockError::new(format!("create {}: {err}", parent.display())))?;
        }
    }
    let body = serde_json::to_vec_pretty(value)
        .map_err(|err| ShockError::new(format!("encode {}: {err}", path.display())))?;
    std::fs::write(path, body)
        .map_err(|err| ShockError::new(format!("write {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn fewer_than_20_trades_fails_before_any_economic_excuse() {
        let gates = evaluate_promotion(&PromotionInput {
            completed_trades: 19,
            cost_a_net: dec!(1),
            higher_net: dec!(1),
            delayed_net: dec!(1),
            placebo_cost_a_net: dec!(0),
            largest_winner: Some(dec!(0.1)),
            negative_entry_years: 0,
        });
        assert!(!gates.iter().all(|gate| gate.passed));
        assert!(!gates[0].passed);
    }

    #[test]
    fn one_winner_that_explains_the_net_fails() {
        let gates = evaluate_promotion(&PromotionInput {
            completed_trades: 20,
            cost_a_net: dec!(5),
            higher_net: dec!(1),
            delayed_net: dec!(1),
            placebo_cost_a_net: dec!(0),
            largest_winner: Some(dec!(5)),
            negative_entry_years: 0,
        });
        let concentration = gates
            .iter()
            .find(|gate| gate.gate == "not_one_trade")
            .unwrap();
        assert!(!concentration.passed);
    }

    #[test]
    fn three_single_knob_supporters_pass_the_neighborhood() {
        let cell = |knob, supports| CellScore {
            sigma_multiple: "3".to_string(),
            hold_completed_hours: 6,
            completed_trades: 20,
            cost_a_net: dec!(1),
            placebo_cost_a_net: dec!(0),
            supports,
            single_knob: knob,
        };
        let cells = vec![
            cell(true, true),
            cell(true, true),
            cell(true, true),
            cell(true, false),
        ];
        assert!(neighborhood_passes(&cells));
        assert!(!neighborhood_passes(&cells[..2]));
    }
}
