# Proposal 05: Data Handling and Persistence

## Overview

This proposal outlines the implementation of state management and data persistence across the trading system. It introduces `ExchangeStateManager` and `MarketDataStateManager` to handle in-memory state, publish data to trading system and UI, and provide MongoDB persistence capabilities (initially disabled via feature flags). MongoDB is used for trading configuration persistence (levels, risk limits, bot configuration) with a single collection per trading bot.

**Note**: Runtime configuration changes (LiveConfig Service) are covered in **Proposal 06 (LiveConfig Service)**. This proposal focuses on state management and persistence infrastructure.

## Key Requirements

1. **State Managers**: Exchange and Market Data state managers for in-memory state management
2. **Publishing**: Publish all updates to trading system and UI via channels (after Proposal 03)
3. **MongoDB Persistence**: Capability to persist state data (disabled initially via feature flag)
4. **Configuration Persistence**: MongoDB stores bot configuration and levels configuration (managed by level manager)
   - **Bot Config**: Entire bot configuration (key elements for startup, excluding API keys) + levels for each bot and their specific values → **Always persisted**
   - **Levels**: Level configurations and their values → **Always persisted**
5. **State Persistence** (Feature Flag: `persist_state`):
   - **Positions, Orders, Balances**: Logic exists, disabled initially (handled in-memory by state manager)
   - **Private Trades**: Publishing option available for future analysis, disabled initially
7. **Market Data Persistence** (Feature Flag: `persist_market_data`):
   - **Orderbooks, Public Trades**: Logic exists, disabled initially (handled in-memory by state manager)
   - Publishing option available for future analysis, disabled initially
8. **Collection Structure**: Single collection per trading bot (except indicators - see below)
9. **Exchange State as SOT**: Exchange state is source of truth, MongoDB is backup for config only
10. **Indicator Persistence**: Indicators stored in separate `indicators` collection (shared across bots/integrations)
    - **Simple Indicators**: Rust-implemented indicators (SMA, EMA, etc.)
    - **Complex/Composite Indicators**: Formula-based indicators defined in JSON config
    - **Shared Collection**: Multiple bots/integrations can use the same indicator definitions
    - **Persistence**: All indicator configurations persisted to MongoDB for reuse

## Architecture Overview

### State Management Flow

```
Exchange Engine (Binance, etc.)
    ↓
ExchangeStateManager
    ├── In-Memory State (orders, balances, positions, private trades)
    ├── Publish to Trading System (Channel: in-process, ultra-low latency)
    ├── Publish to Monitoring (Channel: in-process OR gRPC: external)
    └── Persist to MongoDB (disabled via trigger, lazy evaluation)

Market Data Engine (Binance, etc.)
    ↓
MarketDataStateManager
    ├── In-Memory State (orderbooks, public trades)
    ├── Publish to Trading System (Channel: in-process, ultra-low latency)
    ├── Publish to Monitoring (Channel: in-process OR gRPC: external)
    └── Persist to MongoDB (disabled via trigger, lazy evaluation)

Level Manager
    ├── In-Memory Levels Config
    └── Persist to MongoDB (binance_bot collection)
```

### Data Flow

```
1. Exchange Updates:
   Exchange (WS/REST) → ExchangeStateManager → [Trading System, UI, MongoDB*]

2. Market Data Updates:
   Exchange (WS) → MarketDataStateManager → [Trading System, UI, MongoDB*]

3. Configuration Updates:
   Level Manager → MongoDB (binance_bot collection)

* MongoDB persistence disabled initially via trigger
```

## Component Structure

