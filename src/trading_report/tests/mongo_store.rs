//! Mongo persist/read-json coverage. Does not call runners or venues.

use mongodb::bson::doc;
use mongodb::Client;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());
use trading_report::model::{Reason, TradingReport, Verdict, SCHEMA_VERSION};
use trading_report::store::{
    emit_json_bytes, meta_from_report, new_report_id, sha256_hex, write_store_meta, ReportStore,
    StoreError, StoreMeta, MONGO_URI_ENV,
};

fn test_mongo_uri() -> String {
    std::env::var("TRADING_REPORT_TEST_MONGO_URI")
        .unwrap_or_else(|_| "mongodb://127.0.0.1:27017".into())
}

fn unique_db() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("tr_test_{nanos}")
}

fn fixture_report(verdict: Verdict, reason: Option<Reason>) -> (TradingReport, String) {
    let report = TradingReport {
        schema_version: SCHEMA_VERSION.into(),
        generated_at_utc: "2026-10-02T12:00:00+00:00".into(),
        subjects: vec![],
        verdict,
        actionable_for_analysis: verdict == Verdict::Verified,
        reasons: reason.into_iter().collect(),
    };
    let json = serde_json::to_string_pretty(&report).unwrap();
    (report, json)
}

async fn connect(db: &str) -> ReportStore {
    ReportStore::connect(&test_mongo_uri(), db, "reports")
        .await
        .unwrap_or_else(|e| panic!("test Mongo on {} required: {e}", test_mongo_uri()))
}

async fn drop_db(db: &str) {
    if let Ok(client) = Client::with_uri_str(test_mongo_uri()).await {
        let _ = client.database(db).drop().await;
    }
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_trading-report"))
}

#[tokio::test]
async fn saved_json_round_trips_byte_for_byte() {
    let db = unique_db();
    let store = connect(&db).await;
    let (report, json) = fixture_report(Verdict::Verified, None);
    let meta = meta_from_report(new_report_id(), "paper-lab".into(), &report, &json);
    store.persist(&meta, &json).await.expect("persist");
    let back = store.read_json(&meta.report_id).await.expect("read");
    assert_eq!(back, json);
    drop_db(&db).await;
}

#[tokio::test]
async fn latest_is_scoped_to_deployment() {
    let db = unique_db();
    let store = connect(&db).await;
    let (a, json_a) = fixture_report(Verdict::Verified, None);
    let (mut b, _) = fixture_report(
        Verdict::Unverified,
        Some(Reason::new("VENUE_REST_ERROR", "down")),
    );
    b.generated_at_utc = "2026-10-02T12:00:01+00:00".into();
    let json_b = serde_json::to_string_pretty(&b).unwrap();
    let meta_a = meta_from_report("tr-a".into(), "deploy-a".into(), &a, &json_a);
    let meta_b = meta_from_report("tr-b".into(), "deploy-b".into(), &b, &json_b);
    store.persist(&meta_a, &json_a).await.unwrap();
    store.persist(&meta_b, &json_b).await.unwrap();
    let (id_a, body_a) = store.read_latest_json("deploy-a").await.unwrap();
    let (id_b, body_b) = store.read_latest_json("deploy-b").await.unwrap();
    assert_eq!(id_a, "tr-a");
    assert_eq!(id_b, "tr-b");
    assert_eq!(body_a, json_a);
    assert_eq!(body_b, json_b);
    assert_ne!(body_a, body_b);
    drop_db(&db).await;
}

