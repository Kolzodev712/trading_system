use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use trading_report::collect::{collect_report, write_outputs};
use trading_report::schedule::{publish_latest, try_acquire_lock, unique_utc_dir};
use trading_report::store::{
    deployment_id_from, emit_json_bytes, load_saved_report_json, load_store_meta, meta_from_report,
    new_report_id, write_store_meta, ReportStore, StoreError, StoreMeta,
};
use trading_report::ReportConfig;

#[derive(Parser)]
#[command(name = "trading-report", about = "Read-only five-item trading report")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Collect {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Persist the saved report.json to Mongo. Required for scheduled delivery.
        #[arg(long)]
        persist_mongo: bool,
        #[arg(long)]
        report_id: Option<String>,
        #[arg(long)]
        deployment_id: Option<String>,
    },
    /// Persist a previously saved report without recollecting venue state.
    Persist {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        report_id: Option<String>,
        #[arg(long)]
        deployment_id: Option<String>,
    },
    /// Read stored report.json bytes from Mongo only. No gRPC, REST, journal, or collect.
    ReadJson {
        #[arg(long)]
        report_id: Option<String>,
        #[arg(long)]
        latest: bool,
        #[arg(long)]
        deployment_id: Option<String>,
        /// `-` writes JSON only to stdout. Any other path is written atomically.
        #[arg(long, default_value = "-")]
        output: String,
    },
    /// One-shot scheduled collect: unique UTC dir, Mongo required, latest after delivery.
    Scheduled {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        reports_dir: PathBuf,
        #[arg(long, default_value_t = 120)]
        timeout_secs: u64,
        #[arg(long)]
        report_id: Option<String>,
        #[arg(long)]
        deployment_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if let Some(store) = err.downcast_ref::<StoreError>() {
                eprintln!("{}", store);
                return ExitCode::from(store.exit_code() as u8);
            }
            eprintln!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Collect {
            config,
            out,
            persist_mongo,
            report_id,
            deployment_id,
        } => collect_cmd(config, out, persist_mongo, report_id, deployment_id).await,
        Commands::Persist {
            from,
            report_id,
            deployment_id,
        } => persist_cmd(from, report_id, deployment_id).await,
        Commands::ReadJson {
            report_id,
            latest,
            deployment_id,
            output,
        } => read_json_cmd(report_id, latest, deployment_id, output).await,
        Commands::Scheduled {
            config,
            reports_dir,
            timeout_secs,
            report_id,
            deployment_id,
        } => scheduled_cmd(config, reports_dir, timeout_secs, report_id, deployment_id).await,
    }
}

async fn collect_cmd(
    config: PathBuf,
    out: PathBuf,
    persist_mongo: bool,
    report_id: Option<String>,
    deployment_id: Option<String>,
) -> Result<()> {
    if persist_mongo {
        let _ = deployment_id_from(deployment_id.as_deref())?;
        let _ = trading_report::store::mongo_uri_from_env()?;
    }
    let cfg = ReportConfig::load(&config)?;
    let outputs = collect_report(&cfg).await.context("collect report")?;
    let written = write_outputs(&out, &outputs)?;
    let report_id = report_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(new_report_id);
    let deployment = match deployment_id_from(deployment_id.as_deref()) {
        Ok(id) => id,
        Err(_) => "local-dev".into(),
    };
    let meta = meta_from_report(report_id, deployment, &outputs.report, &written.report_json);
    write_store_meta(&out, &meta)?;
    if persist_mongo {
        persist_saved(&meta, &written.report_json)
            .await
            .map_err(|e| persist_failed(&out, e))?;
        eprintln!(
            "report_id={} verdict={} mongo=delivered out={}",
            meta.report_id,
            outputs.report.verdict.as_str(),
            out.display()
        );
    }
    println!(
        "wrote {} (report_id={} verdict={} actionable_for_analysis={})",
        out.display(),
        meta.report_id,
        outputs.report.verdict.as_str(),
        outputs.report.actionable_for_analysis
    );
    Ok(())
}

