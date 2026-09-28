//! BTCPriceMALongFlatV1 — daily log-price vs geometric mean, long or flat only.
//!
//! Pure model. Callers supply completed close windows; this module does not
//! load datasets, simulate fills, or submit orders.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const STRATEGY_NAME: &str = "BTCPriceMALongFlatV1";
pub const STRATEGY_VERSION: &str = "1";
pub const PRIMARY_LOOKBACK: usize = 28;
pub const DIAGNOSTIC_LOOKBACKS: [usize; 2] = [14, 70];
pub const ALLOW_SHORT: bool = false;
pub const SIGNAL_ID: &str = "log_close_minus_geometric_mean_of_N_including_current";

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
    #[allow(dead_code)]
    NotReady { have: usize, need: usize },
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

#[derive(Serialize)]
struct ConfigHashPayload {
    strategy_id: &'static str,
    strategy_version: &'static str,
    lookback: usize,
    position_qty: Decimal,
    allow_short: bool,
    signal: &'static str,
}

/// Compact SHA-256 of the frozen identity fields. Semantic payload must stay stable.
pub fn config_hash(lookback: usize) -> Result<String, String> {
    let payload = ConfigHashPayload {
        strategy_id: STRATEGY_NAME,
        strategy_version: STRATEGY_VERSION,
        lookback,
        position_qty: position_qty(),
        allow_short: ALLOW_SHORT,
        signal: SIGNAL_ID,
    };
    let bytes = serde_json::to_vec(&payload).map_err(|err| format!("config hash: {err}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn equality_is_flat() {
        let closes = vec![dec!(100); 3];
        assert!(!wants_long(&closes).unwrap());
    }

    #[test]
    fn product_form_matches_log_spread() {
        let closes = vec![dec!(100), dec!(110), dec!(120), dec!(130)];
        assert!(wants_long(&closes).unwrap());
    }

    #[test]
    fn frozen_n28_config_hash() {
        assert_eq!(
            config_hash(28).unwrap(),
            "sha256:6e02ddef41a418dc26f5ea8acfd592417846362211b3f70de3ab553d41c3559a"
        );
    }
}
