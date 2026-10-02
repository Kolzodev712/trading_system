//! Immutable Mongo persistence for a combined five-item report.
//!
//! Stores the exact `report.json` UTF-8 bytes plus indexed metadata.
//! Does not reserialize the report into a different JSON shape.

use anyhow::Context;
use mongodb::bson::{doc, DateTime as BsonDateTime, Document};
use mongodb::error::{ErrorKind, WriteFailure};
use mongodb::options::{ClientOptions, FindOneOptions, IndexOptions};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

use crate::collect::{atomic_write, redact_secrets, reject_secrets};
use crate::model::TradingReport;

pub const DEFAULT_DB: &str = "trading_report";
pub const DEFAULT_COLLECTION: &str = "reports";
pub const MONGO_URI_ENV: &str = "TRADING_REPORT_MONGO_URI";
pub const MONGO_URI_FILE_ENV: &str = "TRADING_REPORT_MONGO_URI_FILE";
pub const MONGO_DB_ENV: &str = "TRADING_REPORT_MONGO_DB";
pub const MONGO_COLLECTION_ENV: &str = "TRADING_REPORT_MONGO_COLLECTION";
pub const DEPLOYMENT_ENV: &str = "TRADING_REPORT_DEPLOYMENT_ID";
pub const STORE_META_FILE: &str = "store-meta.json";
const MONGO_MAX_BSON_BYTES: usize = 16 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("STORAGE_FAILED: {0}")]
    Unavailable(String),
    #[error("STORAGE_CONFLICT: report_id {id} already exists with different JSON")]
    Conflict { id: String },
    #[error("STORAGE_TOO_LARGE: encoded document is {size} bytes; Mongo supports at most {limit}")]
    TooLarge { size: usize, limit: usize },
    #[error("REPORT_MISSING: {0}")]
    Missing(String),
    #[error("REPORT_CORRUPT: stored payload hash does not match report_json")]
    Corrupt,
    #[error("{0}")]
    Invalid(String),
}

impl StoreError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Missing(_) => 2,
            Self::Corrupt => 3,
            Self::Conflict { .. } | Self::TooLarge { .. } | Self::Invalid(_) => 4,
            Self::Unavailable(_) => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreMeta {
    pub report_id: String,
    pub deployment_id: String,
    pub collected_at_utc: String,
    pub schema_version: String,
    pub verdict: String,
    pub actionable_for_analysis: bool,
    pub subject_ids: Vec<String>,
    pub report_json_sha256: String,
}

pub struct ReportStore {
    collection: Collection<Document>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn new_report_id() -> String {
    format!("tr-{}", uuid::Uuid::new_v4())
}

pub fn subject_ids(report: &TradingReport) -> Vec<String> {
    report
        .subjects
        .iter()
        .map(|s| {
            format!(
                "{}:{}:{}:{}",
                s.identity.runner_label,
                s.identity.account_alias,
                s.identity.instrument,
                s.identity
                    .execution_mode
                    .value
                    .as_deref()
                    .unwrap_or("unknown")
            )
        })
        .collect()
}

pub fn meta_from_report(
    report_id: String,
    deployment_id: String,
    report: &TradingReport,
    report_json: &str,
) -> StoreMeta {
    StoreMeta {
        report_id,
        deployment_id,
        collected_at_utc: report.generated_at_utc.clone(),
        schema_version: report.schema_version.clone(),
        verdict: report.verdict.as_str().to_string(),
        actionable_for_analysis: report.actionable_for_analysis,
        subject_ids: subject_ids(report),
        report_json_sha256: sha256_hex(report_json.as_bytes()),
    }
}

pub fn write_store_meta(out_dir: &Path, meta: &StoreMeta) -> anyhow::Result<()> {
    let raw = redact_secrets(&serde_json::to_string_pretty(meta)?);
    reject_secrets(&raw)?;
    atomic_write(out_dir.join(STORE_META_FILE), raw.as_bytes())
}

pub fn load_store_meta(dir: &Path) -> anyhow::Result<StoreMeta> {
    let path = dir.join(STORE_META_FILE);
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).context("parse store-meta.json")
}

