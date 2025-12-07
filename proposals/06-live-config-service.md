# Proposal 06: Live Config Service

## Overview

This proposal outlines the implementation of a LiveConfig Service that enables runtime configuration changes to the trading system without requiring a restart. The service allows modifying level values, bot configuration, and risk limits while the bot is running, with validation, persistence, and real-time updates to affected components.

**Key Insight**: The LiveConfig Service is implemented after Proposal 05 (Data Handling and Persistence), as it depends on MongoDB for configuration persistence and requires the Level Manager and Risk Manager to be in place.

## Key Requirements

1. **Runtime Configuration Updates**: Modify level values, bot config, and risk limits without restart
2. **Validation**: Validate all changes before applying them
3. **Persistence**: Persist changes to MongoDB immediately
4. **Real-Time Updates**: Notify affected components of changes
5. **Safety Checks**: Ensure changes don't violate risk limits or cause unsafe states
6. **Atomic Operations**: Changes are atomic (all-or-nothing)
7. **Audit Trail**: Log all configuration changes for debugging and compliance

## Architecture Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│              Trading System (Monolith)                   │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  LiveConfig Service                              │   │
│  │  - Receives config change requests               │   │
│  │  - Validates changes                             │   │
│  │  - Updates in-memory config                      │   │
│  │  - Persists to MongoDB                           │   │
│  │  - Notifies affected components                  │   │
│  └──────────────┬───────────────────────────────────┘   │
│                 │                                        │
│                 ├──→ Level Manager (update levels)       │
│                 ├──→ Risk Manager (update limits)         │
│                 ├──→ Config Manager (update bot config)   │
│                 └──→ MongoDB (persist changes)             │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  TUI CLI / gRPC Server                          │   │
│  │  - Exposes LiveConfig endpoints                  │   │
│  └──────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────┘
```

### Data Flow

```
1. Configuration Change Request:
   TUI CLI / gRPC → LiveConfig Service → Validate → Update → Persist → Notify

2. Level Update:
   LiveConfig → Level Manager → Update in-memory level → Trading Engine notified

3. Risk Limit Update:
   LiveConfig → Risk Manager → Update in-memory limits → Risk checks use new limits

4. Bot Config Update:
   LiveConfig → Config Manager → Update in-memory config → All components notified
```

## Component Structure

```
trading_system/
├── src/
│   ├── live_config/                   # NEW: LiveConfig Service crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── service.rs             # Main service orchestrator
│   │   │   ├── level_updater.rs       # Updates level values at runtime
│   │   │   ├── config_updater.rs      # Updates bot configuration
│   │   │   ├── risk_updater.rs        # Updates risk limits
│   │   │   ├── validator.rs           # Validates configuration changes
│   │   │   ├── persistence.rs         # Persists changes to MongoDB
│   │   │   ├── notifier.rs            # Notifies affected components
│   │   │   │
│   │   │   ├── types/                 # Type definitions
│   │   │   │   ├── mod.rs
│   │   │   │   ├── level_field.rs      # Level field types
│   │   │   │   ├── config_path.rs      # Config path types
│   │   │   │   └── update_request.rs   # Update request types
│   │   │   │
│   │   │   └── audit/                  # Audit logging
│   │   │       ├── mod.rs
│   │   │       └── logger.rs           # Audit log writer
│   │   │
│   │   └── tests/
│   │       ├── unit/
│   │       │   ├── validator_tests.rs
│   │       │   ├── updater_tests.rs
│   │       │   └── persistence_tests.rs
│   │       └── integration/
│   │           └── service_tests.rs
```

## Detailed Component Design

### 1. Update Request Types (`live_config/src/types/update_request.rs`)

**Purpose**: Defines types for configuration update requests.

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Request to update a level value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelUpdateRequest {
    pub level_id: String,
    pub field: LevelField,
    pub value: LevelValue,
}

/// Request to update bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdateRequest {
    pub path: ConfigPath,
    pub value: ConfigValue,
}

/// Request to update risk limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimitUpdateRequest {
    pub limit_type: RiskLimitType,
    pub value: Decimal,
}

/// Response to an update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: u64,
}
```

### 2. Level Field Types (`live_config/src/types/level_field.rs`)

**Purpose**: Defines level field types that can be updated.

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Fields that can be updated in a level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LevelField {
    /// Static price (overrides indicator-based price)
    Price,
    
    /// Static quantity (overrides indicator-based quantity)
    Quantity,
    
    /// Stop-loss price
    StopLoss,
    
    /// Take-profit price
    TakeProfit,
    
    /// Risk limit for this level
    RiskLimit,
    
    /// Maximum position size for this level
    MaxPositionSize,
    
    /// Enable/disable the level
    Enabled,
    
    /// Repost on fill (true/false)
    RepostOnFill,
    
    /// Repost on cancel (true/false)
    RepostOnCancel,
}

