//! Audit logger for tracking configuration changes
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use thiserror::Error;

/// Audit log entry
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogEntry {
    pub timestamp: u64,
    pub change_type: ChangeType,
    pub user: Option<String>,
    pub details: String,
    pub success: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ChangeType {
    LevelUpdate,
    ConfigUpdate,
    RiskLimitUpdate,
}

pub struct AuditLogger {
    log_file: String,
}

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl AuditLogger {
    pub fn new(log_file: String) -> Self {
        Self { log_file }
    }

    pub fn log(&self, entry: AuditLogEntry) -> Result<(), AuditError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;

        let json = serde_json::to_string(&entry)?;
        writeln!(file, "{}", json)?;

        Ok(())
    }
}
