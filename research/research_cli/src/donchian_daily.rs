//! DonchianDailyV1 offline decision replay and economic backtest.
//!
//! Download and evaluation are separate. `evaluate` reads only local files.
//! It does not place orders or open a private trading client.
//!
//! ```text
//! cargo run -p research_cli -- donchian-daily download \
//!   --environment mainnet --start 2024-01-01 --end 2024-04-01 \
//!   --output data/donchian_daily/bybit-linear-BTCUSDT-D.json
//!
//! cargo run -p research_cli -- donchian-daily replay \
//!   --dataset data/donchian_daily/bybit-linear-BTCUSDT-D.json \
//!   --evaluation-start 2024-01-21 --evaluation-end 2024-04-01 \
//!   --position-size 0.001 --allow-short enabled \
//!   --decisions data/donchian_daily/bybit-linear-BTCUSDT-D-decisions.json
//! ```
//!
//! `--end` and `--evaluation-end` are exclusive UTC dates. The position size is the
//! model's fixed quantity for this run, not a capital or risk budget.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use research_engine::donchian_daily_v1::aux::AuxBundle;
use research_engine::donchian_daily_v1::aux::{
    download_aux_bundle, load_funding_file, load_price_series, market_inputs_from_files,
    save_bundle, AuxDownloadRequest,
};
use research_engine::donchian_daily_v1::backtest::{
    evaluate_experiment, load_decision_log, render_comparison, reproduce_frozen_decisions,
    write_comparison, write_simulation, EvaluateRequest, FROZEN_EVALUATION_END_MS,
    FROZEN_EVALUATION_START_MS,
};
use research_engine::donchian_daily_v1::offline::DatasetMetadata;
use research_engine::donchian_daily_v1::offline::{
    download_donchian_daily_dataset, load_historical_dataset, replay_donchian_daily_decisions,
    save_historical_dataset, DecisionLog, DownloadRequest, HistoricalDataset,
    MarketDataEnvironment, ReplayRequest,
};
use rust_decimal::Decimal;

#[derive(Parser, Debug)]
#[command(about = "Download Bybit BTCUSDT daily candles and replay DonchianDailyV1 decisions")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Fetch public linear daily candles and write a local dataset.
    Download(DownloadArgs),
    /// Replay decisions from a local dataset. Makes no network requests.
    Replay(ReplayArgs),
    /// Download public minute, funding, and mark data for the frozen experiment.
    DownloadAux(DownloadAuxArgs),
    /// Score the frozen decisions from local files. Makes no network requests.
    Evaluate(EvaluateArgs),
}