/// Value types for level fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LevelValue {
    Decimal(Decimal),
    String(String),
    Boolean(bool),
    Integer(i64),
}

impl LevelValue {
    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            LevelValue::Decimal(d) => Some(*d),
            LevelValue::Integer(i) => Some(Decimal::from(*i)),
            _ => None,
        }
    }
    
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            LevelValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
    
    pub fn as_string(&self) -> Option<String> {
        match self {
            LevelValue::String(s) => Some(s.clone()),
            _ => None,
        }
    }
}
```

### 3. Config Path Types (`live_config/src/types/config_path.rs`)

**Purpose**: Defines configuration path types for bot configuration updates.

```rust
use serde::{Deserialize, Serialize};

/// Path to a configuration value (dot-separated)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigPath {
    pub path: String,  // e.g., "strategy.buy_threshold_percent"
}

impl ConfigPath {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }
    
    pub fn segments(&self) -> Vec<&str> {
        self.path.split('.').collect()
    }
    
    pub fn validate(&self) -> Result<(), ConfigPathError> {
        if self.path.is_empty() {
            return Err(ConfigPathError::EmptyPath);
        }
        
        // Validate path format (alphanumeric, dots, underscores)
        if !self.path.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_') {
            return Err(ConfigPathError::InvalidCharacters);
        }
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigPathError {
    #[error("Empty path")]
    EmptyPath,
    
    #[error("Invalid characters in path")]
    InvalidCharacters,
}

/// Value types for configuration updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Decimal(rust_decimal::Decimal),
}
```

### 4. Config Validator (`live_config/src/validator.rs`)

**Purpose**: Validates configuration changes before applying them.

```rust
use std::sync::{Arc, RwLock};
use rust_decimal::Decimal;
use trading_engine::level::manager::LevelManager;
use trading_engine::risk::manager::RiskManager;
use crate::types::{LevelField, LevelValue, ConfigPath, ConfigValue, RiskLimitType};

pub struct ConfigValidator {
    level_manager: Arc<RwLock<LevelManager>>,
    risk_manager: Arc<RwLock<RiskManager>>,
}

