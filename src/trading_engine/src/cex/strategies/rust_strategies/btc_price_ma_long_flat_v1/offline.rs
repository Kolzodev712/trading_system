//! Offline replay for BTCPriceMALongFlatV1 from merged daily datasets.

use super::{
    model_config_hash, on_candle, position_qty, Direction, ModelState, Step,
    PERFORMANCE_END_EXCLUSIVE_MS, PERFORMANCE_START_MS, STRATEGY_NAME, STRATEGY_VERSION,
};
use crate::cex::strategies::rust_strategies::donchian_daily_v1::offline::{
    completed_series, window_series, DatasetCandle, DatasetMetadata, DecisionLog, DecisionRow,
    EmittedTarget, HistoricalDataset, OfflineError, RunMetadata,
};
use rust_decimal::Decimal;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRequest {
    pub lookback: usize,
    pub evaluation_start_ms: u64,
    pub evaluation_end_exclusive_ms: u64,
}

pub fn merge_datasets(
    warmup: &HistoricalDataset,
    main: &HistoricalDataset,
) -> Result<HistoricalDataset, OfflineError> {
    validate_chain(warmup, main)?;
    let mut candles = warmup.candles.clone();
    candles.extend(main.candles.clone());
    candles.sort_by_key(|c| c.start_time_ms);
    let mut unique: Vec<DatasetCandle> = Vec::new();
    for candle in candles {
        if let Some(prev) = unique.last() {
            if prev.start_time_ms == candle.start_time_ms {
                if prev == &candle {
                    continue;
                }
                return Err(OfflineError::new(format!(
                    "conflicting duplicate at {}",
                    candle.start_time_ms
                )));
            }
            if prev.end_time_exclusive_ms != candle.start_time_ms {
                return Err(OfflineError::new(format!(
                    "gap between {} and {}",
                    prev.end_time_exclusive_ms, candle.start_time_ms
                )));
            }
        }
        unique.push(candle);
    }
    Ok(HistoricalDataset {
        metadata: main.metadata.clone(),
        candles: unique,
    })
}

fn validate_chain(
    warmup: &HistoricalDataset,
    main: &HistoricalDataset,
) -> Result<(), OfflineError> {
    let warm_last = warmup
        .candles
        .iter()
        .max_by_key(|c| c.start_time_ms)
        .ok_or_else(|| OfflineError::new("warmup dataset is empty"))?;
    let main_first = main
        .candles
        .iter()
        .min_by_key(|c| c.start_time_ms)
        .ok_or_else(|| OfflineError::new("main dataset is empty"))?;
    if warm_last.end_time_exclusive_ms != main_first.start_time_ms {
        return Err(OfflineError::new(format!(
            "warmup ends at {} but main starts at {}",
            warm_last.end_time_exclusive_ms, main_first.start_time_ms
        )));
    }
    Ok(())
}

pub fn replay(
    dataset: &HistoricalDataset,
    request: &ReplayRequest,
) -> Result<DecisionLog, OfflineError> {
    if request.lookback == 0 {
        return Err(OfflineError::new("lookback must be positive"));
    }
    if request.evaluation_start_ms >= request.evaluation_end_exclusive_ms {
        return Err(OfflineError::new("evaluation window is empty"));
    }
    let completed = completed_series(dataset)?;
    let series = window_series(
        &completed,
        request.evaluation_start_ms,
        request.evaluation_end_exclusive_ms,
        request.lookback,
    )?;
    let qty = position_qty();
    let mut state = ModelState::new();
    let mut decisions = Vec::with_capacity(series.len());
    let lookback = request.lookback;
    for (index, candle) in series.iter().enumerate() {
        let previous = direction_name(state.direction);
        if index < lookback {
            decisions.push(not_ready_row(
                &dataset.metadata,
                candle,
                index + 1,
                lookback,
            ));
            continue;
        }
        let start = index + 1 - lookback;
        let window: Vec<Decimal> = series[start..=index].iter().map(|c| c.close).collect();
        let (next, step) = on_candle(&state, lookback, &window).map_err(OfflineError::new)?;
        decisions.push(decision_row(
            &dataset.metadata,
            candle,
            &previous,
            &next,
            &step,
        ));
        state = next;
    }
    let evaluated = &decisions[lookback..];
    let entries = evaluated.iter().filter(|row| is_entry(row)).count();
    let exits = evaluated.iter().filter(|row| is_exit(row)).count();
    Ok(DecisionLog {
        run: RunMetadata {
            strategy_id: STRATEGY_NAME.to_string(),
            strategy_version: STRATEGY_VERSION.to_string(),
            config_hash: config_hash(request.lookback)?,
            dataset_hash: dataset.metadata.dataset_hash.clone(),
            venue: dataset.metadata.venue.clone(),
            environment: dataset.metadata.environment.clone(),
            product: dataset.metadata.product.clone(),
            symbol: dataset.metadata.symbol.clone(),
            interval: dataset.metadata.interval.clone(),
            evaluation_start_ms: request.evaluation_start_ms,
            evaluation_end_exclusive_ms: request.evaluation_end_exclusive_ms,
            actual_start_ms: dataset.metadata.actual_start_ms,
            actual_end_exclusive_ms: dataset.metadata.actual_end_exclusive_ms,
            dataset_candles: dataset.metadata.candle_count,
            completed_candles: dataset.metadata.completed_candle_count,
            warmup_candles: lookback,
            evaluated_candles: evaluated.len(),
            entries,
            exits,
            validation: "ok".to_string(),
            final_direction: direction_name(state.direction),
            initial_state_policy: initial_state_policy(lookback),
            allow_short: false,
            position_qty: qty,
            entry_lookback: lookback,
            exit_lookback: 0,
        },
        decisions,
    })
}

