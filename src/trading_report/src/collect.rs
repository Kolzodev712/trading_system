use anyhow::{Context, Result};
use std::path::Path;

use crate::config::ReportConfig;
use crate::journal_source::load_latest_journal;
use crate::model::{ReportManifest, SubjectReport, TradingReport, Verdict, SCHEMA_VERSION};
use crate::render::render_markdown;
use crate::runner_source::snapshot_runner;
use crate::venue_source::HttpVenueReader;
use crate::verify::subject_report;

pub struct CollectOutputs {
    pub report: TradingReport,
    pub markdown: String,
    pub manifest: ReportManifest,
}

pub async fn collect_report(cfg: &ReportConfig) -> Result<CollectOutputs> {
    let mandate = cfg.mandate_view();
    let mut subjects = Vec::new();
    for runner_cfg in &cfg.runners {
        let runner = snapshot_runner(runner_cfg).await;
        let venue = HttpVenueReader::new(runner_cfg.venue_source.clone())?
            .snapshot(&runner_cfg.instrument, &runner_cfg.instrument_type)
            .await;
        if venue.http_methods.iter().any(|m| m != "GET") {
            anyhow::bail!("{}: venue adapter used a non-GET method", runner_cfg.label);
        }
        let journal = load_latest_journal(&runner_cfg.journal_path).map_err(|e| e.to_string());
        let completed_at = chrono::Utc::now();
        subjects.push(subject_report(
            runner_cfg,
            &cfg.freshness,
            mandate.clone(),
            &completed_at.to_rfc3339(),
            completed_at.timestamp_millis(),
            &runner,
            &venue,
            journal,
        ));
    }
    let overall = subjects
        .iter()
        .map(|s| s.verdict)
        .fold(Verdict::Verified, Verdict::worse);
    let reasons = subjects.iter().flat_map(|s| s.reasons.clone()).collect();
    let generated_at_utc = chrono::Utc::now().to_rfc3339();
    let report = TradingReport {
        schema_version: SCHEMA_VERSION.to_string(),
        generated_at_utc: generated_at_utc.clone(),
        subjects,
        verdict: overall,
        actionable_for_analysis: overall == Verdict::Verified,
        reasons,
    };
    let markdown = render_markdown(&report);
    let manifest = ReportManifest {
        schema_version: SCHEMA_VERSION.to_string(),
        generated_at_utc,
        source_revisions: cfg
            .runners
            .iter()
            .filter_map(|r| r.executable_revision.clone())
            .collect(),
        config_hash: None,
    };
    Ok(CollectOutputs {
        report,
        markdown,
        manifest,
    })
}

#[derive(Debug)]
pub struct WrittenOutputs {
    pub report_json: String,
}

pub fn write_outputs(out_dir: &Path, outputs: &CollectOutputs) -> Result<WrittenOutputs> {
    let report_json = redact_secrets(&serde_json::to_string_pretty(&outputs.report)?);
    let markdown = redact_secrets(&outputs.markdown);
    let manifest_json = redact_secrets(&serde_json::to_string_pretty(&outputs.manifest)?);
    reject_secrets(&report_json)?;
    reject_secrets(&markdown)?;
    reject_secrets(&manifest_json)?;
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    atomic_write(out_dir.join("report.json"), report_json.as_bytes())?;
    atomic_write(out_dir.join("report.md"), markdown.as_bytes())?;
    atomic_write(out_dir.join("manifest.json"), manifest_json.as_bytes())?;
    Ok(WrittenOutputs { report_json })
}

pub fn atomic_write(path: std::path::PathBuf, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("out")
    ));
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

pub fn redact_secrets(text: &str) -> String {
    let mut out = text.to_string();
    for key in [
        "BYBIT_DEMO_API_KEY",
        "BYBIT_DEMO_API_SECRET",
        "BYBIT_API_KEY",
        "BYBIT_API_SECRET",
        "TRADING_OPERATOR_TOKEN",
        "TRADING_REPORT_MONGO_URI",
        "MONGODB_URI",
        "MONGO_URI",
    ] {
        if let Ok(val) = std::env::var(key) {
            if val.len() >= 8 {
                out = out.replace(&val, "[REDACTED]");
            }
        }
    }
    if let Ok(path) = std::env::var("TRADING_REPORT_MONGO_URI_FILE") {
        if let Ok(val) = std::fs::read_to_string(path) {
            let val = val.trim();
            if val.len() >= 8 {
                out = out.replace(val, "[REDACTED]");
            }
        }
    }
    out
}

pub fn reject_secrets(text: &str) -> Result<()> {
    let lowered = text.to_ascii_lowercase();
    for needle in ["api_secret", "x-operator-token", "begin rsa private"] {
        if lowered.contains(needle) {
            anyhow::bail!("refusing to write output that looks like it contains {needle}");
        }
    }
    Ok(())
}

pub fn subject_has_reason(subject: &SubjectReport, code: &str) -> bool {
    subject.reasons.iter().any(|r| r.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Reason, ReportManifest, TradingReport, SCHEMA_VERSION};

    fn outputs_with_text(extra: &str) -> CollectOutputs {
        CollectOutputs {
            report: TradingReport {
                schema_version: SCHEMA_VERSION.into(),
                generated_at_utc: "2026-10-01T00:00:00Z".into(),
                subjects: vec![],
                verdict: Verdict::Verified,
                actionable_for_analysis: true,
                reasons: vec![Reason::new("NOTE", extra)],
            },
            markdown: format!("# report\n{extra}\n"),
            manifest: ReportManifest {
                schema_version: SCHEMA_VERSION.into(),
                generated_at_utc: "2026-10-01T00:00:00Z".into(),
                source_revisions: vec![],
                config_hash: None,
            },
        }
    }

    #[test]
    fn secrets_are_rejected_before_any_file_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("missing");
        let err =
            write_outputs(&out, &outputs_with_text("leaked api_secret value")).expect_err("secret");
        assert!(err.to_string().contains("api_secret"));
        assert!(!out.join("report.json").exists());
        assert!(!out.join("report.md").exists());
        assert!(!out.join("manifest.json").exists());
    }

    #[test]
    fn env_secret_values_are_redacted_before_write() {
        std::env::set_var("BYBIT_DEMO_API_SECRET", "supersecretvalue");
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("ok");
        write_outputs(&out, &outputs_with_text("token=supersecretvalue")).unwrap();
        let md = std::fs::read_to_string(out.join("report.md")).unwrap();
        assert!(!md.contains("supersecretvalue"));
        assert!(md.contains("[REDACTED]"));
        std::env::remove_var("BYBIT_DEMO_API_SECRET");
    }
}
