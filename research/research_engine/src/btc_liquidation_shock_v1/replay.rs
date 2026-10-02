//! Decision replay. No fills and no network.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use strategy_core::btc_liquidation_shock_v1::{
    config_hash, on_hour, ModelState, Params, SignMode, Step, CLOSES_REQUIRED, STRATEGY_NAME,
    STRATEGY_VERSION,
};

use super::dataset::{HourlyDataset, PERFORMANCE_END_EXCLUSIVE_MS, PERFORMANCE_START_MS};
use super::ShockError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRow {
    pub decision_id: String,
    pub open_time_ms: u64,
    pub decision_time_ms: u64,
    pub close: Decimal,
    pub log_return: f64,
    pub sigma: f64,
    pub target_qty: Decimal,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRow {
    pub open_time_ms: u64,
    pub decision_time_ms: u64,
    pub log_return: f64,
    pub sigma: f64,
    pub z: f64,
    pub target_after: Decimal,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionLog {
    pub strategy_id: String,
    pub strategy_version: String,
    pub sign: String,
    pub sigma_multiple: String,
    pub hold_completed_hours: u32,
    pub config_hash: String,
    pub dataset_hash: String,
    pub spec_sha256: String,
    pub human_spec_sha256: String,
    pub performance_start_ms: u64,
    pub performance_end_exclusive_ms: u64,
    pub decisions: Vec<DecisionRow>,
}

pub fn replay(
    dataset: &HourlyDataset,
    params: Params,
    spec_sha256: &str,
    human_spec_sha256: &str,
) -> Result<(DecisionLog, Vec<EventRow>), ShockError> {
    super::dataset::verify_hourly_dataset(dataset)?;
    let hash = config_hash(params).map_err(|err| ShockError::new(err.to_string()))?;
    let first = dataset
        .candles
        .iter()
        .position(|candle| candle.start_time_ms == super::dataset::first_eligible_open_ms())
        .ok_or_else(|| ShockError::new("first eligible hour is missing"))?;
    let last = dataset
        .candles
        .iter()
        .position(|candle| candle.start_time_ms == super::dataset::last_eligible_open_ms())
        .ok_or_else(|| ShockError::new("last eligible hour is missing"))?;
    if first + 1 < CLOSES_REQUIRED {
        return Err(ShockError::new(
            "warmup does not contain 170 closes before the first eligible hour",
        ));
    }
    let mut state = ModelState::new();
    let mut decisions = Vec::new();
    let mut events = Vec::new();
    for index in first..=last {
        let start = index + 1 - CLOSES_REQUIRED;
        let window = &dataset.candles[start..=index];
        let closes: Vec<Decimal> = window.iter().map(|candle| candle.close).collect();
        let candle = &dataset.candles[index];
        let decision_time_ms = candle
            .start_time_ms
            .checked_add(super::dataset::HOUR_MS)
            .ok_or_else(|| ShockError::new("decision timestamp overflowed"))?;
        let (next, step) =
            on_hour(&state, params, &closes).map_err(|err| ShockError::new(err.to_string()))?;
        let diagnostics = match &step {
            Step::NoChange { diagnostics, .. } | Step::Transition { diagnostics, .. } => {
                diagnostics
            }
        };
        let action = match &step {
            Step::Transition { target_qty, .. } if *target_qty == Decimal::ZERO => "exit",
            Step::Transition { .. } => "enter",
            Step::NoChange { .. } if !state.is_flat() => "hold",
            Step::NoChange { .. } => "flat",
        };
        if let Step::Transition {
            target_qty, reason, ..
        } = &step
        {
            decisions.push(DecisionRow {
                decision_id: format!("{}-{decision_time_ms}", params.sign.as_str()),
                open_time_ms: candle.start_time_ms,
                decision_time_ms,
                close: candle.close,
                log_return: diagnostics.log_return,
                sigma: diagnostics.sigma,
                target_qty: *target_qty,
                reason: (*reason).to_string(),
            });
        }
        let z = diagnostics.log_return / diagnostics.sigma;
        events.push(EventRow {
            open_time_ms: candle.start_time_ms,
            decision_time_ms,
            log_return: diagnostics.log_return,
            sigma: diagnostics.sigma,
            z,
            target_after: next.target_qty,
            action: action.to_string(),
        });
        state = next;
    }
    if !state.is_flat() {
        decisions.push(DecisionRow {
            decision_id: format!(
                "{}-pending-{}",
                params.sign.as_str(),
                PERFORMANCE_END_EXCLUSIVE_MS
            ),
            open_time_ms: PERFORMANCE_END_EXCLUSIVE_MS,
            decision_time_ms: PERFORMANCE_END_EXCLUSIVE_MS,
            close: Decimal::ZERO,
            log_return: 0.0,
            sigma: 0.0,
            target_qty: Decimal::ZERO,
            reason: "hold did not complete inside the performance window; no fill is invented"
                .to_string(),
        });
    }
    let multiple = params.sigma_multiple.normalize().to_string();
    Ok((
        DecisionLog {
            strategy_id: STRATEGY_NAME.to_string(),
            strategy_version: STRATEGY_VERSION.to_string(),
            sign: params.sign.as_str().to_string(),
            sigma_multiple: multiple,
            hold_completed_hours: params.hold_hours,
            config_hash: hash,
            dataset_hash: dataset.metadata.dataset_hash.clone(),
            spec_sha256: spec_sha256.to_string(),
            human_spec_sha256: human_spec_sha256.to_string(),
            performance_start_ms: PERFORMANCE_START_MS,
            performance_end_exclusive_ms: PERFORMANCE_END_EXCLUSIVE_MS,
            decisions,
        },
        events,
    ))
}

pub fn save_decision_log(path: &std::path::Path, log: &DecisionLog) -> Result<(), ShockError> {
    write_json(path, log)
}

pub fn load_decision_log(path: &std::path::Path) -> Result<DecisionLog, ShockError> {
    let body = std::fs::read(path)
        .map_err(|err| ShockError::new(format!("read {}: {err}", path.display())))?;
    serde_json::from_slice(&body)
        .map_err(|err| ShockError::new(format!("decode {}: {err}", path.display())))
}

pub fn save_events(path: &std::path::Path, events: &[EventRow]) -> Result<(), ShockError> {
    write_json(path, &events)
}

fn write_json(path: &std::path::Path, value: &impl Serialize) -> Result<(), ShockError> {
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

pub fn params_for(multiple: &str, hold_hours: u32, sign: SignMode) -> Result<Params, ShockError> {
    Params::new(multiple, hold_hours, sign).map_err(|err| ShockError::new(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btc_liquidation_shock_v1::dataset::{
        dataset_start_ms, first_eligible_open_ms, from_candles, last_eligible_open_ms,
        HourlyCandle, HOUR_MS,
    };
    use rust_decimal_macros::dec;
    use strategy_core::btc_liquidation_shock_v1::position_qty;

    fn crash_dataset() -> crate::btc_liquidation_shock_v1::dataset::HourlyDataset {
        let mut candles = Vec::new();
        let mut cursor = dataset_start_ms();
        let crash_open = first_eligible_open_ms();
        while cursor <= last_eligible_open_ms() {
            let close = if cursor == crash_open {
                dec!(50)
            } else if (cursor / HOUR_MS).is_multiple_of(2) {
                dec!(100)
            } else {
                dec!(101)
            };
            candles.push(HourlyCandle {
                start_time_ms: cursor,
                open: dec!(100),
                high: dec!(110),
                low: dec!(40),
                close,
                volume: dec!(1),
            });
            cursor += HOUR_MS;
        }
        from_candles(candles, PERFORMANCE_END_EXCLUSIVE_MS).unwrap()
    }

    #[test]
    fn crash_enters_long_and_exits_six_hours_later() {
        let dataset = crash_dataset();
        let params = Params::primary().unwrap();
        let (log, _) = replay(&dataset, params, "sha256:spec", "sha256:human").unwrap();
        assert_eq!(log.decisions[0].target_qty, position_qty());
        assert_eq!(
            log.decisions[0].decision_time_ms,
            first_eligible_open_ms() + HOUR_MS
        );
        assert_eq!(
            log.decisions[1].decision_time_ms,
            log.decisions[0].decision_time_ms + 6 * HOUR_MS
        );
        assert_eq!(log.decisions[1].target_qty, Decimal::ZERO);
    }
}
