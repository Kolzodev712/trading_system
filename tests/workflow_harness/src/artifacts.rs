use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{Capability, ScenarioResult, WorkflowMatrix};

#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    pub run_id: String,
    pub started_utc: String,
    pub ended_utc: Option<String>,
    pub trading_system_sha: String,
    pub k_paper_sha: String,
    pub ui_sha: String,
    pub runner_sha256: String,
    pub k_paper_bin_sha256: String,
    pub layer: String,
    pub config_hashes: BTreeMap<String, String>,
    pub capability_matrix: Capability,
}

pub struct ArtifactSink {
    pub dir: PathBuf,
    pub run_id: String,
}

impl ArtifactSink {
    pub fn create(root: &Path, run_id: &str) -> Result<Self> {
        let dir = root.join(run_id);
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        fs::create_dir_all(dir.join("rest"))?;
        fs::create_dir_all(dir.join("grpc"))?;
        Ok(Self {
            dir,
            run_id: run_id.to_string(),
        })
    }

    pub fn write_json(&self, name: &str, value: &impl Serialize) -> Result<()> {
        let path = self.dir.join(name);
        fs::write(&path, serde_json::to_vec_pretty(value)?)
            .with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    pub fn copy_if_exists(&self, src: &Path, name: &str) -> Result<()> {
        if src.exists() {
            fs::copy(src, self.dir.join(name))?;
        }
        Ok(())
    }

    pub fn write_text(&self, name: &str, body: &str) -> Result<()> {
        fs::write(self.dir.join(name), body)?;
        Ok(())
    }
}

pub fn git_dirty(dir: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(true)
}

pub fn git_sha(dir: &Path) -> String {
    let sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "UNKNOWN".into());
    if sha != "UNKNOWN" && git_dirty(dir) {
        format!("{sha}+dirty")
    } else {
        sha
    }
}

pub fn file_sha256(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return "MISSING".into();
    };
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn collect_config_hashes(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    collect_named_hashes(
        root,
        root,
        &[
            "bot.json",
            "levels.json",
            "instruments.yaml",
            "strategies.json",
        ],
        &mut out,
    );
    out
}

fn collect_named_hashes(
    root: &Path,
    dir: &Path,
    names: &[&str],
    out: &mut BTreeMap<String, String>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_named_hashes(root, &path, names, out);
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if names.contains(&name) {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.insert(rel.display().to_string(), file_sha256(&path));
        }
    }
}

pub fn finish_matrix(
    sink: &ArtifactSink,
    mut manifest: Manifest,
    scenarios: Vec<ScenarioResult>,
    layers: Vec<String>,
) -> Result<WorkflowMatrix> {
    manifest.ended_utc = Some(Utc::now().to_rfc3339());
    if manifest.config_hashes.is_empty() {
        manifest.config_hashes = collect_config_hashes(&sink.dir);
    }
    sink.write_json("manifest.json", &manifest)?;
    let matrix = WorkflowMatrix {
        run_id: sink.run_id.clone(),
        artifact_dir: sink.dir.clone(),
        layers,
        scenarios,
    };
    sink.write_json("results.json", &matrix)?;
    let mut md = String::from("# Workflow matrix\n\n");
    md.push_str(&format!(
        "run `{}`  \nlayer `{}`  \n\n",
        matrix.run_id, manifest.layer
    ));
    md.push_str("| workflow | venue | instrument | mode | verdict |\n|---|---|---|---|---|\n");
    for s in &matrix.scenarios {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            s.workflow, s.venue, s.instrument, s.execution_mode, s.verdict
        ));
    }
    sink.write_text("MATRIX.md", &md)?;
    Ok(matrix)
}

pub fn utc_now() -> String {
    Utc::now().to_rfc3339()
}
