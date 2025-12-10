# Refactoring Notes: String → SmolStr and Lock Analysis

## String → SmolStr Migration

### Rules:
- ✅ Replace: Internal data structures, IDs, identifiers, field names
- ❌ Keep String: Error messages, file paths, I/O operations, external API boundaries (gRPC/HTTP)

### Files to Update:

#### live_config module:
- `types/update_request.rs`: `level_id: String` → `SmolStr`
- `types/level_field.rs`: `LevelValue::String(String)` → `SmolStr` (but keep String variant for external APIs)
- `types/config_path.rs`: `path: String` → `SmolStr` (but may need String for I/O)
- `validator.rs`: Error messages can stay String
- `service.rs`: `audit_log_file: String` → keep String (file path)

#### runner module:
- `config.rs`: Config fields that are identifiers → SmolStr
- `lib.rs`: Path strings → keep String (file paths)

#### persistence modules:
- MongoDB document models may need String for BSON compatibility
- Internal conversions should use SmolStr

## Lock Analysis: parking_lot Migration

### Current Lock Usage:

#### tokio::sync::RwLock (CANNOT replace - async):
- `live_config/src/validator.rs`
- `live_config/src/updater/level_updater.rs`
- `live_config/src/updater/risk_updater.rs`
- `runner/src/lib.rs`
- `trading_engine/src/levels/event_loop.rs`
- `trading_engine/src/strategies/executor.rs`
- `trading_engine/src/state/manager.rs`
- `trading_engine/src/state/subscriber.rs`
- `trading_engine/src/levels/indicator_subscriber.rs`
- `trading_engine/src/strategies/strategy_manager.rs`
- `trading_engine/src/strategies/strategies.rs`
- `trading_engine/src/levels/cancellation/manager.rs`
- `trading_engine/src/health.rs`
- `trading_engine/src/levels/executor.rs`
- `exchange_engine/src/state/manager.rs`
- `exchange_engine/src/state/reconciler.rs`
- `market_data_engine/src/state/manager.rs`

#### std::sync::RwLock (CAN replace with parking_lot):
- `indicator_engine/src/calculator.rs` - ✅ CAN REPLACE

#### tokio::sync::Mutex (CANNOT replace - async):
- `trading_engine/src/strategies/executor.rs` - Keep as tokio::sync::Mutex

### Recommendation:
- **Most locks are tokio::sync::RwLock** - These MUST stay (async context)
- **Only std::sync::RwLock can be replaced** - Found 1 instance in `indicator_engine/src/calculator.rs`
- **parking_lot benefit**: Faster, fairer locks, but only for sync code
- **Conclusion**: Limited benefit from parking_lot migration (only 1 file), but worth doing for consistency

### ✅ Completed:
- Replaced `std::sync::RwLock` with `parking_lot::RwLock` in `indicator_engine/src/calculator.rs`
- All `RwLock::read().unwrap()` → `RwLock::read()` (parking_lot doesn't need unwrap)
- All `RwLock::write().unwrap()` → `RwLock::write()` (parking_lot doesn't need unwrap)
- Updated return type of `cache()` method to use `parking_lot::RwLock`

## Summary of Changes

### String → SmolStr Migration:
✅ **live_config module**:
- `LevelUpdateRequest.level_id`: String → SmolStr
- `LevelValue::String`: String → SmolStr  
- `ConfigPath.path`: String → SmolStr
- `ConfigValue::String`: String → SmolStr
- Error messages remain String (for display)

✅ **runner/config module**:
- `SystemConfig.log_level`: String → SmolStr
- `MongoConfig.bot_id`: String → SmolStr
- `StrategiesConfig.strategy_type`: String → SmolStr
- File paths and connection strings remain String (I/O operations)

✅ **persistence modules**:
- `LevelPersistence.bot_id`: String → SmolStr
- `StrategyPersistence.bot_id`: String → SmolStr
- Updated constructors to accept `impl Into<SmolStr>`
- BSON conversions use `.as_str()` for SmolStr fields

### Lock Migration:
✅ **parking_lot integration**:
- Added `parking_lot = "0.12"` to workspace dependencies
- Replaced `std::sync::RwLock` with `parking_lot::RwLock` in `indicator_engine/src/calculator.rs`
- Removed `.unwrap()` calls (parking_lot doesn't return Result)
- Updated all lock operations to use parking_lot API

### Notes:
- **Error messages**: Kept as String (they're for display/logging)
- **File paths**: Kept as String (I/O operations require String)
- **Connection strings**: Kept as String (external API boundaries)
- **Identifiers**: Converted to SmolStr (IDs, names, config paths)
- **tokio::sync locks**: Cannot be replaced (async context required)