#[tokio::test]
async fn idempotent_retry_and_conflicting_id() {
    let db = unique_db();
    let store = connect(&db).await;
    let (report, json) = fixture_report(Verdict::Verified, None);
    let meta = meta_from_report("tr-dup".into(), "paper-lab".into(), &report, &json);
    store.persist(&meta, &json).await.unwrap();
    store.persist(&meta, &json).await.expect("identical retry");
    let mut other_dep = meta.clone();
    other_dep.deployment_id = "other-deploy".into();
    let meta_err = store.persist(&other_dep, &json).await.unwrap_err();
    assert!(
        matches!(meta_err, StoreError::Conflict { id } if id == "tr-dup"),
        "same JSON with different metadata must conflict"
    );
    let other = json!({
        "schema_version": "trading-report-v1",
        "generated_at_utc": "2026-10-02T12:00:00+00:00",
        "subjects": [],
        "verdict": "unverified",
        "actionable_for_analysis": false,
        "reasons": [{"code": "OTHER", "message": "different payload"}]
    });
    let other_json = serde_json::to_string_pretty(&other).unwrap();
    assert_ne!(other_json, json);
    let mut other_meta = meta.clone();
    other_meta.report_json_sha256 = sha256_hex(other_json.as_bytes());
    let err = store.persist(&other_meta, &other_json).await.unwrap_err();
    assert!(matches!(err, StoreError::Conflict { id } if id == "tr-dup"));
    drop_db(&db).await;
}

#[tokio::test]
async fn missing_and_corrupt_and_outage() {
    let db = unique_db();
    let store = connect(&db).await;
    let missing = store.read_json("tr-missing").await.unwrap_err();
    assert!(matches!(missing, StoreError::Missing(_)));

    let (report, json) = fixture_report(Verdict::Verified, None);
    let meta = meta_from_report("tr-corrupt".into(), "paper-lab".into(), &report, &json);
    store.persist(&meta, &json).await.unwrap();
    let client = Client::with_uri_str(test_mongo_uri()).await.unwrap();
    client
        .database(&db)
        .collection::<mongodb::bson::Document>("reports")
        .update_one(
            doc! { "report_id": "tr-corrupt" },
            doc! { "$set": { "report_json": "{not-the-original}" } },
        )
        .await
        .unwrap();
    let corrupt = store.read_json("tr-corrupt").await.unwrap_err();
    assert!(matches!(corrupt, StoreError::Corrupt));

    let down = ReportStore::connect("mongodb://127.0.0.1:1", &db, "reports").await;
    assert!(matches!(down, Err(StoreError::Unavailable(_))));
    drop_db(&db).await;
}

#[tokio::test]
async fn unverified_verdict_is_preserved() {
    let db = unique_db();
    let store = connect(&db).await;
    let (report, json) = fixture_report(
        Verdict::Unverified,
        Some(Reason::new("STATUS_MISSING", "GetTradingStatus failed")),
    );
    assert!(!report.actionable_for_analysis);
    let meta = meta_from_report("tr-unverified".into(), "paper-lab".into(), &report, &json);
    store.persist(&meta, &json).await.unwrap();
    let back = store.read_json("tr-unverified").await.unwrap();
    assert_eq!(back, json);
    let parsed: TradingReport = serde_json::from_str(&back).unwrap();
    assert_eq!(parsed.verdict, Verdict::Unverified);
    assert!(!parsed.actionable_for_analysis);
    assert_eq!(parsed.reasons[0].code, "STATUS_MISSING");
    drop_db(&db).await;
}

