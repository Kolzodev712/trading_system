use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::model::JournalRow;

const KNOWN_STATES: &[&str] = &[
    "prepared",
    "submitted",
    "unknown",
    "acknowledged",
    "rejected",
    "terminal",
    "recovery_required",
];

#[derive(Debug, Clone, Deserialize)]
struct RawRecord {
    client_order_id: String,
    #[serde(default)]
    strategy_id: Option<String>,
    #[serde(default)]
    level_id: Option<String>,
    state: String,
    created_at_ms: u64,
    last_check_at_ms: u64,
}

/// Latest row per client order id. Missing file or unknown state is an error.
pub fn load_latest_journal(path: &Path) -> Result<Vec<JournalRow>> {
    if !path.exists() {
        return Err(anyhow!(
            "journal file missing: {} (not treated as empty)",
            path.display()
        ));
    }
    let file = File::open(path).with_context(|| format!("open journal {}", path.display()))?;
    let mut latest: HashMap<String, RawRecord> = HashMap::new();
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("journal line {}", idx + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: RawRecord = serde_json::from_str(&line)
            .with_context(|| format!("malformed journal row {}", idx + 1))?;
        let state = record.state.to_ascii_lowercase();
        if !KNOWN_STATES.contains(&state.as_str()) {
            return Err(anyhow!(
                "unknown journal state '{}' for {}",
                record.state,
                record.client_order_id
            ));
        }
        latest.insert(record.client_order_id.clone(), record);
    }
    Ok(latest
        .into_values()
        .map(|r| JournalRow {
            ownership: r.strategy_id.or(r.level_id),
            client_order_id: r.client_order_id,
            state: r.state,
            created_at_ms: r.created_at_ms,
            last_check_at_ms: r.last_check_at_ms,
        })
        .collect())
}

pub fn unresolved_rows(rows: &[JournalRow]) -> Vec<JournalRow> {
    rows.iter()
        .filter(|row| {
            matches!(
                row.state.to_ascii_lowercase().as_str(),
                "prepared" | "submitted" | "unknown" | "recovery_required"
            )
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_is_error_not_empty() {
        let err = load_latest_journal(Path::new("/tmp/trading-report-missing-journal.jsonl"))
            .expect_err("missing");
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn latest_state_wins_and_unknown_state_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("j.jsonl");
        let mut f = File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"client_order_id":"c1","state":"submitted","created_at_ms":1,"last_check_at_ms":1}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"client_order_id":"c1","state":"terminal","created_at_ms":1,"last_check_at_ms":2}}"#
        )
        .unwrap();
        let rows = load_latest_journal(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, "terminal");

        writeln!(
            f,
            r#"{{"client_order_id":"c2","state":"exploded","created_at_ms":1,"last_check_at_ms":1}}"#
        )
        .unwrap();
        assert!(load_latest_journal(&path).is_err());
    }

    #[test]
    fn empty_existing_file_is_known_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        File::create(&path).unwrap();
        assert!(load_latest_journal(&path).unwrap().is_empty());
    }
}
