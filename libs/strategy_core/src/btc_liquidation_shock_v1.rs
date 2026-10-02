//! BTCLiquidationShockV1 — hourly log-return shock, opposite target, fixed hold.
//!
//! Pure model. Callers supply completed trade closes. This module does not
//! load datasets, simulate fills, or submit orders. Liquidation prints are not
//! an input; the measured object is the hourly log return.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const STRATEGY_NAME: &str = "BTCLiquidationShockV1";
pub const STRATEGY_VERSION: &str = "1";
pub const PRIOR_RETURNS: usize = 168;
pub const CLOSES_REQUIRED: usize = PRIOR_RETURNS + 2;
pub const PRIMARY_SIGMA_MULTIPLE: &str = "3";
pub const PRIMARY_HOLD_HOURS: u32 = 6;
pub const PRIMARY_SIGN: SignMode = SignMode::Reversal;

pub fn position_qty() -> Decimal {
    Decimal::new(1, 3)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignMode {
    Reversal,
    Continuation,
}

impl SignMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reversal => "reversal",
            Self::Continuation => "continuation",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ModelError> {
        match value {
            "reversal" => Ok(Self::Reversal),
            "continuation" => Ok(Self::Continuation),
            _ => Err(ModelError::Config(format!("unknown sign mode {value}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    pub sigma_multiple: Decimal,
    pub hold_hours: u32,
    pub sign: SignMode,
}

impl Params {
    pub fn primary() -> Result<Self, ModelError> {
        Self::new(PRIMARY_SIGMA_MULTIPLE, PRIMARY_HOLD_HOURS, PRIMARY_SIGN)
    }

    pub fn new(sigma_multiple: &str, hold_hours: u32, sign: SignMode) -> Result<Self, ModelError> {
        let sigma_multiple: Decimal = sigma_multiple
            .parse()
            .map_err(|_| ModelError::Config(format!("invalid sigma multiple {sigma_multiple}")))?;
        if sigma_multiple <= Decimal::ZERO {
            return Err(ModelError::Config(
                "sigma multiple must be positive".to_string(),
            ));
        }
        if hold_hours == 0 {
            return Err(ModelError::Config(
                "hold must be at least one completed hour".to_string(),
            ));
        }
        Ok(Self {
            sigma_multiple,
            hold_hours,
            sign,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelState {
    pub target_qty: Decimal,
    pub hours_since_entry: u32,
}

impl Default for ModelState {
    fn default() -> Self {
        Self {
            target_qty: Decimal::ZERO,
            hours_since_entry: 0,
        }
    }
}

impl ModelState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_flat(self) -> bool {
        self.target_qty == Decimal::ZERO
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShockDiagnostics {
    pub close: Decimal,
    pub log_return: f64,
    pub sigma: f64,
    pub threshold: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    NoChange {
        diagnostics: ShockDiagnostics,
        reason: &'static str,
    },
    Transition {
        target_qty: Decimal,
        diagnostics: ShockDiagnostics,
        reason: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    Config(String),
    Window { expected: usize, got: usize },
    NonPositiveClose(String),
    NonFinite,
    ZeroSigma,
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(message) => formatter.write_str(message),
            Self::Window { expected, got } => {
                write!(formatter, "expected {expected} closes, got {got}")
            }
            Self::NonPositiveClose(message) => formatter.write_str(message),
            Self::NonFinite => formatter.write_str("return or volatility was non-finite"),
            Self::ZeroSigma => formatter.write_str("rolling volatility is zero"),
        }
    }
}

impl std::error::Error for ModelError {}

pub fn decimal_ln(value: Decimal) -> Result<f64, ModelError> {
    if value <= Decimal::ZERO {
        return Err(ModelError::NonPositiveClose(format!(
            "log is undefined for non-positive price {value}"
        )));
    }
    value
        .to_f64()
        .ok_or_else(|| ModelError::NonPositiveClose(format!("price {value} does not fit in f64")))
        .map(|value| value.ln())
}

fn log_return(previous: Decimal, current: Decimal) -> Result<f64, ModelError> {
    let value = decimal_ln(current)? - decimal_ln(previous)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ModelError::NonFinite)
    }
}

pub fn sample_stdev(values: &[f64]) -> Result<f64, ModelError> {
    if values.len() != PRIOR_RETURNS {
        return Err(ModelError::Window {
            expected: PRIOR_RETURNS,
            got: values.len(),
        });
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / (n - 1.0);
    let sigma = variance.sqrt();
    if !sigma.is_finite() {
        return Err(ModelError::NonFinite);
    }
    if sigma == 0.0 {
        return Err(ModelError::ZeroSigma);
    }
    Ok(sigma)
}

/// Target while flat. Negative shocks go with the reversal or continuation sign.
pub fn shock_target(
    current_return: f64,
    prior_returns: &[f64],
    params: Params,
) -> Result<(Decimal, ShockDiagnostics), ModelError> {
    if !current_return.is_finite() {
        return Err(ModelError::NonFinite);
    }
    let sigma = sample_stdev(prior_returns)?;
    let multiple = params
        .sigma_multiple
        .to_f64()
        .ok_or(ModelError::NonFinite)?;
    let threshold = multiple * sigma;
    if !threshold.is_finite() {
        return Err(ModelError::NonFinite);
    }
    let qty = position_qty();
    let target = if current_return <= -threshold {
        match params.sign {
            SignMode::Reversal => qty,
            SignMode::Continuation => -qty,
        }
    } else if current_return >= threshold {
        match params.sign {
            SignMode::Reversal => -qty,
            SignMode::Continuation => qty,
        }
    } else {
        Decimal::ZERO
    };
    Ok((
        target,
        ShockDiagnostics {
            close: Decimal::ZERO,
            log_return: current_return,
            sigma,
            threshold,
        },
    ))
}

/// `closes` must be exactly 170 completed trade closes ending at the current hour.
///
/// A position exits on the `hold_hours`-th later close and does not enter again
/// on that same close. One decision per hour keeps the exit and the next entry
/// from sharing a timestamp.
pub fn on_hour(
    state: &ModelState,
    params: Params,
    closes: &[Decimal],
) -> Result<(ModelState, Step), ModelError> {
    if closes.len() != CLOSES_REQUIRED {
        return Err(ModelError::Window {
            expected: CLOSES_REQUIRED,
            got: closes.len(),
        });
    }
    let close = closes.last().copied().ok_or(ModelError::Window {
        expected: CLOSES_REQUIRED,
        got: 0,
    })?;
    if !state.is_flat() {
        let elapsed = state
            .hours_since_entry
            .checked_add(1)
            .ok_or_else(|| ModelError::Config("hold counter overflowed".to_string()))?;
        let (prior, current) = returns_from_closes(closes)?;
        let (_, mut diagnostics) = shock_target(current, &prior, params)?;
        diagnostics.close = close;
        if elapsed >= params.hold_hours {
            return Ok((
                ModelState::new(),
                Step::Transition {
                    target_qty: Decimal::ZERO,
                    diagnostics,
                    reason: "hold complete; exit to flat",
                },
            ));
        }
        return Ok((
            ModelState {
                target_qty: state.target_qty,
                hours_since_entry: elapsed,
            },
            Step::NoChange {
                diagnostics,
                reason: "position open; ignore new shocks",
            },
        ));
    }

    let (prior, current) = returns_from_closes(closes)?;
    let (target, mut diagnostics) = shock_target(current, &prior, params)?;
    diagnostics.close = close;
    if target == Decimal::ZERO {
        return Ok((
            ModelState::new(),
            Step::NoChange {
                diagnostics,
                reason: "inside the shock band; remain flat",
            },
        ));
    }
    Ok((
        ModelState {
            target_qty: target,
            hours_since_entry: 0,
        },
        Step::Transition {
            target_qty: target,
            diagnostics,
            reason: "shock; enter opposite or continuation target",
        },
    ))
}

fn returns_from_closes(closes: &[Decimal]) -> Result<(Vec<f64>, f64), ModelError> {
    let mut prior = Vec::with_capacity(PRIOR_RETURNS);
    for pair in closes[..closes.len() - 1].windows(2) {
        prior.push(log_return(pair[0], pair[1])?);
    }
    if prior.len() != PRIOR_RETURNS {
        return Err(ModelError::Window {
            expected: PRIOR_RETURNS,
            got: prior.len(),
        });
    }
    let current = log_return(closes[closes.len() - 2], closes[closes.len() - 1])?;
    Ok((prior, current))
}

#[derive(Serialize)]
struct ConfigHashPayload<'a> {
    strategy_id: &'a str,
    strategy_version: &'a str,
    prior_returns: usize,
    volatility: &'a str,
    sigma_multiple: String,
    hold_completed_hours: u32,
    position_qty: Decimal,
    sign: &'a str,
}

pub fn config_hash(params: Params) -> Result<String, ModelError> {
    let payload = ConfigHashPayload {
        strategy_id: STRATEGY_NAME,
        strategy_version: STRATEGY_VERSION,
        prior_returns: PRIOR_RETURNS,
        volatility: "sample_stdev_divisor_n_minus_1",
        sigma_multiple: params.sigma_multiple.normalize().to_string(),
        hold_completed_hours: params.hold_hours,
        position_qty: position_qty(),
        sign: params.sign.as_str(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(|err| ModelError::Config(err.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn varied_prior() -> Vec<f64> {
        (0..PRIOR_RETURNS)
            .map(|index| if index % 2 == 0 { 0.01 } else { -0.01 })
            .collect()
    }

    fn alternating_closes(count: usize, last: Decimal) -> Vec<Decimal> {
        let mut closes = Vec::with_capacity(count);
        for index in 0..count - 1 {
            closes.push(if index % 2 == 0 { dec!(100) } else { dec!(101) });
        }
        closes.push(last);
        closes
    }

    #[test]
    fn down_shock_is_long_and_up_shock_is_short() {
        let params = Params::primary().unwrap();
        let prior = varied_prior();
        let sigma = sample_stdev(&prior).unwrap();
        let (long, _) = shock_target(-3.0 * sigma, &prior, params).unwrap();
        let (short, _) = shock_target(3.0 * sigma, &prior, params).unwrap();
        let (flat, _) = shock_target(-2.9 * sigma, &prior, params).unwrap();
        assert_eq!(long, position_qty());
        assert_eq!(short, -position_qty());
        assert_eq!(flat, Decimal::ZERO);
    }

    #[test]
    fn continuation_flips_the_sign() {
        let params = Params::new("3", 6, SignMode::Continuation).unwrap();
        let prior = varied_prior();
        let sigma = sample_stdev(&prior).unwrap();
        let (target, _) = shock_target(-3.0 * sigma, &prior, params).unwrap();
        assert_eq!(target, -position_qty());
    }

    #[test]
    fn exact_boundary_is_a_shock_and_zero_sigma_fails() {
        let params = Params::primary().unwrap();
        let prior = varied_prior();
        let sigma = sample_stdev(&prior).unwrap();
        let (target, _) = shock_target(-params_threshold(params, sigma), &prior, params).unwrap();
        assert_eq!(target, position_qty());
        let zeros = vec![0.0; PRIOR_RETURNS];
        assert_eq!(sample_stdev(&zeros), Err(ModelError::ZeroSigma));
    }

    fn params_threshold(params: Params, sigma: f64) -> f64 {
        params.sigma_multiple.to_f64().unwrap() * sigma
    }

    #[test]
    fn open_position_ignores_a_later_shock_and_exits_on_the_sixth_hour() {
        let params = Params::primary().unwrap();
        let entry = alternating_closes(CLOSES_REQUIRED, dec!(50));
        let (state, step) = on_hour(&ModelState::new(), params, &entry).unwrap();
        match step {
            Step::Transition { target_qty, .. } => assert_eq!(target_qty, position_qty()),
            Step::NoChange { .. } => panic!("expected an entry"),
        }
        let opposite = alternating_closes(CLOSES_REQUIRED, dec!(200));
        let mut state = state;
        for hour in 1..params.hold_hours {
            let (next, step) = on_hour(&state, params, &opposite).unwrap();
            assert!(
                matches!(step, Step::NoChange { .. }),
                "hour {hour} should ignore the new shock"
            );
            assert_eq!(next.target_qty, position_qty());
            state = next;
        }
        let (flat, step) = on_hour(&state, params, &opposite).unwrap();
        assert!(flat.is_flat());
        match step {
            Step::Transition {
                target_qty, reason, ..
            } => {
                assert_eq!(target_qty, Decimal::ZERO);
                assert_eq!(reason, "hold complete; exit to flat");
            }
            Step::NoChange { .. } => panic!("expected the exit"),
        }
    }

    #[test]
    fn short_window_is_an_error() {
        let params = Params::primary().unwrap();
        let err = on_hour(&ModelState::new(), params, &[dec!(100), dec!(101)]).unwrap_err();
        assert!(matches!(err, ModelError::Window { .. }));
    }

    #[test]
    fn primary_config_hash_is_stable() {
        assert_eq!(
            config_hash(Params::primary().unwrap()).unwrap(),
            "sha256:5df3141260c7d66b8685dcd7910883cd7e29b8a26b468e6cdcd8c68d4ebaca34"
        );
    }
}
