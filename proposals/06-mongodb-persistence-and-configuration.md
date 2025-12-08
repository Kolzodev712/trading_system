# Proposal 06: MongoDB Persistence and Configuration Storage

## Overview

This proposal outlines the implementation of MongoDB persistence and configuration storage for the trading system. MongoDB is used for:

1. **Configuration Persistence**: Bot configuration, levels configuration, risk limits (always persisted)
2. **State Persistence** (Feature Flag: `persist_state`): Exchange state, trading state (disabled initially)
3. **Market Data Persistence** (Feature Flag: `persist_market_data`): Orderbooks, public trades (disabled initially)
4. **Indicator Persistence**: Indicator definitions (shared across bots/integrations)

**Key Principle**: Exchange state is source of truth. MongoDB is backup for configuration only. State persistence is optional and disabled initially.

**Note**: Runtime configuration changes (LiveConfig Service) are covered in **Proposal 06b (LiveConfig Service)**. This proposal focuses on MongoDB infrastructure and static persistence (at startup/shutdown).

## Key Requirements

1. **MongoDB Infrastructure**: Client wrapper, connection management, collection helpers
2. **Configuration Persistence**: Always persist bot configuration and levels configuration
3. **State Persistence** (Feature Flag: `persist_state`):
   - **Positions, Orders, Balances**: Logic exists, disabled initially (handled in-memory by state manager)
   - **Private Trades**: Publishing option available for future analysis, disabled initially
4. **Market Data Persistence** (Feature Flag: `persist_market_data`):
   - **Orderbooks, Public Trades**: Logic exists, disabled initially (handled in-memory by state manager)
   - Publishing option available for future analysis, disabled initially
5. **Collection Structure**: Single collection per trading bot (except indicators - see below)
6. **Exchange State as SOT**: Exchange state is source of truth, MongoDB is backup for config only
7. **Indicator Persistence**: Indicators stored in separate `indicators` collection (shared across bots/integrations)
   - **Simple Indicators**: Rust-implemented indicators (SMA, EMA, etc.)
   - **Complex/Composite Indicators**: Formula-based indicators defined in JSON config
   - **Shared Collection**: Multiple bots/integrations can use the same indicator definitions
   - **Persistence**: All indicator configurations persisted to MongoDB for reuse
8. **Lazy Initialization**: MongoDB client initialized only when persistence is enabled
9. **Batching**: Batch writes when enabled for performance

## Architecture Overview

### Persistence Flow

```
Configuration Updates:
Level Manager → MongoDB (binance_bot collection)
    ├── Save levels configuration
    ├── Save risk limits
    └── Save bot configuration

State Updates (if enabled):
ExchangeStateManager → MongoDB (exchange_states collection)
    ├── Persist balances, orders, trades
    └── Lazy initialization, batched writes

Market Data Updates (if enabled):
MarketDataStateManager → MongoDB (market_data_states collection)
    ├── Persist orderbooks, public trades
    └── Lazy initialization, batched writes

Indicator Updates:
Indicator Engine → MongoDB (indicators collection)
    ├── Save indicator definitions
    └── Shared across all bots
```

## Component Structure

```
trading_system/
├── libs/
│   └── trading_core/
│       └── src/
│           └── persistence/
│               └── mongo/
│                   ├── mod.rs
│                   ├── client.rs           # MongoDB client wrapper
│                   ├── models.rs          # Document models
│                   └── collections.rs      # Collection helpers
│
├── src/
│   ├── exchange_engine/
│   │   └── exchanges/
│   │       └── binance/
│   │           └── state/
│   │               └── persistence.rs      # Exchange state persistence
│   │
│   ├── market_data_engine/
│   │   └── exchanges/
│   │       └── binance/
│   │           └── state/
│   │               └── persistence.rs      # Market data persistence
│   │
│   ├── trading_engine/
│   │   └── levels/
│   │       └── persistence.rs             # Levels config persistence
│   │
│   └── indicator_engine/
│       └── persistence.rs                 # Indicator persistence
```

## Detailed Component Design

