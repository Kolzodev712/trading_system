//! LiveConfig Service - Main orchestrator for runtime configuration updates
use crate::audit::logger::{AuditLogEntry, AuditLogger, ChangeType};
use crate::types::*;
use crate::updater::{ConfigUpdater, LevelUpdater, RiskUpdater};
use crate::validator::ConfigValidator;
use std::sync::Arc;
use trading_engine::levels::LevelManager;
use trading_engine::risk::RiskManager;

pub struct LiveConfigService {
    validator: Arc<ConfigValidator>,
    level_updater: Arc<LevelUpdater>,
    risk_updater: Arc<RiskUpdater>,
    config_updater: Arc<ConfigUpdater>,
    audit_logger: Arc<AuditLogger>,
    // Persistence will be passed to updaters when they need to persist
    level_persistence: Option<Arc<trading_engine::levels::persistence::LevelPersistence>>,
}

#[derive(Debug, thiserror::Error)]
pub enum LiveConfigError {
    #[error("Validation error: {0}")]
    ValidationError(#[from] crate::validator::ValidationError),

    #[error("Update error: {0}")]
    UpdateError(#[from] crate::updater::level_updater::UpdateError),

    #[error("Risk update error: {0}")]
    RiskUpdateError(#[from] crate::updater::risk_updater::UpdateError),

    #[error("Config update error: {0}")]
    ConfigUpdateError(#[from] crate::updater::config_updater::UpdateError),

    #[error("Audit error: {0}")]
    AuditError(#[from] crate::audit::logger::AuditError),

    #[error("Persistence error: {0}")]
    PersistenceError(String),
}

impl LiveConfigService {
    pub fn new(
        level_manager: Arc<tokio::sync::RwLock<LevelManager>>,
        risk_manager: Arc<tokio::sync::RwLock<RiskManager>>,
        level_persistence: Option<Arc<trading_engine::levels::persistence::LevelPersistence>>,
        audit_log_file: String,
    ) -> Self {
        let validator = Arc::new(ConfigValidator::new(
            level_manager.clone(),
            risk_manager.clone(),
        ));

        let level_updater = Arc::new(LevelUpdater::new(level_manager.clone()));
        let risk_updater = Arc::new(RiskUpdater::new(risk_manager));
        let config_updater = Arc::new(ConfigUpdater::new());
        let audit_logger = Arc::new(AuditLogger::new(audit_log_file));

        Self {
            validator,
            level_updater,
            risk_updater,
            config_updater,
            audit_logger,
            level_persistence,
        }
    }

    /// Update a level value
    pub async fn update_level_value(
        &self,
        request: LevelUpdateRequest,
    ) -> Result<UpdateResponse, LiveConfigError> {
        // 1. Validate the change
        self.validator
            .validate_level_update(request.level_id.as_str(), &request.field, &request.value)
            .await?;

        // 2. Update in-memory level configuration
        self.level_updater
            .update_field(
                request.level_id.as_str(),
                request.field,
                request.value.clone(),
            )
            .await?;

        // 3. Persist to MongoDB if persistence is enabled
        if let Some(persistence) = &self.level_persistence {
            // Get the updated level config from LevelManager and persist it
            let level_manager = self.level_updater.level_manager.read().await;
            if let Some(level) = level_manager.get_level(&request.level_id) {
                let config = level.config.clone();
                drop(level_manager);
                if let Err(e) = persistence.update_level(&config).await {
                    tracing::warn!("Failed to persist level update to MongoDB: {}", e);
                }
            }
        }

        // 4. Log audit entry
        self.audit_logger.log(AuditLogEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            change_type: ChangeType::LevelUpdate,
            user: None,
            details: format!(
                "Updated level {} field {:?} to {:?}",
                request.level_id.as_str(),
                request.field,
                request.value
            ),
            success: true,
        })?;

        Ok(UpdateResponse::success(format!(
            "Level {} updated successfully",
            request.level_id.as_str()
        )))
    }

    /// Update risk limit
    pub async fn update_risk_limit(
        &self,
        request: RiskLimitUpdateRequest,
    ) -> Result<UpdateResponse, LiveConfigError> {
        // 1. Validate the change
        self.validator
            .validate_risk_limit_update(&request.limit_type, &request.value)?;

        // 2. Update in-memory risk limit
        self.risk_updater
            .update_risk_limit(request.limit_type, request.value)
            .await?;

        // 3. Persist to MongoDB if persistence is enabled
        if let Some(persistence) = &self.level_persistence {
            // Get updated risk limits from RiskUpdater and persist them
            let limits = (*self.risk_updater).get_limits().await;
            if let Err(e) = persistence.save_risk_limits(&limits).await {
                tracing::warn!("Failed to persist risk limit update to MongoDB: {}", e);
            }
        }

        // 4. Log audit entry
        self.audit_logger.log(AuditLogEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            change_type: ChangeType::RiskLimitUpdate,
            user: None,
            details: format!(
                "Updated risk limit {:?} to {}",
                request.limit_type, request.value
            ),
            success: true,
        })?;

        Ok(UpdateResponse::success(format!(
            "Risk limit {:?} updated successfully",
            request.limit_type
        )))
    }

    /// Update bot configuration
    pub async fn update_bot_config(
        &self,
        request: ConfigUpdateRequest,
    ) -> Result<UpdateResponse, LiveConfigError> {
        // 1. Validate the change
        self.validator
            .validate_config_update(&request.path, &request.value)?;

        // 2. Update in-memory configuration
        self.config_updater
            .update_config(&request.path, &request.value)
            .await?;

        // 3. Log audit entry
        self.audit_logger.log(AuditLogEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            change_type: ChangeType::ConfigUpdate,
            user: None,
            details: format!(
                "Updated config path {} to {:?}",
                request.path.path.as_str(),
                request.value
            ),
            success: true,
        })?;

        Ok(UpdateResponse::success(format!(
            "Config {} updated successfully",
            request.path.path.as_str()
        )))
    }
}