async fn persist_cmd(
    from: PathBuf,
    report_id: Option<String>,
    deployment_id: Option<String>,
) -> Result<()> {
    let json = load_saved_report_json(&from)?;
    let meta = resolve_persist_meta(&from, report_id, deployment_id, &json)?;
    write_store_meta(&from, &meta)?;
    persist_saved(&meta, &json)
        .await
        .map_err(|e| persist_failed(&from, e))?;
    eprintln!(
        "report_id={} verdict={} mongo=delivered out={}",
        meta.report_id,
        meta.verdict,
        from.display()
    );
    Ok(())
}

fn resolve_persist_meta(
    from: &std::path::Path,
    report_id: Option<String>,
    deployment_id: Option<String>,
    json: &str,
) -> Result<StoreMeta> {
    let mut meta = match load_store_meta(from) {
        Ok(meta) => meta,
        Err(_) => {
            let report: trading_report::TradingReport =
                serde_json::from_str(json).context("parse saved report.json")?;
            let report_id = report_id
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(new_report_id);
            let deployment = deployment_id_from(deployment_id.as_deref())?;
            meta_from_report(report_id, deployment, &report, json)
        }
    };
    if let Some(id) = report_id.filter(|s| !s.trim().is_empty()) {
        meta.report_id = id;
    }
    if let Some(id) = deployment_id.filter(|s| !s.trim().is_empty()) {
        meta.deployment_id = id;
    }
    meta.report_json_sha256 = trading_report::store::sha256_hex(json.as_bytes());
    if meta.deployment_id.trim().is_empty() || meta.deployment_id == "local-dev" {
        meta.deployment_id = deployment_id_from(None).unwrap_or(meta.deployment_id);
    }
    Ok(meta)
}

async fn persist_saved(meta: &StoreMeta, report_json: &str) -> Result<(), StoreError> {
    let store = ReportStore::connect_from_env().await?;
    store.persist(meta, report_json).await
}

fn persist_failed(out: &std::path::Path, err: StoreError) -> anyhow::Error {
    eprintln!(
        "STORAGE_FAILED report_id={} mongo=failed out={} err={}",
        load_store_meta(out)
            .map(|m| m.report_id)
            .unwrap_or_else(|_| "unknown".into()),
        out.display(),
        err
    );
    err.into()
}

async fn read_json_cmd(
    report_id: Option<String>,
    latest: bool,
    deployment_id: Option<String>,
    output: String,
) -> Result<()> {
    if latest && report_id.is_some() {
        bail!("use either --report-id or --latest, not both");
    }
    if !latest && report_id.is_none() {
        bail!("--report-id or --latest is required");
    }
    if latest && deployment_id.as_deref().unwrap_or("").trim().is_empty() {
        let _ = deployment_id_from(None)?;
    }
    let store = ReportStore::connect_from_env().await?;
    let json = if latest {
        let deployment = deployment_id_from(deployment_id.as_deref())?;
        let (_id, json) = store.read_latest_json(&deployment).await?;
        json
    } else {
        store.read_json(report_id.as_deref().unwrap()).await?
    };
    emit_json_bytes(&output, &json)?;
    Ok(())
}

async fn scheduled_cmd(
    config: PathBuf,
    reports_dir: PathBuf,
    timeout_secs: u64,
    report_id: Option<String>,
    deployment_id: Option<String>,
) -> Result<()> {
    let _ = deployment_id_from(deployment_id.as_deref())?;
    let _ = trading_report::store::mongo_uri_from_env()?;
    let lock = match try_acquire_lock(&reports_dir)? {
        Some(lock) => lock,
        None => {
            eprintln!("skipped: collection already running");
            return Ok(());
        }
    };
    let work = async {
        let out = unique_utc_dir(&reports_dir)?;
        collect_cmd(config, out.clone(), true, report_id, deployment_id).await?;
        publish_latest(&reports_dir, &out)?;
        eprintln!("latest={}", reports_dir.join("latest").display());
        Ok::<(), anyhow::Error>(())
    };
    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), work).await;
    drop(lock);
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => bail!("collection timed out after {timeout_secs}s"),
    }
}