### 1. MongoDB Client (`libs/trading_core/src/persistence/mongo/client.rs`)

**Purpose**: MongoDB client wrapper and utilities.

```rust
pub struct MongoClient {
    client: mongodb::Client,
    database: String,
}

impl MongoClient {
    pub async fn new(connection_string: &str, database: &str) -> Result<Self, MongoError>;
    
    pub fn get_collection<T>(&self, name: &str) -> Collection<T>;
    
    pub async fn ping(&self) -> Result<(), MongoError>;
    
    pub async fn test_connection(&self) -> Result<(), MongoError>;
}
```

### 2. MongoDB Models (`libs/trading_core/src/persistence/mongo/models.rs`)

**Purpose**: MongoDB document models.

```rust
/// Trading bot configuration document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfigDocument {
    #[serde(rename = "_id")]
    pub bot_id: String,  // e.g., "binance_bot"
    
    /// Levels configuration
    pub levels: Vec<LevelConfig>,
    
    /// Risk limits
    pub risk_limits: RiskLimitsConfig,
    
    /// Trading configuration
    pub trading_config: TradingConfig,
    
    /// Last updated timestamp
    pub updated_at: u64,
}

/// Exchange state document (for persistence, disabled initially)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeStateDocument {
    #[serde(rename = "_id")]
    pub id: String,
    
    pub orders: Vec<Order>,
    pub balances: Vec<Balance>,
    pub trades: Vec<Trade>,
    pub timestamp: u64,
}

/// Market data state document (for persistence, disabled initially)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataStateDocument {
    #[serde(rename = "_id")]
    pub id: String,
    
    pub orderbooks: HashMap<String, Orderbook>,  // String key = pair
    pub timestamp: u64,
}

/// Public trade document (historical, disabled initially)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicTradeDocument {
    #[serde(rename = "_id")]
    pub id: String,
    
    pub pair: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub timestamp: u64,
    pub trade_id: String,
}

/// Candle document (historical, disabled initially)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleDocument {
    #[serde(rename = "_id")]
    pub id: String,
    
    pub pair: String,
    pub interval: String,  // "1m", "5m", "1h", etc.
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: u64,
}

/// Simple indicator document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleIndicatorDocument {
    #[serde(rename = "_id")]
    pub indicator_id: String,
    
    pub indicator_type: String,  // "simple"
    pub name: String,
    pub pair: String,
    pub period: u32,
    pub source: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Composite indicator document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeIndicatorDocument {
    #[serde(rename = "_id")]
    pub indicator_id: String,
    
    pub indicator_type: String,  // "composite"
    pub name: String,
    pub pair: String,
    pub expression: serde_json::Value,  // Formula expression
    pub dependencies: Vec<String>,  // Dependent indicator IDs
    pub created_at: u64,
    pub updated_at: u64,
}
```

### 3. Exchange State Persistence (`exchange_engine/src/exchanges/binance/state/persistence.rs`)

**Purpose**: MongoDB persistence for exchange state (disabled initially, lazy evaluation).

```rust
pub struct StatePersistence {
    mongo_client: Option<MongoClient>,  // Lazy initialization
    database: String,
    collection: String,
    enabled: Arc<AtomicBool>,
    batch_buffer: Vec<StateUpdate>,  // Batch writes when enabled
    batch_size: usize,
    flush_interval: Duration,
}

impl StatePersistence {
    /// Persist current state to MongoDB (lazy evaluation)
    pub async fn persist_state(&mut self, state: &ExchangeState) -> Result<(), StateError> {
        // Early return if disabled (no overhead)
        if !self.enabled.load(Ordering::Relaxed) {
            return Ok(());
        }
        
        // Lazy initialization of MongoDB client
        if self.mongo_client.is_none() {
            self.mongo_client = Some(MongoClient::new(...).await?);
        }
        
        // Add to batch buffer
        // Flush if buffer full or interval elapsed
        self.batch_buffer.push(...);
        if self.batch_buffer.len() >= self.batch_size {
            self.flush_batch().await?;
        }
    }
    
    /// Flush batch to MongoDB
    async fn flush_batch(&mut self) -> Result<(), StateError>;
    
    /// Persist order update
    pub async fn persist_order(&self, order: &Order) -> Result<(), StateError>;
    
    /// Persist balance update
    pub async fn persist_balance(&self, balance: &Balance) -> Result<(), StateError>;
    
    /// Persist trade
    pub async fn persist_trade(&self, trade: &Trade) -> Result<(), StateError>;
    
    /// Restore state from MongoDB (backup only)
    pub async fn restore_state(&self) -> Result<Option<ExchangeState>, StateError>;
}
```

