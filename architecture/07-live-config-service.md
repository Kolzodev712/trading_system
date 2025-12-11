# Live Config Service Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Live Config Service provides runtime configuration updates without requiring system restart. It uses channel-based API for configuration updates (no HTTP dependencies) and includes validation, audit logging, and MongoDB persistence.

## Key Principle

Live Config Service enables **runtime configuration updates** with validation and audit logging, ensuring all changes are tracked and validated before application.

## Architecture

### Structure

```
src/live_config/
├── src/
│   ├── service.rs                # LiveConfigService - orchestrates service
│   ├── types.rs                  # Type definitions for updates
│   ├── validator.rs              # Validation layer
│   ├── updater/
│   │   ├── level_updater.rs      # Level configuration updates
│   │   ├── risk_updater.rs       # Risk limit updates
│   │   └── config_updater.rs     # Bot configuration updates
│   └── audit/
│       └── logger.rs              # Audit logging
```

### Components

#### 1. LiveConfigService (`service.rs`)

**Purpose**: Orchestrates the live config service.

**Key Responsibilities**:
- Coordinates validation, updates, and persistence
- Manages update request channel
- Integrates with MongoDB persistence modules
- Provides audit logging

**Methods**:
- `new(level_manager, risk_manager, mongo_client, audit_log_path)` - Create new service
- `update_level_value(request: LevelUpdateRequest) -> Result<()>` - Update level value
- `update_risk_limit(request: RiskLimitUpdateRequest) -> Result<()>` - Update risk limit
- `update_bot_config(request: ConfigUpdateRequest) -> Result<()>` - Update bot config

#### 2. Type Definitions (`types.rs`)

**Purpose**: Defines types for configuration updates.

**Types**:
- `LevelField`: Enum of level fields that can be updated
  - `Enabled`, `Price`, `Quantity`, `RiskLimit`, `MaxPosition`, etc.

- `LevelValue`: Enum of value types
  - `Boolean(bool)`, `Decimal(Decimal)`, `String(String)`, etc.

- `LevelUpdateRequest`: Request to update level value
  - `level_id: SmolStr`
  - `field: LevelField`
  - `value: LevelValue`

- `RiskLimitType`: Enum of risk limit types
  - `TotalMaxPosition`, `MaxTotalOrders`, `DailyLossLimitAllPairs`, etc.

- `RiskLimitUpdateRequest`: Request to update risk limit
  - `limit_type: RiskLimitType`
  - `value: Decimal`

- `ConfigUpdateRequest`: Request to update bot config
  - `path: String` (JSON path)
  - `value: serde_json::Value`

#### 3. Validator (`validator.rs`)

**Purpose**: Validates all configuration changes.

**Validation Rules**:
- **Level Updates**:
  - Level must exist
  - Field must be valid for level
  - Value must be valid type for field
  - Value must pass business rules (e.g., price > 0)

- **Risk Limit Updates**:
  - Limit type must be valid
  - Value must be positive (for limits)
  - Value must not exceed system limits

- **Config Updates**:
  - Path must be valid JSON path
  - Value must be valid for path
  - Update must not break system

**Methods**:
- `validate_level_update(request: &LevelUpdateRequest, level_manager: &LevelManager) -> Result<()>`
- `validate_risk_limit_update(request: &RiskLimitUpdateRequest) -> Result<()>`
- `validate_config_update(request: &ConfigUpdateRequest) -> Result<()>`

#### 4. Updaters (`updater/`)

##### LevelUpdater (`level_updater.rs`)

**Purpose**: Updates level configurations.

**Methods**:
- `update_field(level_manager: &LevelManager, request: LevelUpdateRequest) -> Result<()>`

**Update Process**:
1. Validate request
2. Get level from LevelManager
3. Update field based on LevelField enum
4. Save to MongoDB (if enabled)
5. Log audit entry

##### RiskUpdater (`risk_updater.rs`)

**Purpose**: Updates risk limits.

**Methods**:
- `update_risk_limit(risk_manager: &RiskManager, request: RiskLimitUpdateRequest) -> Result<()>`

**Update Process**:
1. Validate request
2. Get current limits from RiskManager
3. Update limit based on RiskLimitType
4. Save to MongoDB (if enabled)
5. Log audit entry

##### ConfigUpdater (`config_updater.rs`)

**Purpose**: Updates bot configuration.

**Methods**:
- `update_config(config: &mut BotConfig, request: ConfigUpdateRequest) -> Result<()>`