pub fn load_saved_report_json(dir: &Path) -> anyhow::Result<String> {
    let path = dir.join("report.json");
    std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

pub fn mongo_uri_from_env() -> Result<String, StoreError> {
    if let Ok(path) = std::env::var(MONGO_URI_FILE_ENV) {
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            StoreError::Unavailable(format!("cannot read {MONGO_URI_FILE_ENV}: {e}"))
        })?;
        let uri = raw.trim().to_string();
        if uri.is_empty() {
            return Err(StoreError::Unavailable(format!(
                "{MONGO_URI_FILE_ENV} is empty"
            )));
        }
        return Ok(uri);
    }
    match std::env::var(MONGO_URI_ENV) {
        Ok(uri) if !uri.trim().is_empty() => Ok(uri),
        _ => Err(StoreError::Unavailable(format!(
            "{MONGO_URI_ENV} or {MONGO_URI_FILE_ENV} is required for Mongo persistence"
        ))),
    }
}

pub fn mongo_db_name() -> String {
    std::env::var(MONGO_DB_ENV).unwrap_or_else(|_| DEFAULT_DB.into())
}

pub fn mongo_collection_name() -> String {
    std::env::var(MONGO_COLLECTION_ENV).unwrap_or_else(|_| DEFAULT_COLLECTION.into())
}

pub fn deployment_id_from(cli: Option<&str>) -> Result<String, StoreError> {
    if let Some(id) = cli.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(id.to_string());
    }
    match std::env::var(DEPLOYMENT_ENV) {
        Ok(id) if !id.trim().is_empty() => Ok(id),
        _ => Err(StoreError::Invalid(format!(
            "--deployment-id or {DEPLOYMENT_ENV} is required for Mongo persistence"
        ))),
    }
}

pub fn sanitize_error(text: &str) -> String {
    let mut out = text.to_string();
    for key in [MONGO_URI_ENV, "MONGODB_URI", "MONGO_URI"] {
        if let Ok(val) = std::env::var(key) {
            if val.len() >= 8 {
                out = out.replace(&val, "[REDACTED]");
            }
        }
    }
    if let Ok(path) = std::env::var(MONGO_URI_FILE_ENV) {
        if let Ok(val) = std::fs::read_to_string(path) {
            let val = val.trim();
            if val.len() >= 8 {
                out = out.replace(val, "[REDACTED]");
            }
        }
    }
    redact_mongo_uris(&out)
}

fn redact_mongo_uris(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(pos) = rest
        .find("mongodb+srv://")
        .into_iter()
        .chain(rest.find("mongodb://"))
        .min()
    {
        let scheme = if rest[pos..].starts_with("mongodb+srv://") {
            "mongodb+srv://"
        } else {
            "mongodb://"
        };
        out.push_str(&rest[..pos]);
        out.push_str(scheme);
        out.push_str("[REDACTED]");
        let tail = &rest[pos + scheme.len()..];
        let skip = tail
            .find(|c: char| matches!(c, '/' | '?' | ' ' | '"' | '\n'))
            .unwrap_or(tail.len());
        rest = &tail[skip..];
    }
    out.push_str(rest);
    out
}

fn collected_at_bson(rfc3339: &str) -> BsonDateTime {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|dt| BsonDateTime::from_millis(dt.timestamp_millis()))
        .unwrap_or_else(|_| BsonDateTime::from_millis(chrono::Utc::now().timestamp_millis()))
}