### 4. Market Data State Persistence (`market_data_engine/src/exchanges/binance/state/persistence.rs`)

**Purpose**: MongoDB persistence for market data state and historical data (disabled initially, lazy evaluation).

```rust
pub struct StatePersistence {
    mongo_client: Option<MongoClient>,  // Lazy initialization
    database: String,
    collection: String,
    historical_collection: String,
    persistence_enabled: Arc<AtomicBool>,
    historical_enabled: Arc<AtomicBool>,
    batch_buffer: Vec<MarketDataUpdate>,
    batch_size: usize,
    flush_interval: Duration,
}

impl StatePersistence {
    /// Persist current state to MongoDB (lazy evaluation)
    pub async fn persist_state(&mut self, state: &MarketDataState) -> Result<(), StateError> {
        // Early return if disabled (no overhead)
        if !self.persistence_enabled.load(Ordering::Relaxed) {
            return Ok(());
        }
        
        // Lazy initialization
        if self.mongo_client.is_none() {
            self.mongo_client = Some(MongoClient::new(...).await?);
        }
        
        // Batch writes
    }
    
    /// Persist orderbook snapshot
    pub async fn persist_orderbook(&self, pair: &Pair, orderbook: &Orderbook) -> Result<(), StateError>;
    
    /// Store public trade (historical)
    pub async fn store_public_trade(&self, pair: &Pair, trade: &PublicTrade) -> Result<(), StateError> {
        if !self.historical_enabled.load(Ordering::Relaxed) {
            return Ok(());  // Historical storage disabled
        }
        // Store to historical collection
    }
    
    /// Store candle (historical, logic ready, unused)
    pub async fn store_candle(&self, pair: &Pair, candle: &Candle) -> Result<(), StateError> {
        if !self.historical_enabled.load(Ordering::Relaxed) {
            return Ok(());  // Historical storage disabled
        }
        // Store to historical collection
    }
    
    /// Restore state from MongoDB (backup only)
    pub async fn restore_state(&self) -> Result<Option<MarketDataState>, StateError>;
}
```

### 5. Level Manager Persistence (`trading_engine/src/levels/persistence.rs`)

**Purpose**: MongoDB persistence for levels configuration. **Always persisted** (not behind feature flag).

**Note**: Runtime updates to levels are handled by the LiveConfig Service (Proposal 06b).

```rust
pub struct LevelPersistence {
    mongo_client: MongoClient,
    bot_id: String,
}

impl LevelPersistence {
    /// Save levels configuration to MongoDB
    pub async fn save_levels(
        &self,
        levels: &[LevelConfig],
        risk_limits: &RiskLimitsConfig,
    ) -> Result<(), PersistenceError>;
    
    /// Load levels configuration from MongoDB
    pub async fn load_levels(&self) -> Result<Option<BotConfigDocument>, PersistenceError>;
    
    /// Update single level in MongoDB
    pub async fn update_level(&self, level: &LevelConfig) -> Result<(), PersistenceError>;
    
    /// Delete level from MongoDB
    pub async fn delete_level(&self, level_id: &str) -> Result<(), PersistenceError>;
    
    /// Save risk limits to MongoDB
    pub async fn save_risk_limits(&self, risk_limits: &RiskLimitsConfig) -> Result<(), PersistenceError>;
}
```

### 6. Indicator Engine Persistence (`indicator_engine/src/persistence.rs`)

**Purpose**: MongoDB persistence for indicator definitions (shared collection).