pub fn save_decision_log(path: &Path, log: &DecisionLog) -> Result<(), OfflineError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| OfflineError::new(format!("create {}: {err}", parent.display())))?;
        }
    }
    let body = serde_json::to_vec_pretty(log)
        .map_err(|err| OfflineError::new(format!("encode decisions: {err}")))?;
    std::fs::write(path, body)
        .map_err(|err| OfflineError::new(format!("write {}: {err}", path.display())))
}

pub fn load_decision_log(path: &Path) -> Result<DecisionLog, OfflineError> {
    let body = std::fs::read(path)
        .map_err(|err| OfflineError::new(format!("read {}: {err}", path.display())))?;
    serde_json::from_slice(&body).map_err(|err| OfflineError::new(format!("decode: {err}")))
}

fn initial_state_policy(lookback: usize) -> String {
    format!(
        "Model and simulated account start flat. The {lookback} completed daily candles immediately before evaluation_start initialize the geometric mean only. No trades are simulated before evaluation_start."
    )
}

pub fn config_hash(lookback: usize) -> Result<String, OfflineError> {
    model_config_hash(lookback).map_err(OfflineError::new)
}

fn direction_name(direction: Direction) -> String {
    match direction {
        Direction::Flat => "flat",
        Direction::Long => "long",
    }
    .to_string()
}

fn not_ready_row(
    metadata: &DatasetMetadata,
    candle: &DatasetCandle,
    have: usize,
    need: usize,
) -> DecisionRow {
    DecisionRow {
        venue: metadata.venue.clone(),
        product: metadata.product.clone(),
        instrument: metadata.symbol.clone(),
        interval: metadata.interval.clone(),
        start_time_ms: candle.start_time_ms,
        end_time_exclusive_ms: candle.end_time_exclusive_ms,
        close: candle.close,
        entry_high: None,
        entry_low: None,
        exit_high: None,
        exit_low: None,
        previous_direction: "flat".to_string(),
        direction: "flat".to_string(),
        readiness: "not_ready".to_string(),
        suspension: None,
        reason: format!("not ready: {have} of {need} preceding candles"),
        target: EmittedTarget::NotReady,
    }
}

fn decision_row(
    metadata: &DatasetMetadata,
    candle: &DatasetCandle,
    previous: &str,
    next: &ModelState,
    step: &Step,
) -> DecisionRow {
    let (readiness, reason, target, geom, spread) = match step {
        Step::NotReady { have, need } => (
            "not_ready",
            format!("not ready: {have} of {need} preceding candles"),
            EmittedTarget::NotReady,
            None,
            None,
        ),
        Step::NoChange {
            diagnostics,
            reason,
        } => (
            "ready",
            format!("{reason}; log_spread={}", diagnostics.log_spread),
            EmittedTarget::NoChange,
            Some(diagnostics.geometric_mean),
            Some(Decimal::from_f64_retain(diagnostics.log_spread).unwrap_or(Decimal::ZERO)),
        ),
        Step::Transition {
            target_qty,
            diagnostics,
            reason,
        } => (
            "ready",
            format!("{reason}; log_spread={}", diagnostics.log_spread),
            EmittedTarget::Quantity {
                quantity: *target_qty,
            },
            Some(diagnostics.geometric_mean),
            Some(Decimal::from_f64_retain(diagnostics.log_spread).unwrap_or(Decimal::ZERO)),
        ),
    };
    DecisionRow {
        venue: metadata.venue.clone(),
        product: metadata.product.clone(),
        instrument: metadata.symbol.clone(),
        interval: metadata.interval.clone(),
        start_time_ms: candle.start_time_ms,
        end_time_exclusive_ms: candle.end_time_exclusive_ms,
        close: candle.close,
        entry_high: geom,
        entry_low: spread,
        exit_high: None,
        exit_low: None,
        previous_direction: previous.to_string(),
        direction: direction_name(next.direction),
        readiness: readiness.to_string(),
        suspension: None,
        reason,
        target,
    }
}

fn is_entry(row: &DecisionRow) -> bool {
    row.readiness == "ready"
        && row.previous_direction == "flat"
        && row.direction == "long"
        && matches!(row.target, EmittedTarget::Quantity { quantity } if quantity > Decimal::ZERO)
}

fn is_exit(row: &DecisionRow) -> bool {
    row.readiness == "ready"
        && row.previous_direction == "long"
        && row.direction == "flat"
        && matches!(row.target, EmittedTarget::Quantity { quantity } if quantity == Decimal::ZERO)
}

pub fn default_replay_request(lookback: usize) -> ReplayRequest {
    ReplayRequest {
        lookback,
        evaluation_start_ms: PERFORMANCE_START_MS,
        evaluation_end_exclusive_ms: PERFORMANCE_END_EXCLUSIVE_MS,
    }
}