#[derive(Parser, Debug)]
pub struct DownloadArgs {
    /// Public market-data environment. Not taken from trading configuration.
    #[arg(long, value_enum)]
    environment: EnvironmentArg,
    /// Inclusive UTC date, YYYY-MM-DD.
    #[arg(long)]
    start: String,
    /// Exclusive UTC date, YYYY-MM-DD.
    #[arg(long)]
    end: String,
    /// Dataset JSON path.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Parser, Debug)]
pub struct ReplayArgs {
    /// Dataset written by the download command.
    #[arg(long)]
    dataset: PathBuf,
    /// First UTC date whose candle is evaluated, YYYY-MM-DD.
    #[arg(long)]
    evaluation_start: String,
    /// Exclusive UTC date, YYYY-MM-DD.
    #[arg(long)]
    evaluation_end: String,
    /// Fixed model quantity. Required for every run.
    #[arg(long)]
    position_size: String,
    /// Short entries are permitted only when this is `enabled`.
    #[arg(long, value_enum)]
    allow_short: Shorting,
    /// Decision-log JSON path.
    #[arg(long)]
    decisions: PathBuf,
}

#[derive(Parser, Debug)]
pub struct DownloadAuxArgs {
    /// Frozen daily dataset.
    #[arg(long)]
    dataset: PathBuf,
    /// Frozen decision log.
    #[arg(long)]
    decisions: PathBuf,
    /// Directory for trade-1m.json, funding.json, and mark-60.json.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Parser, Debug)]
pub struct EvaluateArgs {
    /// Frozen daily dataset.
    #[arg(long)]
    dataset: PathBuf,
    /// Frozen decision log.
    #[arg(long)]
    decisions: PathBuf,
    /// Directory written by download-aux.
    #[arg(long)]
    aux: PathBuf,
    /// Directory for scenario ledgers and the comparison.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum EnvironmentArg {
    Mainnet,
    Testnet,
    Demo,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shorting {
    Enabled,
    Disabled,
}

pub async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    execute(Args::parse().command).await
}

pub async fn execute(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Download(args) => download(args).await?,
        Command::Replay(args) => replay(args)?,
        Command::DownloadAux(args) => download_aux(args).await?,
        Command::Evaluate(args) => evaluate(args)?,
    }
    Ok(())
}

async fn download(args: DownloadArgs) -> Result<(), Box<dyn std::error::Error>> {
    let retrieved_at_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let request = DownloadRequest {
        environment: match args.environment {
            EnvironmentArg::Mainnet => MarketDataEnvironment::Mainnet,
            EnvironmentArg::Testnet => MarketDataEnvironment::Testnet,
            EnvironmentArg::Demo => MarketDataEnvironment::Demo,
        },
        symbol: "BTCUSDT".to_string(),
        start_ms: parse_utc_date(&args.start)?,
        end_exclusive_ms: parse_utc_date(&args.end)?,
        retrieved_at_ms,
    };
    let dataset = download_donchian_daily_dataset(&request).await?;
    save_historical_dataset(&args.output, &dataset)?;
    print_download(&dataset, &args.output);
    if !dataset.metadata.gaps.is_empty()
        || !dataset.metadata.conflicts.is_empty()
        || dataset.metadata.completed_candle_count == 0
    {
        return Err("dataset saved, but completed coverage is not contiguous".into());
    }
    Ok(())
}

fn replay(args: ReplayArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dataset = load_historical_dataset(&args.dataset)?;
    let position_qty: Decimal = args.position_size.parse()?;
    let request = ReplayRequest {
        evaluation_start_ms: parse_utc_date(&args.evaluation_start)?,
        evaluation_end_exclusive_ms: parse_utc_date(&args.evaluation_end)?,
        position_qty,
        allow_short: matches!(args.allow_short, Shorting::Enabled),
    };
    let log = replay_donchian_daily_decisions(&dataset, &request)?;
    if let Some(parent) = args.decisions.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&args.decisions, serde_json::to_vec_pretty(&log)?)?;
    print_replay(&log, &args.decisions);
    Ok(())
}

fn print_download(dataset: &HistoricalDataset, path: &Path) {
    let meta = &dataset.metadata;
    println!("validation={}", coverage_status(meta));
    println!(
        "venue={} environment={} product={} symbol={} interval={}",
        meta.venue, meta.environment, meta.product, meta.symbol, meta.interval
    );
    println!(
        "requested_start={}",
        format_ms(Some(meta.requested_start_ms))
    );
    println!(
        "requested_end_exclusive={}",
        format_ms(Some(meta.requested_end_exclusive_ms))
    );
    println!("actual_start={}", format_ms(meta.actual_start_ms));
    println!(
        "actual_end_exclusive={}",
        format_ms(meta.actual_end_exclusive_ms)
    );
    println!("candles={}", meta.candle_count);
    println!("completed_candles={}", meta.completed_candle_count);
    println!("pages_fetched={}", meta.pages_fetched);
    println!("gaps={}", meta.gaps.len());
    println!("conflicts={}", meta.conflicts.len());
    println!("retrieved_at={}", format_ms(Some(meta.retrieved_at_ms)));
    println!(
        "completed_cutoff={}",
        format_ms(Some(meta.completed_cutoff_ms))
    );
    println!("dataset_hash={}", meta.dataset_hash);
    println!("wrote={}", path.display());
    for gap in &meta.gaps {
        println!(
            "gap {} {}",
            format_ms(Some(gap.missing_start_ms)),
            format_ms(Some(gap.missing_end_exclusive_ms))
        );
    }
}

fn print_replay(log: &DecisionLog, path: &Path) {
    let run = &log.run;
    println!("validation={}", run.validation);
    println!("candles={}", run.completed_candles);
    println!("actual_start={}", format_ms(run.actual_start_ms));
    println!(
        "actual_end_exclusive={}",
        format_ms(run.actual_end_exclusive_ms)
    );
    println!("warmup_candles={}", run.warmup_candles);
    println!("evaluated_candles={}", run.evaluated_candles);
    println!("entries={}", run.entries);
    println!("exits={}", run.exits);
    println!("final_direction={}", run.final_direction);
    println!("dataset_hash={}", run.dataset_hash);
    println!("config_hash={}", run.config_hash);
    println!("initial_state_policy={}", run.initial_state_policy);
    println!("wrote={}", path.display());
}

