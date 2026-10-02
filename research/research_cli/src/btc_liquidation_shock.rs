//! Offline commands for BTCLiquidationShockV1. Does not submit orders.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use research_engine::btc_liquidation_shock_v1::coverage::{
    assess_existing_sources, write_coverage_report, CoveragePaths,
};
use research_engine::btc_liquidation_shock_v1::dataset::{
    download_hourly_dataset, frozen_download_request, save_hourly_dataset, sha256_file,
};
use research_engine::btc_liquidation_shock_v1::experiment::{
    download_execution_minutes, execution_minutes_for, run_primary, RunRequest,
};
use research_engine::btc_liquidation_shock_v1::replay::{
    load_decision_log, replay, save_decision_log, save_events,
};
use research_engine::btc_liquidation_shock_v1::ShockError;
use strategy_core::btc_liquidation_shock_v1::{Params, SignMode};

#[derive(Parser, Debug)]
#[command(about = "BTCLiquidationShockV1 research. Does not submit orders.")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Check the committed BTC linear files. Expected to fail closed.
    Coverage {
        #[arg(
            long,
            default_value = "data/btc_price_ma_long_flat_v1/aux/mark-60.json"
        )]
        mark: PathBuf,
        #[arg(
            long,
            default_value = "data/btc_price_ma_long_flat_v1/aux/funding.json"
        )]
        funding: PathBuf,
        #[arg(
            long,
            default_value = "data/btc_price_ma_long_flat_v1/aux/trade-1m.json"
        )]
        trade_minutes: PathBuf,
        #[arg(long)]
        hourly: Option<PathBuf>,
        #[arg(
            long,
            default_value = "data/btc_liquidation_shock_v1/coverage_gate.json"
        )]
        output: PathBuf,
    },
    /// Download the new versioned hourly trade dataset. Does not alter Candidate #1 or #2 files.
    DownloadHourly {
        #[arg(long, default_value = "data/btc_liquidation_shock_v1/trade-60.json")]
        output: PathBuf,
    },
    /// Replay reversal and continuation decisions. No fills.
    Decisions {
        #[arg(long, default_value = "data/btc_liquidation_shock_v1/trade-60.json")]
        hourly: PathBuf,
        #[arg(
            long,
            default_value = "research/candidates/btc_liquidation_shock_v1/experiment.json"
        )]
        experiment: PathBuf,
        #[arg(
            long,
            default_value = "research/candidates/btc_liquidation_shock_v1/SPEC.md"
        )]
        human_spec: PathBuf,
        #[arg(long, default_value = "data/btc_liquidation_shock_v1")]
        output: PathBuf,
        #[arg(long, default_value_t = 3.0)]
        sigma: f64,
        #[arg(long, default_value_t = 6)]
        hold_hours: u32,
    },
    /// Fetch execution minutes for frozen decision files.
    DownloadMinutes {
        #[arg(long)]
        decisions: Vec<PathBuf>,
        #[arg(long, default_value = "data/btc_liquidation_shock_v1/trade-1m.json")]
        output: PathBuf,
    },
    /// Score the primary rule. Refuses a dirty git worktree.
    Run {
        #[arg(
            long,
            default_value = "research/candidates/btc_liquidation_shock_v1/experiment.json"
        )]
        experiment: PathBuf,
        #[arg(
            long,
            default_value = "research/candidates/btc_liquidation_shock_v1/SPEC.md"
        )]
        human_spec: PathBuf,
        #[arg(long, default_value = "data/btc_liquidation_shock_v1/trade-60.json")]
        hourly: PathBuf,
        #[arg(
            long,
            default_value = "data/btc_liquidation_shock_v1/decisions-reversal.json"
        )]
        decisions: PathBuf,
        #[arg(
            long,
            default_value = "data/btc_liquidation_shock_v1/decisions-continuation.json"
        )]
        placebo_decisions: PathBuf,
        #[arg(long, default_value = "data/btc_liquidation_shock_v1/trade-1m.json")]
        minutes: PathBuf,
        #[arg(
            long,
            default_value = "data/btc_price_ma_long_flat_v1/aux/mark-60.json"
        )]
        mark: PathBuf,
        #[arg(
            long,
            default_value = "data/btc_price_ma_long_flat_v1/aux/funding.json"
        )]
        funding: PathBuf,
        #[arg(long, default_value = "data/btc_liquidation_shock_v1/backtest")]
        output: PathBuf,
        #[arg(long, default_value = ".")]
        worktree: PathBuf,
    },
}

