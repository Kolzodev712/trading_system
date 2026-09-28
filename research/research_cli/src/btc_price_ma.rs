//! Offline development experiment for BTCPriceMALongFlatV1 (N=14/28/70).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use research_engine::btc_price_ma_long_flat_v1::experiment::{
    scenario_by_name, score_fixed_long, score_lookback,
};
use research_engine::btc_price_ma_long_flat_v1::gates::{evaluate_primary_screen, ScreenResult};
use research_engine::btc_price_ma_long_flat_v1::offline::{
    default_replay_request, merge_datasets, replay, save_decision_log,
};
use research_engine::btc_price_ma_long_flat_v1::{
    position_qty, DIAGNOSTIC_LOOKBACKS, PRIMARY_LOOKBACK,
};
use research_engine::donchian_daily_v1::aux::{
    download_trade_minutes_for, load_funding_file, load_price_series, market_inputs_from_files,
    AuxDownloadRequest,
};
use research_engine::donchian_daily_v1::backtest::{
    required_execution_minutes_for_target_changes, target_changes, write_simulation, Simulation,
    FROZEN_EVALUATION_END_MS, FROZEN_EVALUATION_START_MS,
};
use research_engine::donchian_daily_v1::offline::{
    download_donchian_daily_dataset, load_historical_dataset, save_historical_dataset,
    DownloadRequest, MarketDataEnvironment,
};

#[derive(Parser, Debug)]
#[command(about = "Run the frozen BTCPriceMALongFlatV1 development experiment")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    DownloadWarmup {
        #[arg(long)]
        output: PathBuf,
    },
    Run(RunArgs),
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[arg(long)]
    main_dataset: PathBuf,
    #[arg(long)]
    warmup_dataset: PathBuf,
    #[arg(long)]
    shared_aux: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

pub async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    execute(Args::parse().command).await
}

pub async fn execute(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::DownloadWarmup { output } => download_warmup(output).await?,
        Command::Run(args) => run_experiment(args).await?,
    }
    Ok(())
}

async fn download_warmup(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let request = DownloadRequest {
        environment: MarketDataEnvironment::Mainnet,
        symbol: "BTCUSDT".to_string(),
        start_ms: parse_utc_date("2022-11-01")?,
        end_exclusive_ms: parse_utc_date("2023-01-01")?,
        retrieved_at_ms: now_ms(),
    };
    let dataset = download_donchian_daily_dataset(&request).await?;
    save_historical_dataset(&output, &dataset)?;
    println!(
        "warmup candles={} hash={} wrote={}",
        dataset.metadata.candle_count,
        dataset.metadata.dataset_hash,
        output.display()
    );
    Ok(())
}