async fn download_aux(args: DownloadAuxArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dataset = load_historical_dataset(&args.dataset)?;
    let saved = load_decision_log(&args.decisions)?;
    let fresh = reproduce_frozen_decisions(&dataset, &saved)?;
    let retrieved_at_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let bundle = download_aux_bundle(
        &AuxDownloadRequest {
            environment: MarketDataEnvironment::Mainnet,
            performance_start_ms: FROZEN_EVALUATION_START_MS,
            performance_end_exclusive_ms: FROZEN_EVALUATION_END_MS,
            retrieved_at_ms,
            position_qty: fresh.run.position_qty,
        },
        &fresh.decisions,
    )
    .await?;
    save_bundle(&args.output, &bundle)?;
    print_aux(&bundle, &args.output);
    if !bundle.funding.metadata.pagination_complete
        || !bundle.hourly_marks.metadata.pagination_complete
        || !bundle.trade_minutes.metadata.gaps.is_empty()
        || !bundle.hourly_marks.metadata.gaps.is_empty()
        || bundle
            .finer_marks
            .as_ref()
            .is_some_and(|marks| !marks.metadata.gaps.is_empty())
    {
        return Err(
            "auxiliary files were saved, but execution, funding, or mark coverage is incomplete"
                .into(),
        );
    }
    Ok(())
}

fn evaluate(args: EvaluateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dataset = load_historical_dataset(&args.dataset)?;
    let saved = load_decision_log(&args.decisions)?;
    let fresh = reproduce_frozen_decisions(&dataset, &saved)?;
    let trades = load_price_series(&args.aux.join("trade-1m.json"))?;
    let funding = load_funding_file(&args.aux.join("funding.json"))?;
    let marks = load_price_series(&args.aux.join("mark-60.json"))?;
    let finer_path = args.aux.join("mark-1.json");
    let finer = if finer_path.exists() {
        Some(load_price_series(&finer_path)?)
    } else {
        None
    };
    let market = market_inputs_from_files(&trades, &funding, &marks, finer.as_ref())?;
    let evaluation = evaluate_experiment(&EvaluateRequest {
        decisions: &fresh.decisions,
        final_direction: &fresh.run.final_direction,
        signal_config_hash: &fresh.run.config_hash,
        daily_dataset_hash: &fresh.run.dataset_hash,
        position_qty: fresh.run.position_qty,
        market: &market,
    })?;
    for pair in &evaluation.pairs {
        write_simulation(
            &args
                .output
                .join("donchian_daily_v1")
                .join(&pair.scenario.name),
            &pair.donchian,
        )?;
        write_simulation(
            &args
                .output
                .join("fixed_long_0.001_btc")
                .join(&pair.scenario.name),
            &pair.fixed_long,
        )?;
    }
    write_comparison(&args.output, &evaluation)?;
    print!("{}", render_comparison(&evaluation));
    println!("wrote={}", args.output.display());
    Ok(())
}

fn print_aux(bundle: &AuxBundle, path: &Path) {
    for (label, meta) in [
        ("trade_1m", &bundle.trade_minutes.metadata),
        ("funding", &bundle.funding.metadata),
        ("mark_60", &bundle.hourly_marks.metadata),
    ] {
        println!(
            "{label} count={} pages={} gaps={} pagination_complete={} hash={} span={}..{}",
            meta.count,
            meta.pages_fetched,
            meta.gaps.len(),
            meta.pagination_complete,
            meta.dataset_hash,
            format_ms(meta.actual_start_ms),
            format_ms(meta.actual_end_ms)
        );
    }
    if let Some(finer) = &bundle.finer_marks {
        println!(
            "mark_1 count={} gaps={} hash={}",
            finer.metadata.count,
            finer.metadata.gaps.len(),
            finer.metadata.dataset_hash
        );
    }
    println!("wrote={}", path.display());
}

fn coverage_status(meta: &DatasetMetadata) -> &'static str {
    if meta.conflicts.is_empty() && meta.gaps.is_empty() && meta.completed_candle_count > 0 {
        "ok"
    } else {
        "rejected"
    }
}

fn parse_utc_date(value: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or("date does not have a UTC midnight")?;
    let millis = Utc.from_utc_datetime(&naive).timestamp_millis();
    if millis < 0 {
        return Err("date is before the unix epoch".into());
    }
    Ok(millis as u64)
}

fn format_ms(value: Option<u64>) -> String {
    let Some(millis) = value else {
        return "none".to_string();
    };
    Utc.timestamp_millis_opt(millis as i64)
        .single()
        .map(|time| time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or(millis.to_string())
}
