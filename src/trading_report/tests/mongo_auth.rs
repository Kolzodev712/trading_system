//! Authenticated writer vs read-only reader. Requires Docker.
//!
//! A `read` role cannot create indexes. `connect_reader` must succeed for that user.

use mongodb::bson::doc;
use mongodb::Client;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use trading_report::model::{TradingReport, Verdict, SCHEMA_VERSION};
use trading_report::store::{meta_from_report, ReportStore, StoreError};

const ROOT_USER: &str = "root";
const ROOT_PASS: &str = "root-lab-pass";
const WRITER_USER: &str = "report_writer";
const WRITER_PASS: &str = "writer-lab-pass";
const READER_USER: &str = "report_reader";
const READER_PASS: &str = "reader-lab-pass";
const DB: &str = "trading_report_auth";

struct AuthMongo {
    name: String,
    port: u16,
}

impl Drop for AuthMongo {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn allocate_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn start_auth_mongo() -> AuthMongo {
    let name = format!("tr-auth-{}", std::process::id());
    let port = allocate_port();
    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &name,
            "-p",
            &format!("127.0.0.1:{port}:27017"),
            "-e",
            &format!("MONGO_INITDB_ROOT_USERNAME={ROOT_USER}"),
            "-e",
            &format!("MONGO_INITDB_ROOT_PASSWORD={ROOT_PASS}"),
            "mongo:7.0",
        ])
        .status()
        .expect("docker run");
    assert!(
        status.success(),
        "docker run mongo:7.0 failed; Docker is required for auth tests"
    );
    let root_uri = format!("mongodb://{ROOT_USER}:{ROOT_PASS}@127.0.0.1:{port}/admin");
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Ok(client) = Client::with_uri_str(&root_uri).await {
            if client
                .database("admin")
                .run_command(doc! { "ping": 1 })
                .await
                .is_ok()
            {
                break;
            }
        }
        if Instant::now() > deadline {
            panic!("auth Mongo did not become ready on 127.0.0.1:{port}");
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    let client = Client::with_uri_str(&root_uri).await.unwrap();
    client
        .database(DB)
        .run_command(doc! {
            "createUser": WRITER_USER,
            "pwd": WRITER_PASS,
            "roles": [{ "role": "readWrite", "db": DB }]
        })
        .await
        .expect("create writer");
    client
        .database(DB)
        .run_command(doc! {
            "createUser": READER_USER,
            "pwd": READER_PASS,
            "roles": [{ "role": "read", "db": DB }]
        })
        .await
        .expect("create reader");
    AuthMongo { name, port }
}

fn writer_uri(port: u16) -> String {
    format!("mongodb://{WRITER_USER}:{WRITER_PASS}@127.0.0.1:{port}/{DB}?authSource={DB}")
}

fn reader_uri(port: u16) -> String {
    format!("mongodb://{READER_USER}:{READER_PASS}@127.0.0.1:{port}/{DB}?authSource={DB}")
}

fn fixture() -> (TradingReport, String) {
    let report = TradingReport {
        schema_version: SCHEMA_VERSION.into(),
        generated_at_utc: "2026-10-02T12:00:00+00:00".into(),
        subjects: vec![],
        verdict: Verdict::Unverified,
        actionable_for_analysis: false,
        reasons: vec![],
    };
    let json = serde_json::to_string_pretty(&report).unwrap();
    (report, json)
}

#[tokio::test]
async fn authenticated_reader_can_read_without_index_privileges() {
    let mongo = start_auth_mongo().await;
    let writer = ReportStore::connect_writer(&writer_uri(mongo.port), DB, "reports")
        .await
        .expect("writer connect + indexes");
    let (report, json) = fixture();
    let meta = meta_from_report("tr-auth".into(), "paper-lab".into(), &report, &json);
    writer.persist(&meta, &json).await.expect("writer persist");

    let reader_as_writer =
        ReportStore::connect_writer(&reader_uri(mongo.port), DB, "reports").await;
    assert!(
        matches!(reader_as_writer, Err(StoreError::Unavailable(_))),
        "read role must not create indexes"
    );

    let reader = ReportStore::connect_reader(&reader_uri(mongo.port), DB, "reports")
        .await
        .expect("reader connect must not create indexes");
    let back = reader.read_json("tr-auth").await.expect("reader find");
    assert_eq!(back, json);
    assert_eq!(back.as_bytes(), json.as_bytes());

    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_trading-report"));
    let out = std::process::Command::new(bin)
        .env("TRADING_REPORT_MONGO_URI", reader_uri(mongo.port))
        .env("TRADING_REPORT_MONGO_DB", DB)
        .args(["read-json", "--report-id", "tr-auth", "--output", "-"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, json.as_bytes());
    assert!(!String::from_utf8_lossy(&out.stderr).contains(READER_PASS));
    assert!(!String::from_utf8_lossy(&out.stdout).contains(READER_PASS));
}