async fn run_experiment(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let main = load_historical_dataset(&args.main_dataset)?;
    let warmup = load_historical_dataset(&args.warmup_dataset)?;
    let merged = merge_datasets(&warmup, &main)?;
    let lookbacks = [
        DIAGNOSTIC_LOOKBACKS[0],
        PRIMARY_LOOKBACK,
        DIAGNOSTIC_LOOKBACKS[1],
    ];
    let mut logs = Vec::new();
    let mut all_minutes = BTreeSet::new();
    let qty = position_qty();
    for lookback in lookbacks {
        let log = replay(&merged, &default_replay_request(lookback))?;
        let changes = target_changes(&log.decisions)?;
        all_minutes.extend(required_execution_minutes_for_target_changes(
            &changes,
            FROZEN_EVALUATION_START_MS,
            FROZEN_EVALUATION_END_MS,
            qty,
        )?);
        let path = args.output.join(format!("decisions-n{lookback}.json"));
        save_decision_log(&path, &log)?;
        println!(
            "n{lookback} entries={} exits={} final={} wrote={}",
            log.run.entries,
            log.run.exits,
            log.run.final_direction,
            path.display()
        );
        logs.push((lookback, log));
    }

    let aux_dir = args.output.join("aux");
    std::fs::create_dir_all(&aux_dir)?;
    let trade_path = aux_dir.join("trade-1m.json");
    if !trade_path.exists() {
        let trade = download_trade_minutes_for(
            &AuxDownloadRequest {
                environment: MarketDataEnvironment::Mainnet,
                performance_start_ms: FROZEN_EVALUATION_START_MS,
                performance_end_exclusive_ms: FROZEN_EVALUATION_END_MS,
                retrieved_at_ms: now_ms(),
                position_qty: qty,
            },
            all_minutes.clone(),
            "btc_price_ma_long_flat_v1_trade_1m",
        )
        .await?;
        if !trade.metadata.gaps.is_empty() {
            return Err("trade minute download has gaps".into());
        }
        std::fs::write(&trade_path, serde_json::to_vec_pretty(&trade)?)?;
        println!(
            "trade_1m count={} hash={}",
            trade.metadata.count, trade.metadata.dataset_hash
        );
    } else {
        println!("reusing {}", trade_path.display());
    }

    for name in ["funding.json", "mark-60.json"] {
        let dst = aux_dir.join(name);
        if !dst.exists() {
            std::fs::copy(args.shared_aux.join(name), &dst)?;
        }
    }

    let market = market_inputs_from_files(
        &load_price_series(&trade_path)?,
        &load_funding_file(&aux_dir.join("funding.json"))?,
        &load_price_series(&aux_dir.join("mark-60.json"))?,
        None,
    )?;

    let fixed = score_fixed_long(&market)?;
    let backtest_root = args.output.join("backtest");
    write_lookback(
        &backtest_root.join("fixed_long_0.001_btc"),
        "fixed_long_0.001_btc",
        &fixed,
    )?;

    let mut scored = Vec::new();
    for (lookback, log) in &logs {
        let sims = score_lookback(log, *lookback, &market)?;
        let role = if *lookback == PRIMARY_LOOKBACK {
            "primary_n28"
        } else {
            &format!("diagnostic_n{lookback}")
        };
        write_lookback(&backtest_root.join(role), &format!("n{lookback}"), &sims)?;
        scored.push((*lookback, sims));
    }

    let primary = scored
        .iter()
        .find(|(n, _)| *n == PRIMARY_LOOKBACK)
        .map(|(_, s)| s)
        .expect("primary");
    let d14 = scored
        .iter()
        .find(|(n, _)| *n == DIAGNOSTIC_LOOKBACKS[0])
        .map(|(_, s)| s)
        .expect("n14");
    let d70 = scored
        .iter()
        .find(|(n, _)| *n == DIAGNOSTIC_LOOKBACKS[1])
        .map(|(_, s)| s)
        .expect("n70");

    let screen = evaluate_primary_screen(
        scenario_by_name(primary, "cost_a")?,
        scenario_by_name(primary, "higher_cost")?,
        scenario_by_name(primary, "delayed_execution")?,
        scenario_by_name(d14, "cost_a")?,
        scenario_by_name(d70, "cost_a")?,
        scenario_by_name(&fixed, "cost_a")?,
    );
    let gates_path = args.output.join("gates.json");
    std::fs::write(&gates_path, serde_json::to_vec_pretty(&screen)?)?;
    write_report(
        &args.output.join("experiment_report.md"),
        &screen,
        primary,
        &fixed,
    )?;

    println!("disposition={}", screen.disposition);
    for gate in &screen.gates {
        println!(
            "gate {} {} {}",
            gate.gate,
            if gate.passed { "PASS" } else { "FAIL" },
            gate.detail
        );
    }
    println!("wrote={}", args.output.display());
    Ok(())
}

fn write_lookback(
    root: &std::path::Path,
    book: &str,
    simulations: &[Simulation],
) -> Result<(), Box<dyn std::error::Error>> {
    for sim in simulations {
        write_simulation(&root.join(book).join(&sim.summary.scenario_name), sim)?;
    }
    Ok(())
}

fn write_report(
    path: &std::path::Path,
    screen: &ScreenResult,
    primary: &[Simulation],
    fixed: &[Simulation],
) -> Result<(), Box<dyn std::error::Error>> {
    let cost_a = scenario_by_name(primary, "cost_a")?;
    let bench = scenario_by_name(fixed, "cost_a")?;
    let zero = scenario_by_name(primary, "zero_cost_diagnostic")?;
    let mut text = String::new();
    use std::fmt::Write;
    writeln!(text, "# BTCPriceMALongFlatV1 development experiment")?;
    writeln!(text)?;
    writeln!(text, "Primary N=28 Cost A net: {}", cost_a.summary.net_pnl)?;
    writeln!(text, "Primary N=28 zero-cost net: {}", zero.summary.net_pnl)?;
    writeln!(
        text,
        "Fixed long Cost A net: {} drawdown {}",
        bench.summary.net_pnl, bench.summary.max_drawdown_usdt
    )?;
    writeln!(text)?;
    writeln!(text, "Disposition: **{}**", screen.disposition)?;
    writeln!(text)?;
    for gate in &screen.gates {
        writeln!(
            text,
            "- {}: {} ({})",
            gate.gate,
            if gate.passed { "pass" } else { "fail" },
            gate.detail
        )?;
    }
    writeln!(
        text,
        "\nHoldout 2026 was not opened. N=14/N=70 higher-cost and delayed ledgers are under backtest/diagnostic_n*/."
    )?;
    std::fs::write(path, text)?;
    Ok(())
}

fn parse_utc_date(value: &str) -> Result<u64, Box<dyn std::error::Error>> {
    use chrono::{NaiveDate, TimeZone, Utc};
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    Ok(Utc
        .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .timestamp_millis() as u64)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