impl ReportStore {
    async fn connect_base(uri: &str, database: &str, collection: &str) -> Result<Self, StoreError> {
        let mut opts = tokio::time::timeout(CONNECT_TIMEOUT, ClientOptions::parse(uri))
            .await
            .map_err(|_| StoreError::Unavailable("Mongo URI parse timed out".into()))?
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?;
        opts.server_selection_timeout = Some(CONNECT_TIMEOUT);
        opts.connect_timeout = Some(CONNECT_TIMEOUT);
        let client = Client::with_options(opts)
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?;
        // Ping the application database. A read-only user cannot use admin.
        tokio::time::timeout(
            CONNECT_TIMEOUT,
            client.database(database).run_command(doc! { "ping": 1 }),
        )
        .await
        .map_err(|_| StoreError::Unavailable("Mongo connect timed out".into()))?
        .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?;
        Ok(Self {
            collection: client.database(database).collection::<Document>(collection),
        })
    }

    /// Writer path: ping + create the identity/latest indexes.
    pub async fn connect_writer(
        uri: &str,
        database: &str,
        collection: &str,
    ) -> Result<Self, StoreError> {
        let store = Self::connect_base(uri, database, collection).await?;
        tokio::time::timeout(CONNECT_TIMEOUT, store.ensure_indexes())
            .await
            .map_err(|_| StoreError::Unavailable("Mongo index setup timed out".into()))??;
        Ok(store)
    }

    /// Reader path: ping and find only. Never create indexes.
    pub async fn connect_reader(
        uri: &str,
        database: &str,
        collection: &str,
    ) -> Result<Self, StoreError> {
        Self::connect_base(uri, database, collection).await
    }

    pub async fn connect(uri: &str, database: &str, collection: &str) -> Result<Self, StoreError> {
        Self::connect_writer(uri, database, collection).await
    }

    pub async fn connect_from_env() -> Result<Self, StoreError> {
        let uri = mongo_uri_from_env()?;
        Self::connect_writer(&uri, &mongo_db_name(), &mongo_collection_name()).await
    }

    pub async fn connect_reader_from_env() -> Result<Self, StoreError> {
        let uri = mongo_uri_from_env()?;
        Self::connect_reader(&uri, &mongo_db_name(), &mongo_collection_name()).await
    }

