//! Research errors and the frozen experiment file for BTCLiquidationShockV1.

use serde::Deserialize;
use std::path::Path;

pub mod coverage;
pub mod dataset;
pub mod experiment;
pub mod replay;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ShockError {
    message: String,
}

impl ShockError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentFile {
    pub candidate_id: String,
    pub candidate_version: String,
    pub venue: String,
    pub environment: String,
    pub product: String,
    pub symbol: String,
    pub interval: String,
    pub performance_start_ms: u64,
    pub performance_end_exclusive_ms: u64,
    pub prior_returns: usize,
    pub volatility: String,
    pub position_qty_btc: String,
    pub primary: PrimarySpec,
    pub placebo_sign: String,
    pub scenarios: Vec<String>,
    pub promotion: PromotionSpec,
    pub robustness: RobustnessSpec,
    pub volatility_terciles: String,
    pub holdout: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimarySpec {
    pub sigma_multiple: String,
    pub hold_completed_hours: u32,
    pub sign: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionSpec {
    pub min_completed_trades: usize,
    pub require_cost_a_net_positive: bool,
    pub require_higher_cost_net_positive: bool,
    pub require_delayed_net_positive: bool,
    pub require_cost_a_beats_placebo: bool,
    pub reject_if_removing_largest_winner_leaves_non_positive: bool,
    pub reject_if_negative_entry_years_at_least: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobustnessSpec {
    pub sigma_multiples: Vec<String>,
    pub hold_completed_hours: Vec<u32>,
    pub min_single_knob_supporters: usize,
    pub min_completed_trades: usize,
    pub single_knob_neighbors: Vec<NeighborSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeighborSpec {
    pub sigma_multiple: String,
    pub hold_completed_hours: u32,
}

pub fn load_experiment(path: &Path) -> Result<ExperimentFile, ShockError> {
    let body = std::fs::read(path)
        .map_err(|err| ShockError::new(format!("read {}: {err}", path.display())))?;
    let file: ExperimentFile = serde_json::from_slice(&body)
        .map_err(|err| ShockError::new(format!("decode experiment spec: {err}")))?;
    validate_experiment(&file)?;
    Ok(file)
}

pub fn validate_experiment(file: &ExperimentFile) -> Result<(), ShockError> {
    let expected_scenarios = [
        "zero_cost_diagnostic",
        "cost_a",
        "higher_cost",
        "delayed_execution",
    ];
    if file.candidate_id != strategy_core::btc_liquidation_shock_v1::STRATEGY_NAME
        || file.candidate_version != strategy_core::btc_liquidation_shock_v1::STRATEGY_VERSION
        || file.venue != "bybit"
        || file.environment != "mainnet"
        || file.product != "linear"
        || file.symbol != "BTCUSDT"
        || file.interval != "60"
        || file.performance_start_ms != dataset::PERFORMANCE_START_MS
        || file.performance_end_exclusive_ms != dataset::PERFORMANCE_END_EXCLUSIVE_MS
        || file.prior_returns != strategy_core::btc_liquidation_shock_v1::PRIOR_RETURNS
        || file.volatility != "sample_stdev_divisor_n_minus_1"
        || file.position_qty_btc != "0.001"
        || file.primary.sigma_multiple
            != strategy_core::btc_liquidation_shock_v1::PRIMARY_SIGMA_MULTIPLE
        || file.primary.hold_completed_hours
            != strategy_core::btc_liquidation_shock_v1::PRIMARY_HOLD_HOURS
        || file.primary.sign != "reversal"
        || file.placebo_sign != "continuation"
        || file.scenarios != expected_scenarios
        || file.promotion.min_completed_trades != 20
        || !file.promotion.require_cost_a_net_positive
        || !file.promotion.require_higher_cost_net_positive
        || !file.promotion.require_delayed_net_positive
        || !file.promotion.require_cost_a_beats_placebo
        || !file
            .promotion
            .reject_if_removing_largest_winner_leaves_non_positive
        || file.promotion.reject_if_negative_entry_years_at_least != 2
        || file.robustness.min_single_knob_supporters != 3
        || file.robustness.min_completed_trades != 20
        || file.volatility_terciles != "descriptive_only"
        || file.holdout != "sealed_2026"
    {
        return Err(ShockError::new(
            "experiment.json does not match the frozen BTCLiquidationShockV1 constants",
        ));
    }
    let multiples = ["2.5", "3", "3.5"];
    let holds = [3u32, 6, 12];
    if file.robustness.sigma_multiples != multiples
        || file.robustness.hold_completed_hours != holds
        || file.robustness.single_knob_neighbors.len() != 4
    {
        return Err(ShockError::new(
            "robustness matrix does not match the preregistered neighborhood",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod spec_file {
    use super::load_experiment;
    use std::path::Path;

    #[test]
    fn committed_experiment_file_matches_the_frozen_constants() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../candidates/btc_liquidation_shock_v1/experiment.json");
        load_experiment(&path).unwrap();
    }
}
