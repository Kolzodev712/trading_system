use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::model::MandateView;

pub const BYBIT_DEMO_REST: &str = "https://api-demo.bybit.com";

pub fn approved_demo_rest(url: &str) -> bool {
    url.trim()
        .trim_end_matches('/')
        .eq_ignore_ascii_case(BYBIT_DEMO_REST)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportConfig {
    #[serde(default = "default_schema")]
    pub schema_version: String,
    pub freshness: FreshnessConfig,
    pub mandate: MandateConfig,
    pub runners: Vec<RunnerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FreshnessConfig {
    pub max_book_age_ms: i64,
    pub max_source_age_ms: i64,
    pub qty_tolerance: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MandateConfig {
    #[serde(default = "default_true")]
    pub advisory: bool,
    pub allowed_instruments: Vec<String>,
    pub allowed_direction: String,
    pub max_leverage: String,
    pub max_exposure: String,
    pub max_loss_per_trade: String,
    pub max_loss_per_day: String,
    pub time_horizon: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunnerConfig {
    pub label: String,
    pub grpc_endpoint: String,
    pub mode: String,
    pub venue: String,
    pub environment: String,
    pub execution_mode: String,
    pub account_alias: String,
    pub instrument: String,
    pub instrument_type: String,
    pub journal_path: PathBuf,
    #[serde(default)]
    pub executable_revision: Option<String>,
    pub venue_source: VenueSourceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VenueSourceConfig {
    pub kind: String,
    pub rest_base: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_secret_env: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

fn default_schema() -> String {
    "1".into()
}

fn default_true() -> bool {
    true
}

impl ReportConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read report config {}", path.display()))?;
        let cfg: Self = toml::from_str(&raw).context("parse report config")?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.runners.is_empty() {
            bail!("report config requires at least one runner");
        }
        if self.freshness.max_book_age_ms <= 0 || self.freshness.max_source_age_ms <= 0 {
            bail!("freshness thresholds must be positive");
        }
        if self.freshness.qty_tolerance.trim().is_empty() {
            bail!("qty_tolerance is required");
        }
        let mut labels = HashSet::new();
        let mut bindings = HashSet::new();
        for runner in &self.runners {
            if runner.label.trim().is_empty() {
                bail!("runner label is required");
            }
            if !labels.insert(runner.label.clone()) {
                bail!("duplicate runner label {}", runner.label);
            }
            if runner.grpc_endpoint.trim().is_empty() {
                bail!("{}: grpc_endpoint is required", runner.label);
            }
            if runner.account_alias.trim().is_empty() || runner.instrument.trim().is_empty() {
                bail!(
                    "{}: account_alias and instrument are required",
                    runner.label
                );
            }
            let binding = format!(
                "{}:{}:{}:{}",
                runner.venue, runner.environment, runner.account_alias, runner.instrument
            );
            if !bindings.insert(binding) {
                bail!(
                    "ambiguous account binding for {} / {}",
                    runner.account_alias,
                    runner.instrument
                );
            }
            if runner.journal_path.as_os_str().is_empty() {
                bail!("{}: journal_path is required", runner.label);
            }
            let kind = runner.venue_source.kind.to_ascii_lowercase();
            if kind != "paper" && kind != "bybit_demo" {
                bail!(
                    "{}: unsupported venue kind {} (explicit unverified at collect)",
                    runner.label,
                    runner.venue_source.kind
                );
            }
            if kind == "bybit_demo" {
                if runner.venue_source.api_key_env.is_none()
                    || runner.venue_source.api_secret_env.is_none()
                {
                    bail!(
                        "{}: bybit_demo requires api_key_env and api_secret_env",
                        runner.label
                    );
                }
                if !approved_demo_rest(&runner.venue_source.rest_base) {
                    bail!(
                        "{}: bybit_demo rest_base must be {BYBIT_DEMO_REST}; refusing {}",
                        runner.label,
                        runner.venue_source.rest_base
                    );
                }
            }
            if runner.venue_source.rest_base.trim().is_empty() {
                bail!("{}: venue rest_base is required", runner.label);
            }
        }
        Ok(())
    }

    pub fn mandate_view(&self) -> MandateView {
        MandateView {
            advisory: self.mandate.advisory,
            allowed_instruments: self.mandate.allowed_instruments.clone(),
            allowed_direction: self.mandate.allowed_direction.clone(),
            max_leverage: self.mandate.max_leverage.clone(),
            max_exposure: self.mandate.max_exposure.clone(),
            max_loss_per_trade: self.mandate.max_loss_per_trade.clone(),
            max_loss_per_day: self.mandate.max_loss_per_day.clone(),
            time_horizon: self.mandate.time_horizon.clone(),
            enforced_by_runner: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ReportConfig {
        ReportConfig {
            schema_version: "1".into(),
            freshness: FreshnessConfig {
                max_book_age_ms: 5_000,
                max_source_age_ms: 10_000,
                qty_tolerance: "0.00000001".into(),
            },
            mandate: MandateConfig {
                advisory: true,
                allowed_instruments: vec!["BTC/USDT".into()],
                allowed_direction: "both".into(),
                max_leverage: "1".into(),
                max_exposure: "0.01".into(),
                max_loss_per_trade: "50".into(),
                max_loss_per_day: "100".into(),
                time_horizon: "intraday".into(),
            },
            runners: vec![RunnerConfig {
                label: "a".into(),
                grpc_endpoint: "http://127.0.0.1:1".into(),
                mode: "levels".into(),
                venue: "PAPER".into(),
                environment: "LOCAL".into(),
                execution_mode: "exchange".into(),
                account_alias: "paper-a".into(),
                instrument: "BTC/USDT".into(),
                instrument_type: "spot".into(),
                journal_path: PathBuf::from("/tmp/a.jsonl"),
                executable_revision: None,
                venue_source: VenueSourceConfig {
                    kind: "paper".into(),
                    rest_base: "http://127.0.0.1:2".into(),
                    api_key_env: None,
                    api_secret_env: None,
                    category: None,
                },
            }],
        }
    }

    #[test]
    fn rejects_duplicate_labels() {
        let mut cfg = sample();
        cfg.runners.push(cfg.runners[0].clone());
        cfg.runners[1].account_alias = "other".into();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_ambiguous_binding() {
        let mut cfg = sample();
        let mut second = cfg.runners[0].clone();
        second.label = "b".into();
        cfg.runners.push(second);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_unapproved_demo_rest_base() {
        let mut cfg = sample();
        cfg.runners[0].venue_source.kind = "bybit_demo".into();
        cfg.runners[0].venue_source.rest_base = "https://evil.example".into();
        cfg.runners[0].venue_source.api_key_env = Some("BYBIT_DEMO_API_KEY".into());
        cfg.runners[0].venue_source.api_secret_env = Some("BYBIT_DEMO_API_SECRET".into());
        let err = cfg.validate().expect_err("unapproved host");
        assert!(err.to_string().contains(BYBIT_DEMO_REST));
    }
}
