//! Process-local restart snapshot. Not position truth. See design log 012.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SNAPSHOT_FILE: &str = "config/cex/runtime-snapshot.json";
pub const RESTART_FLAG_FILE: &str = "config/cex/restart.requested";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotStrategy {
    pub strategy_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    pub mode: String,
    pub global_trading: bool,
    pub strategies: Vec<SnapshotStrategy>,
    #[serde(default = "default_on_stop")]
    pub on_stop: String,
}

fn default_on_stop() -> String {
    "cancel_working".to_string()
}

impl RuntimeSnapshot {
    pub fn load(path: impl AsRef<Path>) -> Result<Option<Self>, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Ok(Some(
            serde_json::from_str(&data).map_err(|e| e.to_string())?,
        ))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            path,
            serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n",
        )
        .map_err(|e| e.to_string())
    }
}

pub fn write_restart_flag(path: impl AsRef<Path>) -> Result<(), String> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, "1\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip_has_no_qty() {
        let snap = RuntimeSnapshot {
            mode: "strategies".into(),
            global_trading: false,
            strategies: vec![SnapshotStrategy {
                strategy_id: "PropHold:BTC/USDT".into(),
                enabled: true,
            }],
            on_stop: "cancel_working".into(),
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(!json.contains("qty"));
        assert!(!json.contains("api"));
        let parsed: RuntimeSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, snap);
    }
}