```
trading_system/
├── src/
│   ├── libs/
│   │   ├── exchange/
│   │   │   ├── core/
│   │   │   │   └── state/              # NEW: Core state abstractions
│   │   │   │       ├── mod.rs
│   │   │   │       ├── traits.rs       # StateManager trait
│   │   │   │       └── types.rs        # State types
│   │   │   └── binance/
│   │   │       └── state/              # NEW: Binance state implementation
│   │   │           ├── mod.rs
│   │   │           ├── manager.rs      # ExchangeStateManager
│   │   │           ├── state.rs        # ExchangeState
│   │   │           ├── publisher.rs    # Publishes to trading system/UI
│   │   │           └── persistence.rs # MongoDB persistence (disabled)
│   │   │
│   │   ├── market_data/
│   │   │   ├── core/
│   │   │   │   └── state/              # NEW: Core state abstractions
│   │   │   │       ├── mod.rs
│   │   │   │       ├── traits.rs       # StateManager trait
│   │   │   │       └── types.rs        # State types
│   │   │   └── binance/
│   │   │       └── state/              # NEW: Binance state implementation
│   │   │           ├── mod.rs
│   │   │           ├── manager.rs      # MarketDataStateManager
│   │   │           ├── state.rs        # MarketDataState
│   │   │           ├── publisher.rs    # Publishes to trading system/UI
│   │   │           └── persistence.rs # MongoDB persistence (disabled)
│   │   │
│   │   └── shared/
│   │       └── mongo/                  # NEW: MongoDB utilities
│   │           ├── mod.rs
│   │           ├── client.rs           # MongoDB client wrapper
│   │           ├── models.rs           # Document models
│   │           └── collections.rs     # Collection helpers
│   │
│   ├── trading_engine/
│   │   └── level/
│   │       └── persistence.rs         # NEW: Levels config MongoDB persistence
│   │
│   └── ... (existing)
│
│   Note: LiveConfig Service is implemented in Proposal 06
```

## Detailed Component Design

### 1. Exchange State Manager (`exchange/binance/state/manager.rs`)

**Purpose**: Manages in-memory exchange state and publishes updates.

```rust
pub struct ExchangeStateManager {
    state: Arc<RwLock<ExchangeState>>,
    publisher: StatePublisher,
    persistence: StatePersistence,
    persistence_enabled: Arc<AtomicBool>,  // Trigger for persistence
}

impl ExchangeStateManager {
    /// Update order in state and publish
    pub async fn update_order(&self, order: Order) -> Result<(), StateError> {
        // 1. Update in-memory state
        // 2. Publish to trading system
        // 3. Publish to UI
        // 4. Persist to MongoDB (if enabled)
    }
    
    /// Update balance in state and publish
    pub async fn update_balance(&self, balance: Balance) -> Result<(), StateError>;
    
    /// Update position in state and publish
    pub async fn update_position(&self, pair: Pair, position: Position) -> Result<(), StateError>;
    
    /// Add private trade and publish
    pub async fn add_trade(&self, trade: Trade) -> Result<(), StateError>;
    
    /// Get current state
    pub fn get_state(&self) -> Arc<RwLock<ExchangeState>>;
    
    /// Enable/disable MongoDB persistence
    pub fn set_persistence_enabled(&self, enabled: bool);
}
```

### 2. Exchange State (`exchange/binance/state/state.rs`)

**Purpose**: In-memory state structure for exchange data.

```rust
pub struct ExchangeState {
    /// Orders by order ID
    orders: HashMap<OrderId, Order>,
    
    /// Orders by pair
    orders_by_pair: HashMap<Pair, Vec<OrderId>>,
    
    /// Balances by currency symbol
    balances: HashMap<String, Balance>,
    
    /// Positions by pair
    positions: HashMap<Pair, Position>,
    
    /// Recent private trades (last N)
    private_trades: Vec<Trade>,
    
    /// Last update timestamp
    last_update: u64,
}

impl ExchangeState {
    pub fn new() -> Self;
    
    pub fn update_order(&mut self, order: Order);
    
    pub fn remove_order(&mut self, order_id: &OrderId);
    
    pub fn update_balance(&mut self, balance: Balance);
    
    pub fn update_position(&mut self, pair: Pair, position: Position);
    
    pub fn add_trade(&mut self, trade: Trade);
    
    pub fn get_orders(&self) -> Vec<&Order>;
    
    pub fn get_orders_for_pair(&self, pair: &Pair) -> Vec<&Order>;
    
    pub fn get_balances(&self) -> Vec<&Balance>;
    
    pub fn get_positions(&self) -> Vec<&Position>;
    
    pub fn get_trades(&self) -> Vec<&Trade>;
}
```

### 3. Exchange State Publisher (`exchange/binance/state/grpc/publisher.rs`)

**Purpose**: Publishes state updates via gRPC streaming to trading system and monitoring.

