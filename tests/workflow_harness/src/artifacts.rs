use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{ScenarioResult, WorkflowMatrix};

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

pub fn git_sha(dir: &Path) -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "UNKNOWN".into())
}

pub fn file_sha256(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return "MISSING".into();
    };
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn finish_matrix(
    sink: &ArtifactSink,
    mut manifest: Manifest,
    scenarios: Vec<ScenarioResult>,
    layers: Vec<String>,
) -> Result<WorkflowMatrix> {
    manifest.ended_utc = Some(Utc::now().to_rfc3339());
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
