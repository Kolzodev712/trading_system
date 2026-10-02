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
    let generated_at_utc = chrono::Utc::now().to_rfc3339();
    let now_ms = chrono::Utc::now().timestamp_millis();
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
        subjects.push(subject_report(
            runner_cfg,
            &cfg.freshness,
            mandate.clone(),
            &generated_at_utc,
            now_ms,
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

pub fn write_outputs(out_dir: &Path, outputs: &CollectOutputs) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    atomic_write(
        out_dir.join("report.json"),
        &serde_json::to_vec_pretty(&outputs.report)?,
    )?;
    atomic_write(out_dir.join("report.md"), outputs.markdown.as_bytes())?;
    atomic_write(
        out_dir.join("manifest.json"),
        &serde_json::to_vec_pretty(&outputs.manifest)?,
    )?;
    reject_secrets(&outputs.markdown)?;
    reject_secrets(&serde_json::to_string(&outputs.report)?)?;
    Ok(())
}

fn atomic_write(path: std::path::PathBuf, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

fn reject_secrets(text: &str) -> Result<()> {
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