pub async fn execute(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Coverage {
            mark,
            funding,
            trade_minutes,
            hourly,
            output,
        } => {
            let report = assess_existing_sources(CoveragePaths {
                mark_60: &mark,
                funding: &funding,
                trade_1m: &trade_minutes,
                hourly_trade: hourly.as_deref(),
            })?;
            write_coverage_report(&output, &report)?;
            println!(
                "disposition={} wrote={}",
                report.disposition,
                output.display()
            );
            if report.disposition != "pass" {
                return Err(ShockError::new("coverage gate failed closed").into());
            }
        }
        Command::DownloadHourly { output } => {
            let dataset = download_hourly_dataset(&frozen_download_request(now_ms())).await?;
            save_hourly_dataset(&output, &dataset)?;
            println!(
                "candles={} hash={} wrote={}",
                dataset.metadata.candle_count,
                dataset.metadata.dataset_hash,
                output.display()
            );
        }
        Command::Decisions {
            hourly,
            experiment,
            human_spec,
            output,
            sigma,
            hold_hours,
        } => {
            let multiple = sigma_arg(sigma)?;
            let dataset =
                research_engine::btc_liquidation_shock_v1::dataset::load_hourly_dataset(&hourly)?;
            let spec_sha = sha256_file(&experiment)?;
            let human_sha = sha256_file(&human_spec)?;
            research_engine::btc_liquidation_shock_v1::load_experiment(&experiment)?;
            for sign in [SignMode::Reversal, SignMode::Continuation] {
                let params = Params::new(&multiple, hold_hours, sign)
                    .map_err(|err| ShockError::new(err.to_string()))?;
                let (log, events) = replay(&dataset, params, &spec_sha, &human_sha)?;
                let stem = sign.as_str();
                save_decision_log(&output.join(format!("decisions-{stem}.json")), &log)?;
                save_events(&output.join(format!("events-{stem}.json")), &events)?;
                println!(
                    "{stem} decisions={} hash={}",
                    log.decisions.len(),
                    log.config_hash
                );
            }
        }
        Command::DownloadMinutes { decisions, output } => {
            if decisions.is_empty() {
                return Err(ShockError::new("pass at least one --decisions file").into());
            }
            let mut logs = Vec::new();
            for path in &decisions {
                logs.push(load_decision_log(path)?);
            }
            let required = execution_minutes_for(&logs)?;
            let file = download_execution_minutes(&required, &output, now_ms()).await?;
            println!(
                "minutes={} hash={} wrote={}",
                file.metadata.count,
                file.metadata.dataset_hash,
                output.display()
            );
        }
        Command::Run {
            experiment,
            human_spec,
            hourly,
            decisions,
            placebo_decisions,
            minutes,
            mark,
            funding,
            output,
            worktree,
        } => {
            let disposition = run_primary(&RunRequest {
                experiment: &experiment,
                human_spec: &human_spec,
                hourly: &hourly,
                decisions: &decisions,
                placebo_decisions: &placebo_decisions,
                minutes: &minutes,
                mark: &mark,
                funding: &funding,
                output: &output,
                worktree: &worktree,
            })?;
            println!("disposition={disposition} wrote={}", output.display());
        }
    }
    Ok(())
}

fn sigma_arg(value: f64) -> Result<String, ShockError> {
    if value == 2.5 {
        Ok("2.5".to_string())
    } else if value == 3.0 {
        Ok("3".to_string())
    } else if value == 3.5 {
        Ok("3.5".to_string())
    } else {
        Err(ShockError::new("sigma must be one of 2.5, 3, or 3.5"))
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
