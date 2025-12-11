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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_audit_logger_new() {
        let _logger = AuditLogger::new("/tmp/test_audit.log".to_string());
        // Should not panic
        assert!(true);
    }

    #[test]
    fn test_audit_logger_log() {
        let test_file = "/tmp/test_audit_logger.log";
        // Clean up if exists
        let _ = fs::remove_file(test_file);

        let logger = AuditLogger::new(test_file.to_string());
        let entry = AuditLogEntry {
            timestamp: 1234567890,
            change_type: ChangeType::LevelUpdate,
            user: None,
            details: "Test update".to_string(),
            success: true,
        };

        let result = logger.log(entry);
        assert!(result.is_ok());

        // Verify file was created and contains the entry
        let content = fs::read_to_string(test_file).unwrap();
        assert!(content.contains("Test update"));
        assert!(content.contains("1234567890"));

        // Clean up
        let _ = fs::remove_file(test_file);
    }

    #[test]
    fn test_audit_logger_multiple_entries() {
        let test_file = "/tmp/test_audit_logger_multiple.log";
        let _ = fs::remove_file(test_file);

        let logger = AuditLogger::new(test_file.to_string());

        let entry1 = AuditLogEntry {
            timestamp: 1000,
            change_type: ChangeType::LevelUpdate,
            user: None,
            details: "First update".to_string(),
            success: true,
        };

        let entry2 = AuditLogEntry {
            timestamp: 2000,
            change_type: ChangeType::RiskLimitUpdate,
            user: Some("user1".to_string()),
            details: "Second update".to_string(),
            success: false,
        };

        assert!(logger.log(entry1).is_ok());
        assert!(logger.log(entry2).is_ok());

        let content = fs::read_to_string(test_file).unwrap();
        assert!(content.contains("First update"));
        assert!(content.contains("Second update"));
        assert!(content.contains("user1"));

        let _ = fs::remove_file(test_file);
    }
}