```rust
pub struct GrpcStatePublisher {
    trading_system_client: TradingSystemClient,
    monitoring_client: MonitoringClient,
    connection_pool: ConnectionPool,
    batch_buffer: Vec<StateUpdate>,
    batch_size: usize,
    flush_interval: Duration,
}

impl GrpcStatePublisher {
    /// Start gRPC streaming
    pub async fn start_streaming(&mut self) -> Result<(), StateError>;
    
    /// Publish order update (buffered, batched)
    pub async fn publish_order(&mut self, order: &Order) -> Result<(), StateError>;
    
    /// Publish balance update (buffered, batched)
    pub async fn publish_balance(&mut self, balance: &Balance) -> Result<(), StateError>;
    
    /// Publish position update (buffered, batched)
    pub async fn publish_position(&mut self, pair: &Pair, position: &Position) -> Result<(), StateError>;
    
    /// Publish trade update (buffered, batched)
    pub async fn publish_trade(&mut self, trade: &Trade) -> Result<(), StateError>;
    
    /// Flush batch buffer
    async fn flush_batch(&mut self) -> Result<(), StateError>;
}
```

**Performance Optimizations**:
- **Batching**: Batch updates (e.g., every 50ms or 20 updates)
- **Connection pooling**: Reuse gRPC connections
- **Binary protocol**: Efficient protobuf serialization
- **Streaming**: Real-time updates with backpressure

### 3b. Redis Publisher (`exchange/binance/state/redis_publisher.rs`)

**Purpose**: Publishes occasional snapshots to UI via Redis.

```rust
pub struct RedisStatePublisher {
    redis_client: RedisClient,
    snapshot_interval: Duration,
    last_snapshot: Instant,
}

impl RedisStatePublisher {
    /// Publish full state snapshot (occasional, debounced)
    pub async fn publish_snapshot(&mut self, state: &ExchangeState) -> Result<(), StateError> {
        // Only publish if interval elapsed (e.g., every 5 seconds)
        if self.last_snapshot.elapsed() >= self.snapshot_interval {
            // Publish to Redis
            self.last_snapshot = Instant::now();
        }
    }
}
```
```

### 4. Exchange State Persistence (`exchange/binance/state/persistence.rs`)

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
    
    /// Persist position update
    pub async fn persist_position(&self, pair: &Pair, position: &Position) -> Result<(), StateError>;
    
    /// Persist trade
    pub async fn persist_trade(&self, trade: &Trade) -> Result<(), StateError>;
    
    /// Restore state from MongoDB (backup only)
    pub async fn restore_state(&self) -> Result<Option<ExchangeState>, StateError>;
}
```

### 5. Market Data State Manager (`market_data/binance/state/manager.rs`)

**Purpose**: Manages in-memory market data state and publishes updates.

```rust
pub struct MarketDataStateManager {
    state: Arc<RwLock<MarketDataState>>,
    publisher: StatePublisher,
    persistence: StatePersistence,
    persistence_enabled: Arc<AtomicBool>,  // Trigger for persistence
    historical_enabled: Arc<AtomicBool>,    // Trigger for historical data
}

impl MarketDataStateManager {
    /// Update orderbook and publish
    pub async fn update_orderbook(&self, pair: Pair, orderbook: Orderbook) -> Result<(), StateError> {
        // 1. Update in-memory state
        // 2. Publish to trading system
        // 3. Publish to UI
        // 4. Persist to MongoDB (if enabled)
    }
    
    /// Add public trade and publish
    pub async fn add_public_trade(&self, pair: Pair, trade: Trade) -> Result<(), StateError> {
        // 1. Add to in-memory state
        // 2. Publish to trading system
        // 3. Publish to UI
        // 4. Store historical (if enabled)
    }
    
    /// Generate candle from trades (logic ready, unused)
    pub async fn generate_candle(&self, pair: Pair, interval: CandleInterval) -> Result<Candle, StateError>;
    
    /// Get current state
    pub fn get_state(&self) -> Arc<RwLock<MarketDataState>>;
    
    /// Enable/disable MongoDB persistence
    pub fn set_persistence_enabled(&self, enabled: bool);
    
    /// Enable/disable historical data storage
    pub fn set_historical_enabled(&self, enabled: bool);
}
```

### 6. Market Data State (`market_data/binance/state/state.rs`)

**Purpose**: In-memory state structure for market data.