```rust
pub struct IndicatorPersistence {
    mongo_client: MongoClient,
}

impl IndicatorPersistence {
    /// Save simple indicator to MongoDB
    pub async fn save_simple_indicator(&self, indicator: &SimpleIndicatorDocument) -> Result<(), PersistenceError>;
    
    /// Save composite indicator to MongoDB
    pub async fn save_composite_indicator(&self, indicator: &CompositeIndicatorDocument) -> Result<(), PersistenceError>;
    
    /// Load all indicators from MongoDB
    pub async fn load_all_indicators(&self) -> Result<Vec<IndicatorConfig>, PersistenceError>;
    
    /// Load indicator by ID
    pub async fn load_indicator(&self, indicator_id: &str) -> Result<Option<IndicatorConfig>, PersistenceError>;
    
    /// Update indicator in MongoDB
    pub async fn update_indicator(&self, indicator: &IndicatorConfig) -> Result<(), PersistenceError>;
    
    /// Delete indicator from MongoDB
    pub async fn delete_indicator(&self, indicator_id: &str) -> Result<(), PersistenceError>;
}
```

## MongoDB Collection Structure

### Single Collection Per Bot

**Collection**: `binance_bot` (or `kraken_bot`, etc.)

**Document Structure**:
```json
{
  "_id": "binance_bot",
  "levels": [
    {
      "id": "buy_level_1",
      "name": "Buy at SMA-2%",
      "symbol": { "base": "BTC", "quote": "USDT" },
      "side": "Buy",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.001" },
      "price": { "type": "Indicator", "indicator_name": "sma_20", "offset_type": "PercentBelow", "offset_value": "2.0" },
      "risk": { "max_position": "0.01", "max_orders": 2 },
      "lifecycle": { "auto_repost": true, "repost_delay_secs": 5 },
      "enabled": true,
      "group": "main_strategy"
    }
  ],
  "risk_limits": {
    "total_max_position": "0.1",
    "max_total_orders": 20,
    "daily_loss_limit_all_pairs": "500.0"
  },
  "trading_config": {
    "configured_pairs": ["BTCUSDT", "ETHUSDT"]
  },
  "updated_at": 1234567890
}
```

### Indicators Collection (Shared)

**Collection**: `indicators` (shared across all bots/integrations)

**Purpose**: Store indicator definitions that can be reused across multiple trading bots or integrations.

**Simple Indicator Document Structure**:
```json
{
  "_id": "sma_20_btcusdt",
  "indicator_id": "sma_20_btcusdt",
  "indicator_type": "simple",
  "name": "SMA",
  "pair": "BTCUSDT",
  "period": 20,
  "source": "Close",
  "created_at": 1234567890,
  "updated_at": 1234567890
}
```

**Composite Indicator Document Structure**:
```json
{
  "_id": "golden_cross_btcusdt",
  "indicator_id": "golden_cross_btcusdt",
  "indicator_type": "composite",
  "name": "Golden Cross",
  "pair": "BTCUSDT",
  "expression": {
    "subtract": [
      {"simple": "ema_50_btcusdt"},
      {"simple": "ema_200_btcusdt"}
    ]
  },
  "dependencies": ["ema_50_btcusdt", "ema_200_btcusdt"],
  "created_at": 1234567890,
  "updated_at": 1234567890
}
```

**Key Points**:
- Indicators are stored in a separate collection from bot configurations
- Multiple bots can reference the same indicator by ID
- Simple indicators are Rust-implemented (SMA, EMA, etc.)
- Composite indicators are formula-based and depend on simple indicators
- Circular dependencies are prevented (complex indicators depend on simple ones, not vice versa)

### State Persistence Collections (Optional, Feature Flag)

**Collections** (only created if persistence enabled):
- `exchange_states`: Exchange state snapshots
- `market_data_states`: Market data state snapshots
- `historical_trades`: Historical public trades
- `historical_candles`: Historical candle data

## Integration Points

### 1. Exchange Engine Integration

```rust
// In exchange engine (Binance client)
let state_manager = ExchangeStateManager::new(
    exchange_update_tx,  // Broadcast channel sender
    trading_client,
    // ... other params
);

// Persistence is optional (lazy initialization)
// State manager handles persistence internally if enabled
```

