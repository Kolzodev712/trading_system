//! BTCPriceMALongFlatV1 — daily log-price vs geometric mean, long or flat only.
//!
//! Offline research only. Does not submit orders.

pub mod experiment;
pub mod gates;
pub mod offline;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

pub const STRATEGY_NAME: &str = "BTCPriceMALongFlatV1";
pub const STRATEGY_VERSION: &str = "1";
pub const PRIMARY_LOOKBACK: usize = 28;
pub const DIAGNOSTIC_LOOKBACKS: [usize; 2] = [14, 70];

pub const PERFORMANCE_START_MS: u64 = 1_674_259_200_000;
pub const PERFORMANCE_END_EXCLUSIVE_MS: u64 = 1_767_225_600_000;

pub fn position_qty() -> Decimal {
    Decimal::new(1, 3)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Flat,
    Long,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelState {
    pub direction: Direction,
    pub requested_qty: Decimal,
}

impl Default for ModelState {
    fn default() -> Self {
        Self {
            direction: Direction::Flat,
            requested_qty: Decimal::ZERO,
        }
    }
}

impl ModelState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalDiagnostics {
    pub close: Decimal,
    pub geometric_mean: Decimal,
    pub log_spread: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    NotReady {
        have: usize,
        need: usize,
    },
    NoChange {
        diagnostics: SignalDiagnostics,
        reason: &'static str,
    },
    Transition {
        target_qty: Decimal,
        diagnostics: SignalDiagnostics,
        reason: &'static str,
    },
}

pub fn decimal_ln(value: Decimal) -> Result<f64, String> {
    if value <= Decimal::ZERO {
        return Err(format!("log is undefined for non-positive price {value}"));
    }
    value
        .to_f64()
        .ok_or_else(|| format!("price {value} does not fit in f64"))
        .map(|v| v.ln())
}

/// `closes` must contain exactly `lookback` completed closes ending with the current candle.
pub fn log_spread(closes: &[Decimal]) -> Result<f64, String> {
    let n = closes.len();
    if n == 0 {
        return Err("empty close window".to_string());
    }
    let sum: f64 = closes
        .iter()
        .map(|c| decimal_ln(*c))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    let current = decimal_ln(*closes.last().expect("non-empty"))?;
    Ok(current - sum / n as f64)
}

pub fn geometric_mean(closes: &[Decimal]) -> Result<Decimal, String> {
    let n = closes.len();
    let sum: f64 = closes
        .iter()
        .map(|c| decimal_ln(*c))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    let mean_ln = sum / n as f64;
    Decimal::from_f64_retain(mean_ln.exp()).ok_or_else(|| "geometric mean overflowed".to_string())
}

pub fn wants_long(closes: &[Decimal]) -> Result<bool, String> {
    Ok(log_spread(closes)? > 0.0)
}

pub fn on_candle(
    state: &ModelState,
    lookback: usize,
    window: &[Decimal],
) -> Result<(ModelState, Step), String> {
    if window.len() != lookback {
        return Err(format!("expected {lookback} closes, got {}", window.len()));
    }
    let close = *window.last().expect("window");
    let geom = geometric_mean(window)?;
    let spread = log_spread(window)?;
    let diagnostics = SignalDiagnostics {
        close,
        geometric_mean: geom,
        log_spread: spread,
    };
    if spread <= 0.0 {
        let target = Decimal::ZERO;
        if state.requested_qty == target {
            return Ok((
                state.clone(),
                Step::NoChange {
                    diagnostics,
                    reason: "at or below geometric mean; remain flat",
                },
            ));
        }
        return Ok((
            ModelState {
                direction: Direction::Flat,
                requested_qty: target,
            },
            Step::Transition {
                target_qty: target,
                diagnostics,
                reason: "at or below geometric mean; exit to flat",
            },
        ));
    }
    let target = position_qty();
    if state.requested_qty == target {
        return Ok((
            state.clone(),
            Step::NoChange {
                diagnostics,
                reason: "above geometric mean; retain long",
            },
        ));
    }
    Ok((
        ModelState {
            direction: Direction::Long,
            requested_qty: target,
        },
        Step::Transition {
            target_qty: target,
            diagnostics,
            reason: "above geometric mean; enter long",
        },
    ))
}