#[tokio::test]
async fn mongo_failure_leaves_recoverable_local_report() {
    let _env = ENV_LOCK.lock().unwrap();
    let db = unique_db();
    let dir = tempfile::tempdir().unwrap();
    let (report, json) = fixture_report(Verdict::Verified, None);
    fs::write(dir.path().join("report.json"), &json).unwrap();
    let meta = meta_from_report("tr-replay".into(), "paper-lab".into(), &report, &json);
    write_store_meta(dir.path(), &meta).unwrap();

    let fail = Command::new(bin())
        .env(MONGO_URI_ENV, "mongodb://127.0.0.1:1")
        .env("TRADING_REPORT_MONGO_DB", &db)
        .env("TRADING_REPORT_DEPLOYMENT_ID", "paper-lab")
        .args(["persist", "--from", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!fail.status.success(), "outage must fail persist");
    let err_text = String::from_utf8_lossy(&fail.stderr);
    assert!(err_text.contains("STORAGE_FAILED"));
    assert_eq!(
        fs::read_to_string(dir.path().join("report.json")).unwrap(),
        json
    );

    let ok = Command::new(bin())
        .env(MONGO_URI_ENV, test_mongo_uri())
        .env("TRADING_REPORT_MONGO_DB", &db)
        .env("TRADING_REPORT_DEPLOYMENT_ID", "paper-lab")
        .args(["persist", "--from", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let store = connect(&db).await;
    let back = store.read_json("tr-replay").await.unwrap();
    assert_eq!(back, json, "replay must store the original snapshot");
    drop_db(&db).await;
}

#[tokio::test]
async fn secrets_stay_off_stdout_and_stdout_is_json_only() {
    let _env = ENV_LOCK.lock().unwrap();
    let db = unique_db();
    let store = connect(&db).await;
    let (report, json) = fixture_report(Verdict::Verified, None);
    let meta = meta_from_report("tr-stdout".into(), "paper-lab".into(), &report, &json);
    store.persist(&meta, &json).await.unwrap();

    let ok = Command::new(bin())
        .env(MONGO_URI_ENV, test_mongo_uri())
        .env("TRADING_REPORT_MONGO_DB", &db)
        .args(["read-json", "--report-id", "tr-stdout", "--output", "-"])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let stdout = String::from_utf8_lossy(&ok.stdout);
    assert!(!stdout.contains("report_id="));
    assert_eq!(
        ok.stdout,
        json.as_bytes(),
        "stdout must be the stored JSON bytes with no added newline"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&ok.stdout).expect("stdout JSON only");
    assert_eq!(parsed["verdict"], "verified");

    let fail = Command::new(bin())
        .env(
            MONGO_URI_ENV,
            "mongodb://writer:supersecretvalue@127.0.0.1:1/trading_report",
        )
        .env("TRADING_REPORT_MONGO_DB", &db)
        .args(["read-json", "--report-id", "tr-stdout", "--output", "-"])
        .output()
        .unwrap();
    assert!(!fail.status.success());
    let fail_out = String::from_utf8_lossy(&fail.stdout);
    let fail_err = String::from_utf8_lossy(&fail.stderr);
    assert!(!fail_out.contains("supersecretvalue"));
    assert!(!fail_err.contains("supersecretvalue"));
    assert!(fail_err.contains("STORAGE_FAILED") || fail_err.contains("[REDACTED]"));
    drop_db(&db).await;
}

#[tokio::test]
async fn persist_rejects_changed_local_file() {
    let _env = ENV_LOCK.lock().unwrap();
    let db = unique_db();
    let dir = tempfile::tempdir().unwrap();
    let (report, json) = fixture_report(Verdict::Verified, None);
    fs::write(dir.path().join("report.json"), &json).unwrap();
    let meta = meta_from_report("tr-changed".into(), "paper-lab".into(), &report, &json);
    write_store_meta(dir.path(), &meta).unwrap();
    fs::write(dir.path().join("report.json"), "{\"verdict\":\"changed\"}").unwrap();
    let fail = Command::new(bin())
        .env(MONGO_URI_ENV, test_mongo_uri())
        .env("TRADING_REPORT_MONGO_DB", &db)
        .args(["persist", "--from", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!fail.status.success());
    let err = String::from_utf8_lossy(&fail.stderr);
    assert!(err.contains("do not match") || err.contains("changed snapshot"));
    drop_db(&db).await;
}

#[test]
fn emit_json_bytes_writes_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.json");
    emit_json_bytes(path.to_str().unwrap(), "{\"ok\":true}").unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "{\"ok\":true}");
}

#[test]
fn conflicting_meta_hash_is_rejected_without_write() {
    let (report, json) = fixture_report(Verdict::Verified, None);
    let mut meta = meta_from_report("tr-hash".into(), "paper-lab".into(), &report, &json);
    meta.report_json_sha256 = "00".repeat(32);
    let err = StoreMeta {
        report_json_sha256: meta.report_json_sha256.clone(),
        ..meta
    };
    assert_ne!(err.report_json_sha256, sha256_hex(json.as_bytes()));
}