impl ConfigValidator {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        risk_manager: Arc<RwLock<RiskManager>>,
    ) -> Self {
        Self {
            level_manager,
            risk_manager,
        }
    }
    
    /// Validate a level update request
    pub fn validate_level_update(
        &self,
        level_id: &str,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), ValidationError> {
        // 1. Check if level exists
        let level_manager = self.level_manager.read().unwrap();
        let level = level_manager.get_level(level_id)
            .ok_or(ValidationError::LevelNotFound {
                level_id: level_id.to_string(),
            })?;
        
        // 2. Check if field is valid for this level
        self.validate_field_for_level(&level, field)?;
        
        // 3. Validate value type matches field type
        self.validate_value_type(field, value)?;
        
        // 4. Validate value range/constraints
        self.validate_value_constraints(field, value)?;
        
        // 5. Check if change violates risk limits
        self.validate_risk_constraints(level_id, field, value)?;
        
        // 6. Check if change is safe to apply at runtime
        self.validate_runtime_safety(level_id, field, value)?;
        
        Ok(())
    }
    
    fn validate_field_for_level(
        &self,
        level: &trading_engine::level::level::Level,
        field: &LevelField,
    ) -> Result<(), ValidationError> {
        // Some fields may not be applicable to certain level types
        // For example, stop-loss may not apply to market orders
        match field {
            LevelField::StopLoss | LevelField::TakeProfit => {
                if level.config.order_type == OrderType::Market {
                    return Err(ValidationError::FieldNotApplicable {
                        field: format!("{:?}", field),
                        reason: "Stop-loss/take-profit not applicable to market orders".to_string(),
                    });
                }
            }
            _ => {}
        }
        
        Ok(())
    }
    
    fn validate_value_type(
        &self,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), ValidationError> {
        match field {
            LevelField::Price
            | LevelField::Quantity
            | LevelField::StopLoss
            | LevelField::TakeProfit
            | LevelField::RiskLimit
            | LevelField::MaxPositionSize => {
                if value.as_decimal().is_none() {
                    return Err(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Decimal".to_string(),
                    });
                }
            }
            LevelField::Enabled
            | LevelField::RepostOnFill
            | LevelField::RepostOnCancel => {
                if value.as_bool().is_none() {
                    return Err(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Boolean".to_string(),
                    });
                }
            }
        }
        
        Ok(())
    }
    
    fn validate_value_constraints(
        &self,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), ValidationError> {
        match field {
            LevelField::Price | LevelField::StopLoss | LevelField::TakeProfit => {
                let price = value.as_decimal()
                    .ok_or(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Decimal".to_string(),
                    })?;
                
                if price <= Decimal::ZERO {
                    return Err(ValidationError::InvalidValue {
                        field: format!("{:?}", field),
                        reason: "Price must be greater than zero".to_string(),
                    });
                }
            }
            LevelField::Quantity => {
                let qty = value.as_decimal()
                    .ok_or(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Decimal".to_string(),
                    })?;
                
                if qty <= Decimal::ZERO {
                    return Err(ValidationError::InvalidValue {
                        field: format!("{:?}", field),
                        reason: "Quantity must be greater than zero".to_string(),
                    });
                }
            }
            LevelField::RiskLimit | LevelField::MaxPositionSize => {
                let limit = value.as_decimal()
                    .ok_or(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Decimal".to_string(),
                    })?;
                
                if limit < Decimal::ZERO {
                    return Err(ValidationError::InvalidValue {
                        field: format!("{:?}", field),
                        reason: "Limit cannot be negative".to_string(),
                    });
                }
            }
            _ => {}
        }
        
        Ok(())
    }
    
    fn validate_risk_constraints(
        &self,
        level_id: &str,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), ValidationError> {
        let risk_manager = self.risk_manager.read().unwrap();
        
        // Check if the change would violate global risk limits
        match field {
            LevelField::MaxPositionSize => {
                let new_max = value.as_decimal()
                    .ok_or(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Decimal".to_string(),
                    })?;
                
                // Check against global max position size
                let global_max = risk_manager.get_max_position_size();
                if new_max > global_max {
                    return Err(ValidationError::RiskLimitViolation {
                        field: format!("{:?}", field),
                        reason: format!("Max position size {} exceeds global limit {}", new_max, global_max),
                    });
                }
            }
            LevelField::RiskLimit => {
                let new_limit = value.as_decimal()
                    .ok_or(ValidationError::InvalidValueType {
                        field: format!("{:?}", field),
                        expected: "Decimal".to_string(),
                    })?;
                
                // Check against global risk limit
                let global_limit = risk_manager.get_max_risk_per_level();
                if new_limit > global_limit {
                    return Err(ValidationError::RiskLimitViolation {
                        field: format!("{:?}", field),
                        reason: format!("Risk limit {} exceeds global limit {}", new_limit, global_limit),
                    });
                }
            }
            _ => {}
        }
        
        Ok(())
    }
    
    fn validate_runtime_safety(
        &self,
        level_id: &str,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), ValidationError> {
        let level_manager = self.level_manager.read().unwrap();
        let level = level_manager.get_level(level_id)
            .ok_or(ValidationError::LevelNotFound {
                level_id: level_id.to_string(),
            })?;
        
        // Check if level has active orders
        if level.has_active_orders() {
            // Some fields cannot be changed while orders are active
            match field {
                LevelField::Price | LevelField::Quantity => {
                    return Err(ValidationError::UnsafeChange {
                        field: format!("{:?}", field),
                        reason: "Cannot change price/quantity while level has active orders".to_string(),
                    });
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    /// Validate a bot configuration update
    pub fn validate_config_update(
        &self,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), ValidationError> {
        // 1. Validate path format
        path.validate()?;
        
        // 2. Check if path is allowed to be updated at runtime
        self.validate_path_allowed(path)?;
        
        // 3. Validate value type matches expected type for path
        self.validate_path_value_type(path, value)?;
        
        // 4. Validate value constraints
        self.validate_path_value_constraints(path, value)?;
        
        Ok(())
    }
    
    fn validate_path_allowed(&self, path: &ConfigPath) -> Result<(), ValidationError> {
        // Some paths are not allowed to be updated at runtime
        let forbidden_paths = vec![
            "exchange.api_key",
            "exchange.api_secret",
            "exchange.testnet",
        ];
        
        for forbidden in forbidden_paths {
            if path.path.starts_with(forbidden) {
                return Err(ValidationError::PathNotAllowed {
                    path: path.path.clone(),
                    reason: "Security-sensitive configuration cannot be changed at runtime".to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    fn validate_path_value_type(
        &self,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), ValidationError> {
        // Validate value type based on path
        // This is a simplified example - actual implementation would be more comprehensive
        if path.path.contains("threshold") || path.path.contains("limit") {
            if matches!(value, ConfigValue::String(_) | ConfigValue::Boolean(_)) {
                return Err(ValidationError::InvalidValueType {
                    field: path.path.clone(),
                    expected: "Number or Decimal".to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    fn validate_path_value_constraints(
        &self,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), ValidationError> {
        // Validate value constraints based on path
        if path.path.contains("threshold") {
            if let ConfigValue::Number(n) = value {
                if *n < 0.0 || *n > 100.0 {
                    return Err(ValidationError::InvalidValue {
                        field: path.path.clone(),
                        reason: "Threshold must be between 0 and 100".to_string(),
                    });
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate a risk limit update
    pub fn validate_risk_limit_update(
        &self,
        limit_type: &RiskLimitType,
        value: &Decimal,
    ) -> Result<(), ValidationError> {
        if *value < Decimal::ZERO {
            return Err(ValidationError::InvalidValue {
                field: format!("{:?}", limit_type),
                reason: "Risk limit cannot be negative".to_string(),
            });
        }
        
        // Additional validation based on limit type
        match limit_type {
            RiskLimitType::MaxPositionSize => {
                // Check if any level has a max position size greater than the new limit
                let level_manager = self.level_manager.read().unwrap();
                for level in level_manager.get_all_levels() {
                    if level.config.max_position_size > *value {
                        return Err(ValidationError::RiskLimitViolation {
                            field: format!("{:?}", limit_type),
                            reason: format!(
                                "Level {} has max position size {} which exceeds new limit {}",
                                level.id, level.config.max_position_size, value
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Level not found: {level_id}")]
    LevelNotFound { level_id: String },
    
    #[error("Field not applicable: {field} - {reason}")]
    FieldNotApplicable { field: String, reason: String },
    
    #[error("Invalid value type for field {field}: expected {expected}")]
    InvalidValueType { field: String, expected: String },
    
    #[error("Invalid value for field {field}: {reason}")]
    InvalidValue { field: String, reason: String },
    
    #[error("Risk limit violation for field {field}: {reason}")]
    RiskLimitViolation { field: String, reason: String },
    
    #[error("Unsafe change for field {field}: {reason}")]
    UnsafeChange { field: String, reason: String },
    
    #[error("Path not allowed: {path} - {reason}")]
    PathNotAllowed { path: String, reason: String },
    
    #[error("Config path error: {0}")]
    ConfigPathError(#[from] crate::types::config_path::ConfigPathError),
}
```

### 5. Level Updater (`live_config/src/level_updater.rs`)

**Purpose**: Handles level value updates at runtime.

```rust
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use trading_engine::level::manager::LevelManager;
use crate::types::{LevelField, LevelValue};
use crate::notifier::ConfigNotifier;

pub struct LevelUpdater {
    level_manager: Arc<RwLock<LevelManager>>,
    notifier: Arc<ConfigNotifier>,
}

impl LevelUpdater {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        notifier: Arc<ConfigNotifier>,
    ) -> Self {
        Self {
            level_manager,
            notifier,
        }
    }
    
    /// Update a level field
    pub async fn update_field(
        &self,
        level_id: &str,
        field: LevelField,
        value: LevelValue,
    ) -> Result<(), UpdateError> {
        let mut level_manager = self.level_manager.write().unwrap();
        let level = level_manager.get_level_mut(level_id)
            .ok_or(UpdateError::LevelNotFound {
                level_id: level_id.to_string(),
            })?;
        
        // Update the field
        match field {
            LevelField::Price => {
                if let Some(price) = value.as_decimal() {
                    level.config.price = Some(price);
                }
            }
            LevelField::Quantity => {
                if let Some(qty) = value.as_decimal() {
                    level.config.quantity = Some(qty);
                }
            }
            LevelField::StopLoss => {
                if let Some(sl) = value.as_decimal() {
                    level.config.stop_loss = Some(sl);
                }
            }
            LevelField::TakeProfit => {
                if let Some(tp) = value.as_decimal() {
                    level.config.take_profit = Some(tp);
                }
            }
            LevelField::RiskLimit => {
                if let Some(limit) = value.as_decimal() {
                    level.config.risk_limit = Some(limit);
                }
            }
            LevelField::MaxPositionSize => {
                if let Some(max) = value.as_decimal() {
                    level.config.max_position_size = Some(max);
                }
            }
            LevelField::Enabled => {
                if let Some(enabled) = value.as_bool() {
                    level.config.enabled = enabled;
                }
            }
            LevelField::RepostOnFill => {
                if let Some(repost) = value.as_bool() {
                    level.config.repost_on_fill = repost;
                }
            }
            LevelField::RepostOnCancel => {
                if let Some(repost) = value.as_bool() {
                    level.config.repost_on_cancel = repost;
                }
            }
        }
        
        // Notify trading engine of the change
        self.notifier.notify_level_update(level_id, field, &value).await?;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Level not found: {level_id}")]
    LevelNotFound { level_id: String },
    
    #[error("Notification error: {0}")]
    NotificationError(#[from] crate::notifier::NotifierError),
}
```

### 6. Config Updater (`live_config/src/config_updater.rs`)

**Purpose**: Handles bot configuration updates.

```rust
use std::sync::{Arc, RwLock};
use crate::types::{ConfigPath, ConfigValue};
use crate::notifier::ConfigNotifier;
use core_logic::config::TradingSystemConfig;

pub struct ConfigUpdater {
    config: Arc<RwLock<TradingSystemConfig>>,
    notifier: Arc<ConfigNotifier>,
}

impl ConfigUpdater {
    pub fn new(
        config: Arc<RwLock<TradingSystemConfig>>,
        notifier: Arc<ConfigNotifier>,
    ) -> Self {
        Self {
            config,
            notifier,
        }
    }
    
    /// Update a configuration value
    pub async fn update_config(
        &self,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), UpdateError> {
        let mut config = self.config.write().unwrap();
        
        // Update the configuration based on path
        self.update_config_value(&mut config, path, value)?;
        
        // Notify all components of the change
        self.notifier.notify_config_update(path, value).await?;
        
        Ok(())
    }
    
    fn update_config_value(
        &self,
        config: &mut TradingSystemConfig,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), UpdateError> {
        let segments = path.segments();
        
        match segments.as_slice() {
            ["strategy", "buy_threshold_percent"] => {
                if let ConfigValue::Number(n) = value {
                    config.strategy.buy_threshold_percent = rust_decimal::Decimal::from_f64_retain(*n)
                        .ok_or(UpdateError::InvalidValue)?;
                }
            }
            ["strategy", "sell_threshold_percent"] => {
                if let ConfigValue::Number(n) = value {
                    config.strategy.sell_threshold_percent = rust_decimal::Decimal::from_f64_retain(*n)
                        .ok_or(UpdateError::InvalidValue)?;
                }
            }
            // Add more path handlers as needed
            _ => {
                return Err(UpdateError::UnknownPath {
                    path: path.path.clone(),
                });
            }
        }
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Unknown config path: {path}")]
    UnknownPath { path: String },
    
    #[error("Invalid value")]
    InvalidValue,
    
    #[error("Notification error: {0}")]
    NotificationError(#[from] crate::notifier::NotifierError),
}
```

### 7. Risk Updater (`live_config/src/risk_updater.rs`)

**Purpose**: Handles risk limit updates.

```rust
use std::sync::{Arc, RwLock};
use rust_decimal::Decimal;
use trading_engine::risk::manager::RiskManager;
use crate::types::RiskLimitType;
use crate::notifier::ConfigNotifier;

pub struct RiskUpdater {
    risk_manager: Arc<RwLock<RiskManager>>,
    notifier: Arc<ConfigNotifier>,
}

impl RiskUpdater {
    pub fn new(
        risk_manager: Arc<RwLock<RiskManager>>,
        notifier: Arc<ConfigNotifier>,
    ) -> Self {
        Self {
            risk_manager,
            notifier,
        }
    }
    
    /// Update a risk limit
    pub async fn update_risk_limit(
        &self,
        limit_type: RiskLimitType,
        value: Decimal,
    ) -> Result<(), UpdateError> {
        let mut risk_manager = self.risk_manager.write().unwrap();
        
        // Update the risk limit
        match limit_type {
            RiskLimitType::MaxPositionSize => {
                risk_manager.set_max_position_size(value);
            }
            RiskLimitType::MaxDailyLoss => {
                risk_manager.set_max_daily_loss(value);
            }
            RiskLimitType::MaxExposure => {
                risk_manager.set_max_exposure(value);
            }
            // Add more limit types as needed
        }
        
        // Notify trading engine of the change
        self.notifier.notify_risk_limit_update(limit_type, value).await?;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Notification error: {0}")]
    NotificationError(#[from] crate::notifier::NotifierError),
}
```

### 8. Config Persistence (`live_config/src/persistence.rs`)

**Purpose**: Persists configuration changes to MongoDB.

```rust
use mongodb::Client as MongoClient;
use mongodb::bson::{doc, Bson};
use crate::types::{LevelField, LevelValue, ConfigPath, ConfigValue, RiskLimitType};
use rust_decimal::Decimal;

pub struct ConfigPersistence {
    mongo_client: MongoClient,
    database: String,
    collection: String,
}

impl ConfigPersistence {
    pub fn new(
        mongo_client: MongoClient,
        database: String,
        collection: String,
    ) -> Self {
        Self {
            mongo_client,
            database,
            collection,
        }
    }
    
    /// Persist a level update to MongoDB
    pub async fn persist_level_update(
        &self,
        level_id: &str,
        field: &LevelField,
        value: &LevelValue,
    ) -> Result<(), PersistenceError> {
        let db = self.mongo_client.database(&self.database);
        let collection = db.collection::<mongodb::bson::Document>(&self.collection);
        
        // Build update document
        let field_path = format!("levels.$[level].{}", self.field_to_path(field));
        let update_value = self.value_to_bson(value)?;
        
        let filter = doc! { "levels.id": level_id };
        let update = doc! {
            "$set": {
                &field_path: update_value,
                "updated_at": chrono::Utc::now().timestamp() as u64,
            }
        };
        
        let options = mongodb::options::UpdateOptions::builder()
            .array_filters(vec![doc! { "level.id": level_id }])
            .build();
        
        collection.update_one(filter, update, options).await?;
        
        Ok(())
    }
    
    /// Persist a bot config update to MongoDB
    pub async fn persist_config_update(
        &self,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), PersistenceError> {
        let db = self.mongo_client.database(&self.database);
        let collection = db.collection::<mongodb::bson::Document>(&self.collection);
        
        let update_path = path.path.replace('.', ".");
        let update_value = self.value_to_bson(value)?;
        
        let filter = doc! {};  // Update the bot config document
        let update = doc! {
            "$set": {
                &update_path: update_value,
                "updated_at": chrono::Utc::now().timestamp() as u64,
            }
        };
        
        collection.update_one(filter, update, None).await?;
        
        Ok(())
    }
    
    /// Persist a risk limit update to MongoDB
    pub async fn persist_risk_limit_update(
        &self,
        limit_type: &RiskLimitType,
        value: Decimal,
    ) -> Result<(), PersistenceError> {
        let db = self.mongo_client.database(&self.database);
        let collection = db.collection::<mongodb::bson::Document>(&self.collection);
        
        let limit_path = format!("risk_limits.{}", self.limit_type_to_path(limit_type));
        
        let filter = doc! {};
        let update = doc! {
            "$set": {
                &limit_path: Bson::Decimal128(value.into()),
                "updated_at": chrono::Utc::now().timestamp() as u64,
            }
        };
        
        collection.update_one(filter, update, None).await?;
        
        Ok(())
    }
    
    fn field_to_path(&self, field: &LevelField) -> &str {
        match field {
            LevelField::Price => "price",
            LevelField::Quantity => "quantity",
            LevelField::StopLoss => "stop_loss",
            LevelField::TakeProfit => "take_profit",
            LevelField::RiskLimit => "risk_limit",
            LevelField::MaxPositionSize => "max_position_size",
            LevelField::Enabled => "enabled",
            LevelField::RepostOnFill => "repost_on_fill",
            LevelField::RepostOnCancel => "repost_on_cancel",
        }
    }
    
    fn value_to_bson(&self, value: &LevelValue) -> Result<Bson, PersistenceError> {
        match value {
            LevelValue::Decimal(d) => Ok(Bson::Decimal128((*d).into())),
            LevelValue::String(s) => Ok(Bson::String(s.clone())),
            LevelValue::Boolean(b) => Ok(Bson::Boolean(*b)),
            LevelValue::Integer(i) => Ok(Bson::Int64(*i)),
        }
    }
    
    fn limit_type_to_path(&self, limit_type: &RiskLimitType) -> &str {
        match limit_type {
            RiskLimitType::MaxPositionSize => "max_position_size",
            RiskLimitType::MaxDailyLoss => "max_daily_loss",
            RiskLimitType::MaxExposure => "max_exposure",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("MongoDB error: {0}")]
    MongoError(#[from] mongodb::error::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}
```

### 9. Config Notifier (`live_config/src/notifier.rs`)

**Purpose**: Notifies affected components of configuration changes.

```rust
use tokio::sync::mpsc;
use crate::types::{LevelField, LevelValue, ConfigPath, ConfigValue, RiskLimitType};
use rust_decimal::Decimal;

pub struct ConfigNotifier {
    level_update_tx: mpsc::UnboundedSender<LevelUpdateNotification>,
    config_update_tx: mpsc::UnboundedSender<ConfigUpdateNotification>,
    risk_update_tx: mpsc::UnboundedSender<RiskUpdateNotification>,
}

#[derive(Debug, Clone)]
pub struct LevelUpdateNotification {
    pub level_id: String,
    pub field: LevelField,
    pub value: LevelValue,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct ConfigUpdateNotification {
    pub path: ConfigPath,
    pub value: ConfigValue,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct RiskUpdateNotification {
    pub limit_type: RiskLimitType,
    pub value: Decimal,
    pub timestamp: u64,
}

impl ConfigNotifier {
    pub fn new(
        level_update_tx: mpsc::UnboundedSender<LevelUpdateNotification>,
        config_update_tx: mpsc::UnboundedSender<ConfigUpdateNotification>,
        risk_update_tx: mpsc::UnboundedSender<RiskUpdateNotification>,
    ) -> Self {
        Self {
            level_update_tx,
            config_update_tx,
            risk_update_tx,
        }
    }
    
    pub async fn notify_level_update(
        &self,
        level_id: &str,
        field: LevelField,
        value: &LevelValue,
    ) -> Result<(), NotifierError> {
        let notification = LevelUpdateNotification {
            level_id: level_id.to_string(),
            field,
            value: value.clone(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        
        self.level_update_tx.send(notification)
            .map_err(|_| NotifierError::ChannelClosed)?;
        
        Ok(())
    }
    
    pub async fn notify_config_update(
        &self,
        path: &ConfigPath,
        value: &ConfigValue,
    ) -> Result<(), NotifierError> {
        let notification = ConfigUpdateNotification {
            path: path.clone(),
            value: value.clone(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        
        self.config_update_tx.send(notification)
            .map_err(|_| NotifierError::ChannelClosed)?;
        
        Ok(())
    }
    
    pub async fn notify_risk_limit_update(
        &self,
        limit_type: RiskLimitType,
        value: Decimal,
    ) -> Result<(), NotifierError> {
        let notification = RiskUpdateNotification {
            limit_type,
            value,
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        
        self.risk_update_tx.send(notification)
            .map_err(|_| NotifierError::ChannelClosed)?;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotifierError {
    #[error("Channel closed")]
    ChannelClosed,
}
```

### 10. Audit Logger (`live_config/src/audit/logger.rs`)

**Purpose**: Logs all configuration changes for audit and debugging.

```rust
use std::fs::OpenOptions;
use std::io::Write;
use serde::Serialize;
use chrono::Utc;

#[derive(Debug, Clone, Serialize)]
pub struct AuditLogEntry {
    pub timestamp: u64,
    pub change_type: ChangeType,
    pub user: Option<String>,  // If authenticated
    pub details: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize)]
pub enum ChangeType {
    LevelUpdate,
    ConfigUpdate,
    RiskLimitUpdate,
}

pub struct AuditLogger {
    log_file: String,
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

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
```

### 11. LiveConfig Service (`live_config/src/service.rs`)

**Purpose**: Main orchestrator for the LiveConfig Service.

```rust
use std::sync::{Arc, RwLock};
use mongodb::Client as MongoClient;
use trading_engine::level::manager::LevelManager;
use trading_engine::risk::manager::RiskManager;
use core_logic::config::TradingSystemConfig;
use crate::validator::ConfigValidator;
use crate::level_updater::LevelUpdater;
use crate::config_updater::ConfigUpdater;
use crate::risk_updater::RiskUpdater;
use crate::persistence::ConfigPersistence;
use crate::notifier::ConfigNotifier;
use crate::audit::logger::AuditLogger;
use crate::types::*;

pub struct LiveConfigService {
    validator: Arc<ConfigValidator>,
    level_updater: Arc<LevelUpdater>,
    config_updater: Arc<ConfigUpdater>,
    risk_updater: Arc<RiskUpdater>,
    persistence: Arc<ConfigPersistence>,
    audit_logger: Arc<AuditLogger>,
}

impl LiveConfigService {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        risk_manager: Arc<RwLock<RiskManager>>,
        config: Arc<RwLock<TradingSystemConfig>>,
        mongo_client: MongoClient,
        notifier: Arc<ConfigNotifier>,
        audit_logger: Arc<AuditLogger>,
        database: String,
        collection: String,
    ) -> Self {
        let validator = Arc::new(ConfigValidator::new(
            level_manager.clone(),
            risk_manager.clone(),
        ));
        
        let persistence = Arc::new(ConfigPersistence::new(
            mongo_client,
            database,
            collection,
        ));
        
        let level_updater = Arc::new(LevelUpdater::new(
            level_manager.clone(),
            notifier.clone(),
        ));
        
        let config_updater = Arc::new(ConfigUpdater::new(
            config,
            notifier.clone(),
        ));
        
        let risk_updater = Arc::new(RiskUpdater::new(
            risk_manager,
            notifier,
        ));
        
        Self {
            validator,
            level_updater,
            config_updater,
            risk_updater,
            persistence,
            audit_logger,
        }
    }
    
    /// Update a level value
    pub async fn update_level_value(
        &self,
        request: LevelUpdateRequest,
    ) -> Result<UpdateResponse, LiveConfigError> {
        // 1. Validate the change
        self.validator.validate_level_update(
            &request.level_id,
            &request.field,
            &request.value,
        )?;
        
        // 2. Update in-memory level configuration
        self.level_updater.update_field(
            &request.level_id,
            request.field,
            request.value.clone(),
        ).await?;
        
        // 3. Persist to MongoDB
        self.persistence.persist_level_update(
            &request.level_id,
            &request.field,
            &request.value,
        ).await?;
        
        // 4. Log audit entry
        self.audit_logger.log(crate::audit::logger::AuditLogEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            change_type: crate::audit::logger::ChangeType::LevelUpdate,
            user: None,
            details: format!("Updated level {} field {:?} to {:?}", 
                request.level_id, request.field, request.value),
            success: true,
        })?;
        
        Ok(UpdateResponse {
            success: true,
            message: format!("Level {} updated successfully", request.level_id),
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
    }
    
    /// Update bot configuration
    pub async fn update_bot_config(
        &self,
        request: ConfigUpdateRequest,
    ) -> Result<UpdateResponse, LiveConfigError> {
        // 1. Validate the change
        self.validator.validate_config_update(&request.path, &request.value)?;
        
        // 2. Update in-memory configuration
        self.config_updater.update_config(&request.path, &request.value).await?;
        
        // 3. Persist to MongoDB
        self.persistence.persist_config_update(&request.path, &request.value).await?;
        
        // 4. Log audit entry
        self.audit_logger.log(crate::audit::logger::AuditLogEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            change_type: crate::audit::logger::ChangeType::ConfigUpdate,
            user: None,
            details: format!("Updated config path {} to {:?}", request.path.path, request.value),
            success: true,
        })?;
        
        Ok(UpdateResponse {
            success: true,
            message: format!("Config {} updated successfully", request.path.path),
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
    }
    
    /// Update risk limit
    pub async fn update_risk_limit(
        &self,
        request: RiskLimitUpdateRequest,
    ) -> Result<UpdateResponse, LiveConfigError> {
        // 1. Validate the change
        self.validator.validate_risk_limit_update(&request.limit_type, &request.value)?;
        
        // 2. Update in-memory risk limit
        self.risk_updater.update_risk_limit(request.limit_type, request.value).await?;
        
        // 3. Persist to MongoDB
        self.persistence.persist_risk_limit_update(&request.limit_type, request.value).await?;
        
        // 4. Log audit entry
        self.audit_logger.log(crate::audit::logger::AuditLogEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            change_type: crate::audit::logger::ChangeType::RiskLimitUpdate,
            user: None,
            details: format!("Updated risk limit {:?} to {}", request.limit_type, request.value),
            success: true,
        })?;
        
        Ok(UpdateResponse {
            success: true,
            message: format!("Risk limit {:?} updated successfully", request.limit_type),
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LiveConfigError {
    #[error("Validation error: {0}")]
    ValidationError(#[from] crate::validator::ValidationError),
    
    #[error("Update error: {0}")]
    UpdateError(#[from] crate::level_updater::UpdateError),
    
    #[error("Persistence error: {0}")]
    PersistenceError(#[from] crate::persistence::PersistenceError),
    
    #[error("Audit error: {0}")]
    AuditError(#[from] crate::audit::logger::AuditError),
}
```

## Integration in Runner and gRPC Server

The LiveConfig Service will be integrated into the `runner` and exposed via the gRPC server:

```rust
// runner/src/grpc/service.rs
use live_config::service::LiveConfigService;
use live_config::types::*;

impl TradingCliService for TradingCliServiceImpl {
    async fn update_level_value(
        &self,
        request: Request<UpdateLevelValueRequest>,
    ) -> Result<Response<UpdateResponse>, Status> {
        let req = request.into_inner();
        let update_request = LevelUpdateRequest {
            level_id: req.level_id,
            field: req.field.into(),
            value: req.value.into(),
        };
        
        let response = self.live_config_service
            .update_level_value(update_request)
            .await
            .map_err(|e| Status::internal(format!("Update failed: {}", e)))?;
        
        Ok(Response::new(response.into()))
    }
    
    // Similar implementations for update_bot_config and update_risk_limit
}
```

## Success Criteria

- [ ] LiveConfig Service updates level values at runtime
- [ ] LiveConfig Service updates bot configuration at runtime
- [ ] LiveConfig Service updates risk limits at runtime
- [ ] All changes are validated before applying
- [ ] All changes are persisted to MongoDB
- [ ] Affected components are notified of changes
- [ ] All changes are logged for audit
- [ ] gRPC endpoints expose LiveConfig functionality
- [ ] TUI CLI can update configuration via gRPC