### 2. Market Data Engine Integration

```rust
// In market data engine (Binance client)
let state_manager = MarketDataStateManager::new(
    market_data_update_tx,  // Broadcast channel sender
    // ... other params
);

// Persistence is optional (lazy initialization)
// State manager handles persistence internally if enabled
```

### 3. Level Manager Integration

```rust
// In level manager
let persistence = LevelPersistence::new(mongo_client, "binance_bot".to_string());

// On level configuration change (always persisted)
persistence.save_levels(&levels, &risk_limits).await?;

// On startup - restore from MongoDB (backup)
if let Some(config) = persistence.load_levels().await? {
    // Restore levels if exchange data unavailable
}
```

### 4. Indicator Engine Persistence

```rust
// In indicator engine
let indicator_persistence = IndicatorPersistence::new(mongo_client);

// On startup - load indicators from MongoDB
let indicators = indicator_persistence.load_all_indicators().await?;
for indicator_config in indicators {
    indicator_service.register_indicator(indicator_config)?;
}

// When new indicator is created (from UI or TUI)
indicator_persistence.save_simple_indicator(&indicator_config).await?;

// When indicator is updated
indicator_persistence.update_indicator(&indicator_config).await?;
```

## Configuration

### MongoDB Configuration

```json
{
  "mongo": {
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system",
    "collections": {
      "bot_config": "bot_configs",
      "indicators": "indicators",
      "exchange_state": "exchange_states",
      "market_data_state": "market_data_states",
      "historical_trades": "historical_trades",
      "historical_candles": "historical_candles"
    }
  },
  "persistence": {
    "enabled": false,  // Disabled initially
    "historical_enabled": false  // Disabled initially
  }
}
```

## Error Handling

1. **MongoDB Connection Loss**
   - Log error
   - Continue with in-memory state only
   - Retry connection with exponential backoff
   - Configuration persistence: Retry until successful (critical)

2. **Persistence Failures**
   - Log error
   - Continue processing (state persistence is optional)
   - Retry persistence
   - Configuration persistence: Must succeed (critical)

3. **State Restoration Failures**
   - Log error
   - Continue with empty state
   - Fetch from exchange instead (for state)
   - Use default config (for configuration)

## Testing Strategy

1. **Unit Tests**
   - MongoDB client wrapper
   - Document models
   - Persistence logic (with mock MongoDB)

2. **Integration Tests**
   - End-to-end: State Manager → MongoDB
   - Configuration persistence
   - State restoration
   - Indicator persistence

3. **Mock Tests**
   - Mock MongoDB client
   - Test error scenarios
   - Test lazy initialization

## Success Criteria

- [ ] MongoDB client wrapper implemented
- [ ] Document models defined
- [ ] Configuration persistence always works (levels, risk limits, bot config)
- [ ] State persistence capability exists (disabled initially)
- [ ] Market data persistence capability exists (disabled initially)
- [ ] Indicator persistence implemented (shared collection)
- [ ] Single collection per bot structure
- [ ] Lazy initialization works (no overhead when disabled)
- [ ] Batching works when enabled
- [ ] State restoration from MongoDB (backup only)

## Performance Optimizations

### 1. Lazy Initialization
- Initialize MongoDB client only when persistence is enabled
- Early return if disabled (no overhead)
- Connection pooling when enabled

### 2. Batching
- Batch multiple updates before writing
- Configurable batch size and flush interval
- Reduce write operations

### 3. Connection Pooling
- Reuse MongoDB connections
- Efficient resource usage

## Notes

- **Configuration persistence is always enabled** - levels, risk limits, bot config
- **State persistence is optional** - disabled initially via feature flag
- **MongoDB is backup for config only** - exchange state is source of truth
- **Lazy initialization** - no overhead when persistence disabled
- **Batching** - efficient writes when enabled
- **Single collection per bot** - all config in one document
- **Indicators shared collection** - reusable across bots
- **Prerequisite for Proposal 06b** - LiveConfig Service uses MongoDB for persistence