```rust
pub struct MarketDataState {
    /// Orderbooks by pair
    orderbooks: HashMap<Pair, Orderbook>,
    
    /// Latest orderbook timestamp by pair
    orderbook_timestamps: HashMap<Pair, u64>,
    
    /// Recent public trades by pair (last N)
    public_trades: HashMap<Pair, Vec<Trade>>,
    
    /// Candle data by pair and interval (logic ready, unused)
    candles: HashMap<(Pair, CandleInterval), Vec<Candle>>,
    
    /// Last update timestamp
    last_update: u64,
}

impl MarketDataState {
    pub fn new() -> Self;
    
    pub fn update_orderbook(&mut self, pair: Pair, orderbook: Orderbook);
    
    pub fn add_public_trade(&mut self, pair: Pair, trade: Trade);
    
    pub fn get_orderbook(&self, pair: &Pair) -> Option<&Orderbook>;
    
    pub fn get_public_trades(&self, pair: &Pair) -> Vec<&Trade>;
    
    /// Generate candle (logic ready, unused)
    pub fn generate_candle(&mut self, pair: Pair, interval: CandleInterval) -> Option<Candle>;
}
```

### 7. Market Data State Publisher (`market_data/binance/state/grpc/publisher.rs`)

**Purpose**: Publishes market data updates via gRPC streaming.

```rust
pub struct GrpcStatePublisher {
    trading_system_client: TradingSystemClient,
    monitoring_client: MonitoringClient,
    connection_pool: ConnectionPool,
    batch_buffer: Vec<MarketDataUpdate>,
    batch_size: usize,
    flush_interval: Duration,
}

impl GrpcStatePublisher {
    /// Start gRPC streaming
    pub async fn start_streaming(&mut self) -> Result<(), StateError>;
    
    /// Publish orderbook update (buffered, batched)
    pub async fn publish_orderbook(&mut self, pair: &Pair, orderbook: &Orderbook) -> Result<(), StateError>;
    
    /// Publish public trade update (buffered, batched)
    pub async fn publish_trade(&mut self, pair: &Pair, trade: &Trade) -> Result<(), StateError>;
    
    /// Flush batch buffer
    async fn flush_batch(&mut self) -> Result<(), StateError>;
}
```

### 7b. Redis Publisher (`market_data/binance/state/redis_publisher.rs`)

**Purpose**: Publishes occasional snapshots to UI via Redis.

```rust
pub struct RedisStatePublisher {
    redis_client: RedisClient,
    snapshot_interval: Duration,
    last_snapshot: Instant,
}

impl RedisStatePublisher {
    /// Publish full state snapshot (occasional, debounced)
    pub async fn publish_snapshot(&mut self, state: &MarketDataState) -> Result<(), StateError>;
}
```

### 8. Market Data State Persistence (`market_data/binance/state/persistence.rs`)

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
    pub async fn store_public_trade(&self, pair: &Pair, trade: &Trade) -> Result<(), StateError> {
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

### 9. MongoDB Client (`shared/mongo/client.rs`)

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
}
```

### 10. MongoDB Models (`shared/mongo/models.rs`)

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
    pub positions: Vec<Position>,
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

### 11. Live Config Service (`live_config/service.rs`)

**Purpose**: Enables runtime configuration changes (e.g., modifying level values while the bot is running).

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
```

### 11. Level Manager Persistence (`trading_engine/level/persistence.rs`)

**Purpose**: MongoDB persistence for levels configuration. Note: Runtime updates to levels are handled by the LiveConfig Service (Proposal 06).

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

## Integration Points

### 1. Exchange Engine Integration

```rust
// In exchange engine (Binance client)
let state_manager = ExchangeStateManager::new(
    redis_trading_client,
    redis_ui_client,
    mongo_client,
    "binance_bot".to_string(),
);

// On order update from WebSocket
state_manager.update_order(order).await?;

// On balance update
state_manager.update_balance(balance).await?;
```

### 2. Market Data Engine Integration

```rust
// In market data engine (Binance client)
let state_manager = MarketDataStateManager::new(
    redis_trading_client,
    redis_ui_client,
    mongo_client,
    "binance_bot".to_string(),
);

// On orderbook update
state_manager.update_orderbook(pair, orderbook).await?;

// On public trade
state_manager.add_public_trade(pair, trade).await?;
```

### 3. Level Manager Integration

```rust
// In level manager
let persistence = LevelPersistence::new(mongo_client, "binance_bot".to_string());

// On level configuration change
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
indicator_persistence.save_indicator(&indicator_config).await?;

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

## Publishing Flow

### Exchange State Updates

```
Order Update (from Exchange)
    ↓
ExchangeStateManager.update_order()
    ├── Update in-memory state
    ├── Publish to gRPC Stream → Trading System (high-frequency, batched)
    ├── Publish to gRPC Stream → Monitoring Service (high-frequency, batched)
    ├── Publish to Redis (monitoring:ui) → UI (occasional snapshots, debounced)
    └── Persist to MongoDB (if enabled, lazy, batched) → MongoDB