    async fn ensure_indexes(&self) -> Result<(), StoreError> {
        let unique = IndexModel::builder()
            .keys(doc! { "report_id": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("report_id_unique".to_string())
                    .build(),
            )
            .build();
        let latest = IndexModel::builder()
            .keys(doc! { "deployment_id": 1, "collected_at": -1 })
            .options(
                IndexOptions::builder()
                    .name("deployment_latest".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_index(unique)
            .await
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?;
        self.collection
            .create_index(latest)
            .await
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?;
        Ok(())
    }

    fn document(meta: &StoreMeta, report_json: &str) -> Document {
        doc! {
            "report_id": &meta.report_id,
            "deployment_id": &meta.deployment_id,
            "collected_at_utc": &meta.collected_at_utc,
            "collected_at": collected_at_bson(&meta.collected_at_utc),
            "schema_version": &meta.schema_version,
            "verdict": &meta.verdict,
            "actionable_for_analysis": meta.actionable_for_analysis,
            "subject_ids": &meta.subject_ids,
            "report_json": report_json,
            "report_json_sha256": &meta.report_json_sha256,
        }
    }

    pub fn check_document_size(meta: &StoreMeta, report_json: &str) -> Result<(), StoreError> {
        let doc = Self::document(meta, report_json);
        let encoded = mongodb::bson::to_vec(&doc)
            .map_err(|e| StoreError::Invalid(format!("cannot encode Mongo document: {e}")))?;
        if encoded.len() > MONGO_MAX_BSON_BYTES {
            return Err(StoreError::TooLarge {
                size: encoded.len(),
                limit: MONGO_MAX_BSON_BYTES,
            });
        }
        Ok(())
    }

    pub async fn persist(&self, meta: &StoreMeta, report_json: &str) -> Result<(), StoreError> {
        if meta.report_id.trim().is_empty() || meta.deployment_id.trim().is_empty() {
            return Err(StoreError::Invalid(
                "report_id and deployment_id are required".into(),
            ));
        }
        let expected = sha256_hex(report_json.as_bytes());
        if expected != meta.report_json_sha256 {
            return Err(StoreError::Invalid(
                "store-meta SHA-256 does not match report.json bytes".into(),
            ));
        }
        Self::check_document_size(meta, report_json)?;
        let doc = Self::document(meta, report_json);
        match self.collection.insert_one(doc).await {
            Ok(_) => Ok(()),
            Err(e) if is_duplicate_key(&e) => self.confirm_idempotent(meta, report_json).await,
            Err(e) => Err(StoreError::Unavailable(sanitize_error(&e.to_string()))),
        }
    }

    async fn confirm_idempotent(
        &self,
        meta: &StoreMeta,
        report_json: &str,
    ) -> Result<(), StoreError> {
        let existing = self
            .collection
            .find_one(doc! { "report_id": &meta.report_id })
            .await
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?
            .ok_or_else(|| {
                StoreError::Unavailable(format!(
                    "duplicate key for {} but document is missing",
                    meta.report_id
                ))
            })?;
        let stored = existing
            .get_str("report_json")
            .map_err(|_| StoreError::Corrupt)?;
        if stored == report_json && metadata_matches(&existing, meta) {
            Ok(())
        } else {
            Err(StoreError::Conflict {
                id: meta.report_id.clone(),
            })
        }
    }

    pub async fn read_json(&self, report_id: &str) -> Result<String, StoreError> {
        let doc = self
            .collection
            .find_one(doc! { "report_id": report_id })
            .await
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?
            .ok_or_else(|| StoreError::Missing(format!("report_id {report_id}")))?;
        extract_verified_json(&doc)
    }

    pub async fn read_latest_json(
        &self,
        deployment_id: &str,
    ) -> Result<(String, String), StoreError> {
        if deployment_id.trim().is_empty() {
            return Err(StoreError::Invalid(
                "--deployment-id is required for --latest".into(),
            ));
        }
        let opts = FindOneOptions::builder()
            .sort(doc! { "collected_at": -1 })
            .build();
        let doc = self
            .collection
            .find_one(doc! { "deployment_id": deployment_id })
            .with_options(opts)
            .await
            .map_err(|e| StoreError::Unavailable(sanitize_error(&e.to_string())))?
            .ok_or_else(|| {
                StoreError::Missing(format!("no reports for deployment {deployment_id}"))
            })?;
        let id = doc.get_str("report_id").unwrap_or("unknown").to_string();
        let json = extract_verified_json(&doc)?;
        Ok((id, json))
    }
}

fn extract_verified_json(doc: &Document) -> Result<String, StoreError> {
    let json = doc
        .get_str("report_json")
        .map_err(|_| StoreError::Corrupt)?
        .to_string();
    let stored_hash = doc
        .get_str("report_json_sha256")
        .map_err(|_| StoreError::Corrupt)?;
    if sha256_hex(json.as_bytes()) != stored_hash {
        return Err(StoreError::Corrupt);
    }
    Ok(json)
}

fn is_duplicate_key(err: &mongodb::error::Error) -> bool {
    match err.kind.as_ref() {
        ErrorKind::Write(WriteFailure::WriteError(we)) => we.code == 11000,
        ErrorKind::Write(WriteFailure::WriteConcernError(we)) => we.code == 11000,
        ErrorKind::BulkWrite(bwe) => {
            bwe.write_errors.iter().any(|(_, e)| e.code == 11000)
                || bwe.write_concern_errors.iter().any(|e| e.code == 11000)
        }
        _ => {
            let msg = err.to_string();
            msg.contains("E11000") || msg.contains("duplicate key")
        }
    }
}

pub fn load_replay_payload(dir: &Path) -> Result<(StoreMeta, String), StoreError> {
    let meta = load_store_meta(dir).map_err(|e| {
        StoreError::Invalid(format!("replay requires unmodified store-meta.json: {e:#}"))
    })?;
    let json = load_saved_report_json(dir)
        .map_err(|e| StoreError::Invalid(format!("replay requires saved report.json: {e:#}")))?;
    if sha256_hex(json.as_bytes()) != meta.report_json_sha256 {
        return Err(StoreError::Invalid(
            "report.json bytes do not match store-meta SHA-256; refusing to persist a changed snapshot"
                .into(),
        ));
    }
    Ok((meta, json))
}

fn metadata_matches(doc: &Document, meta: &StoreMeta) -> bool {
    let ids = doc
        .get_array("subject_ids")
        .ok()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    doc.get_str("report_id").ok() == Some(meta.report_id.as_str())
        && doc.get_str("deployment_id").ok() == Some(meta.deployment_id.as_str())
        && doc.get_str("collected_at_utc").ok() == Some(meta.collected_at_utc.as_str())
        && doc.get_str("schema_version").ok() == Some(meta.schema_version.as_str())
        && doc.get_str("verdict").ok() == Some(meta.verdict.as_str())
        && doc.get_bool("actionable_for_analysis").ok() == Some(meta.actionable_for_analysis)
        && doc.get_str("report_json_sha256").ok() == Some(meta.report_json_sha256.as_str())
        && ids == meta.subject_ids
}

pub fn emit_json_bytes(output: &str, json: &str) -> anyhow::Result<()> {
    if output == "-" {
        use std::io::Write;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(json.as_bytes())?;
        stdout.flush()?;
        return Ok(());
    }
    let path = PathBuf::from(output);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    atomic_write(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_check_rejects_oversized_payload() {
        let huge = "x".repeat(MONGO_MAX_BSON_BYTES + 1024);
        let meta = StoreMeta {
            report_id: "tr-size".into(),
            deployment_id: "dep".into(),
            collected_at_utc: "2026-10-02T00:00:00Z".into(),
            schema_version: "trading-report-v1".into(),
            verdict: "verified".into(),
            actionable_for_analysis: true,
            subject_ids: vec!["a".into()],
            report_json_sha256: sha256_hex(huge.as_bytes()),
        };
        let err = ReportStore::check_document_size(&meta, &huge).unwrap_err();
        match err {
            StoreError::TooLarge { size, limit } => {
                assert!(size > limit);
                assert_eq!(limit, MONGO_MAX_BSON_BYTES);
            }
            other => panic!("expected TooLarge, got {other}"),
        }
    }

    #[test]
    fn sanitize_redacts_mongo_uri() {
        let raw = "failed mongodb://writer:s3cret@127.0.0.1:27017/trading_report";
        let cleaned = sanitize_error(raw);
        assert!(!cleaned.contains("s3cret"));
        assert!(cleaned.contains("[REDACTED]"));
    }

    #[test]
    fn hash_is_stable_for_exact_bytes() {
        assert_eq!(
            sha256_hex(b"{\"verdict\":\"unverified\"}"),
            sha256_hex(b"{\"verdict\":\"unverified\"}")
        );
        assert_ne!(
            sha256_hex(b"{\"verdict\":\"unverified\"}"),
            sha256_hex(b"{\"verdict\":\"verified\"}")
        );
    }

    #[test]
    fn replay_rejects_changed_report_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let json = "{\"verdict\":\"verified\"}";
        let meta = StoreMeta {
            report_id: "tr-replay".into(),
            deployment_id: "paper-lab".into(),
            collected_at_utc: "2026-10-02T00:00:00Z".into(),
            schema_version: "trading-report-v1".into(),
            verdict: "verified".into(),
            actionable_for_analysis: true,
            subject_ids: vec![],
            report_json_sha256: sha256_hex(json.as_bytes()),
        };
        write_store_meta(dir.path(), &meta).unwrap();
        std::fs::write(dir.path().join("report.json"), "{\"verdict\":\"changed\"}").unwrap();
        let err = load_replay_payload(dir.path()).unwrap_err();
        assert!(matches!(err, StoreError::Invalid(msg) if msg.contains("do not match")));
    }
}
