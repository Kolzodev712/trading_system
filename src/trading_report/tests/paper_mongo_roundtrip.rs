//! Paper Levels Exchange + Strategies Shadow → Mongo → read-json bytes.
//! Does not Resume, place Demo orders, or mutate Bybit.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use trading_report::store::sha256_hex;
use workflow_harness::paper::{CatalogKind, PaperInstrument, PaperStack};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_trading-report"))
}

fn test_mongo_uri() -> String {
    std::env::var("TRADING_REPORT_TEST_MONGO_URI")
        .unwrap_or_else(|_| "mongodb://127.0.0.1:27017".into())
}

#[tokio::test]
#[ignore = "spawns k-paper and two runners; run with --ignored"]
async fn paper_collect_mongo_readback() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/paper-mongo-roundtrip");
    fs::create_dir_all(&root).unwrap();
    let levels = PaperStack::launch(
        &root,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    .expect("levels");
    let strategies = PaperStack::launch_c2_shadow_readonly(&root)
        .await
        .expect("strategies shadow");
    tokio::time::sleep(Duration::from_secs(2)).await;
    for path in [&levels.journal_path, &strategies.journal_path] {
        if !path.exists() {
            fs::write(path, "").unwrap();
        }
    }
    let config_path = root.join("collect.toml");
    fs::write(
        &config_path,
        format!(
            r#"schema_version = "1"

[freshness]
max_book_age_ms = 5000
max_source_age_ms = 10000
qty_tolerance = "0.00000001"

[mandate]
advisory = true
allowed_instruments = ["BTC/USDT"]
allowed_direction = "both"
max_leverage = "1"
max_exposure = "0.01"
max_loss_per_trade = "50"
max_loss_per_day = "100"
time_horizon = "intraday"

[[runners]]
label = "paper-levels"
grpc_endpoint = "{}"
mode = "levels"
venue = "PAPER"
environment = "LOCAL"
execution_mode = "exchange"
account_alias = "paper-levels-alias"
instrument = "BTC/USDT"
instrument_type = "spot"
journal_path = "{}"

[runners.venue_source]
kind = "paper"
rest_base = "{}"

[[runners]]
label = "paper-strategies-shadow"
grpc_endpoint = "{}"
mode = "strategies"
venue = "PAPER"
environment = "LOCAL"
execution_mode = "shadow"
account_alias = "paper-strategies-shadow-alias"
instrument = "BTC/USDT"
instrument_type = "spot"
journal_path = "{}"

[runners.venue_source]
kind = "paper"
rest_base = "{}"
"#,
            levels.grpc_url,
            levels.journal_path.display(),
            levels.http,
            strategies.grpc_url,
            strategies.journal_path.display(),
            strategies.http,
        ),
    )
    .unwrap();

    let db = format!(
        "tr_paper_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let reports = root.join("reports");
    let collect = Command::new(bin())
        .env("TRADING_REPORT_MONGO_URI", test_mongo_uri())
        .env("TRADING_REPORT_MONGO_DB", &db)
        .env("TRADING_REPORT_DEPLOYMENT_ID", "paper-lab")
        .args([
            "scheduled",
            "--config",
            config_path.to_str().unwrap(),
            "--reports-dir",
            reports.to_str().unwrap(),
            "--timeout-secs",
            "120",
            "--deployment-id",
            "paper-lab",
        ])
        .output()
        .expect("scheduled collect");
    assert!(
        collect.status.success(),
        "scheduled collect failed: {}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let latest = reports.join("latest");
    let saved = fs::read_to_string(latest.join("report.json")).expect("latest report.json");
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(latest.join("store-meta.json")).unwrap()).unwrap();
    let report_id = meta["report_id"].as_str().expect("report_id");
    assert_eq!(meta["report_json_sha256"], sha256_hex(saved.as_bytes()));

    let read = Command::new(bin())
        .env("TRADING_REPORT_MONGO_URI", test_mongo_uri())
        .env("TRADING_REPORT_MONGO_DB", &db)
        .args(["read-json", "--report-id", report_id, "--output", "-"])
        .output()
        .expect("read-json");
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    let stdout = String::from_utf8(read.stdout).unwrap();
    assert_eq!(stdout.trim_end(), saved.trim_end());
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["subjects"].as_array().map(|a| a.len()), Some(2));

    let latest_read = Command::new(bin())
        .env("TRADING_REPORT_MONGO_URI", test_mongo_uri())
        .env("TRADING_REPORT_MONGO_DB", &db)
        .args([
            "read-json",
            "--latest",
            "--deployment-id",
            "paper-lab",
            "--output",
            "-",
        ])
        .output()
        .expect("read latest");
    assert!(latest_read.status.success());
    assert_eq!(
        String::from_utf8(latest_read.stdout).unwrap().trim_end(),
        saved.trim_end()
    );
}