```

**Performance Optimizations**:
- **gRPC batching**: Batch updates (e.g., every 50ms or 20 updates)
- **Redis debouncing**: Publish snapshots only every N seconds
- **MongoDB lazy init**: Initialize client only when enabled
- **MongoDB batching**: Batch writes when enabled

### Market Data Updates

```
Orderbook Update (from Exchange)
    ↓
MarketDataStateManager.update_orderbook()
    ├── Update in-memory state
    ├── Publish to gRPC Stream → Trading System (high-frequency, batched)
    ├── Publish to gRPC Stream → Monitoring Service (high-frequency, batched)
    ├── Publish to Redis (monitoring:ui) → UI (occasional snapshots, debounced)
    └── Persist to MongoDB (if enabled, lazy, batched) → MongoDB
```

**Performance Optimizations**:
- **gRPC batching**: Batch orderbook updates (high-frequency)
- **Redis debouncing**: Publish snapshots only every N seconds
- **MongoDB lazy init**: Initialize client only when enabled
- **MongoDB batching**: Batch writes when enabled

## Error Handling

1. **MongoDB Connection Loss**
   - Log error
   - Continue with in-memory state only
   - Retry connection with exponential backoff

2. **Publishing Failures**
   - Log error
   - Continue processing
   - Retry publishing

3. **State Restoration Failures**
   - Log error
   - Continue with empty state
   - Fetch from exchange instead

## Testing Strategy

1. **Unit Tests**
   - State management
   - Publishing logic
   - Persistence logic (with mock MongoDB)

2. **Integration Tests**
   - End-to-end: Exchange → State Manager → Redis → MongoDB
   - State restoration
   - Configuration persistence

3. **Mock Tests**
   - Mock Redis client
   - Mock MongoDB client
   - Test error scenarios

## Success Criteria

- [ ] ExchangeStateManager manages in-memory state
- [ ] MarketDataStateManager manages in-memory state
- [ ] All updates published to trading system and UI
- [ ] MongoDB persistence capability exists (disabled initially)
- [ ] Levels configuration persisted to MongoDB
- [ ] Historical data logic ready (disabled initially)
- [ ] Single collection per bot structure
- [ ] State restoration from MongoDB (backup only)
- [ ] Publishing on every update

## Future Enhancements (Out of Scope)

- MongoDB performance optimization
- Historical data retention policies
- Advanced querying and analytics
- Data archival strategies
- Backup and recovery procedures

## Performance Optimizations

### 1. gRPC Streaming for High-Frequency Updates
- **Binary protocol**: 3-10x smaller than JSON
- **HTTP/2 multiplexing**: Lower latency
- **Batching**: Batch updates (e.g., every 50ms or 20 updates)
- **Connection pooling**: Reuse gRPC connections
- **Backpressure**: Built-in flow control

### 2. Redis Pub/Sub for Broadcast Scenarios
- **Debouncing**: Publish snapshots only every N seconds (e.g., 5 seconds)
- **Batching**: Batch multiple updates into single message
- **Incremental updates**: Publish only changed data
- **Connection pooling**: Reuse Redis connections

### 3. MongoDB Persistence Optimization
- **Lazy initialization**: Initialize client only when enabled
- **Early return**: Skip persistence logic if disabled (no overhead)
- **Batch writes**: Batch multiple updates before writing
- **Conditional compilation**: Feature flags for compile-time optimization

### 4. State Management Efficiency
- **In-memory state**: Fast O(1) lookups with HashMap
- **Arc<RwLock<>>**: Efficient concurrent access
- **Minimal allocations**: Reuse buffers where possible

## Notes

- **State managers handle in-memory state** - orders, balances, orderbooks, trades
- **MongoDB stores configuration only** - levels, risk limits (managed by level manager)
- **Persistence capability exists but disabled** - can be enabled via trigger, lazy evaluation
- **Historical data logic ready but unused** - public trades, candles
- **gRPC for high-frequency** - trading system and monitoring (streaming, batched)
- **Redis for broadcasts** - UI snapshots (occasional, debounced)
- **Exchange state is SOT** - MongoDB is backup for config only
- **Single collection per bot** - all config in one document
- **State restoration** - from MongoDB only if exchange data unavailable
- **Performance optimized** - batching, debouncing, lazy evaluation, connection pooling