**Update Process**:
1. Validate request
2. Update config at JSON path
3. Validate updated config
4. Save to MongoDB (if enabled)
5. Log audit entry

#### 5. Audit Logger (`audit/logger.rs`)

**Purpose**: Logs all configuration changes for audit trail.

**Log Format**:
```
[timestamp] [type] [entity_id] [field] [old_value] -> [new_value] [user]
```

**Log Types**:
- `LEVEL_UPDATE`: Level configuration changes
- `RISK_LIMIT_UPDATE`: Risk limit changes
- `CONFIG_UPDATE`: Bot configuration changes

**Methods**:
- `new(log_path: String)` - Create new audit logger
- `log(entry: AuditEntry)` - Log audit entry

## Data Flow

```
Update Request (Channel) → LiveConfigService
    ├── Validate request
    ├── Update configuration
    ├── Persist to MongoDB (if enabled)
    └── Log audit entry
```

## Communication

### Input
- **Channel**: Update requests via `live_config_request_channel`
- **LevelManager**: For level updates
- **RiskManager**: For risk limit updates
- **BotConfig**: For bot configuration updates

### Output
- **MongoDB**: Persisted configuration changes
- **Audit Log**: File-based audit trail
- **Validation Errors**: Returned to caller

## Update Process

### Level Update Flow

```
LevelUpdateRequest → LiveConfigService
    ├── Validate (validator.rs)
    ├── Update level (level_updater.rs)
    ├── Persist to MongoDB
    └── Log audit entry
```

### Risk Limit Update Flow

```
RiskLimitUpdateRequest → LiveConfigService
    ├── Validate (validator.rs)
    ├── Update risk limit (risk_updater.rs)
    ├── Persist to MongoDB
    └── Log audit entry
```

### Config Update Flow

```
ConfigUpdateRequest → LiveConfigService
    ├── Validate (validator.rs)
    ├── Update config (config_updater.rs)
    ├── Persist to MongoDB
    └── Log audit entry
```

## Channel-Based API

**No HTTP Dependencies**: Live Config Service uses channels for communication, not HTTP.

**Channel**:
- `live_config_request_channel(capacity: usize)` - Creates channel for update requests
- Sender: `LiveConfigRequestSender`
- Receiver: `LiveConfigRequestReceiver`

**Request Types**:
- `LevelUpdate(LevelUpdateRequest)`
- `RiskLimitUpdate(RiskLimitUpdateRequest)`
- `ConfigUpdate(ConfigUpdateRequest)`

## Persistence

### MongoDB

**Purpose**: Persist configuration changes

**Collections**:
- `bot_configs` - Bot configurations
- `levels` - Level configurations (via LevelPersistence)
- Risk limits stored in `bot_configs`

**Integration**:
- Uses existing MongoDB persistence modules
- LevelPersistence for level updates
- BotConfigPersistence for config updates

### Audit Log

**Purpose**: File-based audit trail

**Location**: Configurable (default: `/tmp/audit.log`)

**Format**: Structured log entries with timestamps

## Performance Characteristics

- **Update Latency**: < 10ms (validation + update + persistence)
- **Validation**: Fast in-memory checks
- **Persistence**: Async MongoDB writes
- **Audit Logging**: Non-blocking file writes

## Dependencies

- `trading_engine`: LevelManager, RiskManager
- `runner`: BotConfig
- MongoDB client (optional)

## Testing

- Unit tests for validation
- Unit tests for updaters
- Unit tests for audit logging
- Integration tests for end-to-end updates

## Configuration

### Audit Log Configuration

```json
{
  "live_config": {
    "audit_log_path": "/var/log/trading_system/audit.log"
  }
}
```

## Error Handling

- **Validation Failures**: Returned to caller with error message
- **Update Failures**: Rollback if possible, log error
- **Persistence Failures**: Logged, update still applied in memory
- **Audit Log Failures**: Logged, update still proceeds

## Security

- **Validation**: All updates validated before application
- **Audit Trail**: All changes logged for compliance
- **Rollback**: Support for rollback on critical failures

## Usage Example

```rust
// Update level enabled status
let request = LevelUpdateRequest {
    level_id: SmolStr::new("level_1"),
    field: LevelField::Enabled,
    value: LevelValue::Boolean(false),
};
live_config_service.update_level_value(request).await?;

// Update risk limit
let request = RiskLimitUpdateRequest {
    limit_type: RiskLimitType::TotalMaxPosition,
    value: dec!(5000),
};
live_config_service.update_risk_limit(request).await?;
```

