# Proposal 02: Level-Based Trading System

## Overview

This proposal outlines the implementation of a level-based trading system where trading commands (levels) are configured to open bid or ask orders. Levels use indicators for dynamic order requirements and are exchange-agnostic. The system includes robust lifecycle management, risk checks, and cancellation logic.

## Current State Analysis

### Current Limitations

1. **Single Strategy**: Only `BuyLowSellHigh` strategy exists
2. **Hard-coded Logic**: Strategy logic is embedded in code, not configurable
3. **No Level Management**: No concept of reusable trading levels
4. **Limited Risk Checks**: Basic risk checks, not comprehensive
5. **No Indicator Integration**: Strategies calculate indicators internally
6. **No Dynamic Configuration**: Changes require code modifications

### Current Flow

```
Orderbook → Strategy → Order Request → Exchange
```

### Target Flow

```
Config → Level Manager → Level Executor → Risk Checks → Exchange
         ↑                                    ↓
    Indicators (Channel/gRPC) ← Market Data ← Indicator Service
```

## Architecture Overview

### Core Concepts

**Level**: A trading command that opens a bid (buy) or ask (sell) order. A level is configured with:
- Static values (price, quantity, order type, etc.)
- Dynamic values derived from indicators
- Risk parameters
- Lifecycle rules

**Level Lifecycle**:
1. **Created**: Level configuration loaded from config/UI
2. **Posted**: Order placed on exchange
3. **Active**: Order pending on exchange
4. **Filled**: Order executed → Repost level (unless condition prevents)
5. **Cancelled**: Order cancelled → Repost level (unless condition prevents)
6. **Paused**: Level disabled (no reposting)
7. **Removed**: Level deleted from system

### Component Structure

```
trading_system/
├── src/
│   ├── libs/
│   │   └── ... (existing libs from Proposal 01)
│   │
│   ├── trading_engine/
│   │   ├── level/
│   │   │   ├── mod.rs
│   │   │   ├── level.rs              # Level struct and types
│   │   │   ├── config.rs             # Level configuration types
│   │   │   ├── manager.rs            # Level lifecycle management
│   │   │   ├── executor.rs           # Level execution logic
│   │   │   ├── resolver.rs           # Resolves indicator values for levels
│   │   │   └── indicator_subscriber.rs # Channel/gRPC subscriber for indicators
│   │   │
│   │   ├── risk/
│   │   │   ├── mod.rs
│   │   │   ├── manager.rs            # RiskManager - orchestrates checks
│   │   │   ├── limits.rs              # Risk limit definitions
│   │   │   ├── position_checks.rs     # Position size checks
│   │   │   ├── balance_checks.rs      # Balance validation
│   │   │   ├── pnl_checks.rs         # P&L validation
│   │   │   └── daily_limits.rs       # Daily loss limits
│   │   │
│   │   ├── cancellation/
│   │   │   ├── mod.rs
│   │   │   ├── manager.rs             # CancellationManager
│   │   │   ├── conditions.rs         # Cancellation conditions
│   │   │   └── logic.rs               # Cancellation logic
│   │   │
│   │   ├── event_loop.rs              # Refined event loop with risk/cancellation
│   │   └── ... (existing strategy code - can coexist)
│   │
│   └── cli/
│       ├── main.rs
│       ├── commands/
│       │   ├── mod.rs
│       │   ├── level.rs               # Level management commands
│       │   ├── config.rs              # Config management commands
│       │   └── status.rs              # Status/view commands
│       └── config_loader.rs            # Loads config from JSON
```

## Detailed Component Design

### 1. Level Configuration (`trading_engine/level/config.rs`)

**Purpose**: Exchange-agnostic level configuration types.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelConfig {
    /// Unique identifier for this level
    pub id: String,
    
    /// Level name/description
    pub name: Option<String>,
    
    /// Trading pair
    pub symbol: Pair,
    
    /// Side: Buy (bid) or Sell (ask)
    pub side: Side,
    
    /// Order type configuration
    pub order_type: OrderTypeConfig,
    
    /// Quantity configuration (static or indicator-based)
    pub quantity: QuantityConfig,
    
    /// Price configuration (static or indicator-based)
    pub price: PriceConfig,
    
    /// Risk parameters for this level
    pub risk: LevelRiskConfig,
    
    /// Lifecycle rules
    pub lifecycle: LifecycleConfig,
    
    /// Whether level is enabled
    pub enabled: bool,
    
    /// Level group (for management)
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderTypeConfig {
    Market,
    Limit,
    PostOnly,
    IoC,  // Immediate or Cancel
    FoK,  // Fill or Kill
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuantityConfig {
    /// Fixed quantity
    Fixed(Decimal),
    
    /// Quantity based on indicator value
    Indicator {
        indicator_name: String,
        multiplier: Option<Decimal>,  // Multiply indicator value
        offset: Option<Decimal>,      // Add/subtract from indicator
    },
    
    /// Percentage of available balance
    BalancePercent(Decimal),
    
    /// Fixed notional value (price * quantity = notional)
    Notional(Decimal),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceConfig {
    /// Fixed price
    Fixed(Decimal),
    
    /// Price based on indicator with offset
    Indicator {
        indicator_name: String,
        offset_type: PriceOffsetType,
        offset_value: Decimal,
    },
    
    /// Best bid/ask with offset
    BestBid { offset: Decimal },
    BestAsk { offset: Decimal },
    
    /// Mid price with offset
    MidPrice { offset_percent: Decimal },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceOffsetType {
    Absolute,      // Add/subtract absolute value
    Percent,       // Add/subtract percentage
    PercentBelow,  // X% below indicator
    PercentAbove,  // X% above indicator
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelRiskConfig {
    /// Maximum position size for this level
    pub max_position: Option<Decimal>,
    
    /// Maximum number of orders for this level
    pub max_orders: Option<usize>,
    
    /// Daily loss limit for this level
    pub daily_loss_limit: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    /// Auto-repost after fill (default: true)
    pub auto_repost: bool,
    
    /// Auto-repost after cancellation (default: true)
    pub auto_repost_on_cancel: bool,
    
    /// Maximum reposts before stopping
    pub max_reposts: Option<usize>,
    
    /// Delay before reposting (seconds)
    pub repost_delay_secs: Option<u64>,
    
    /// Conditions that prevent reposting
    pub repost_conditions: Vec<RepostCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RepostCondition {
    /// Don't repost if daily loss limit reached
    DailyLossLimitReached,
    
    /// Don't repost if position limit reached
    PositionLimitReached,
    
    /// Don't repost if balance insufficient
    InsufficientBalance,
    
    /// Don't repost if P&L below threshold
    PnLBelowThreshold(Decimal),
}
```

### 2. Level Types (`trading_engine/level/level.rs`)

**Purpose**: Runtime level representation with resolved values.

```rust
#[derive(Debug, Clone)]
pub struct Level {
    pub config: LevelConfig,
    
    /// Current state
    pub state: LevelState,
    
    /// Active orders for this level
    pub active_orders: Vec<OrderId>,
    
    /// Repost count
    pub repost_count: usize,
    
    /// Last posted timestamp
    pub last_posted_at: Option<u64>,
    
    /// Last filled timestamp
    pub last_filled_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LevelState {
    /// Level created but not posted
    Created,
    
    /// Order posted, waiting for fill
    Posted,
    
    /// Order filled, waiting to repost
    Filled,
    
    /// Order cancelled, waiting to repost
    Cancelled,
    
    /// Level paused (manual or condition)
    Paused,
    
    /// Level removed
    Removed,
}

#[derive(Debug, Clone)]
pub struct ResolvedLevel {
    pub level: Level,
    
    /// Resolved price (from config + indicators)
    pub resolved_price: Decimal,
    
    /// Resolved quantity (from config + indicators)
    pub resolved_quantity: Decimal,
    
    /// Resolved order type
    pub resolved_order_type: OrderType,
}
```

### 3. Level Manager (`trading_engine/level/manager.rs`)

**Purpose**: Manages level lifecycle, state transitions, and reposting logic.

**Key Responsibilities**:
- Load levels from configuration
- Manage level state transitions
- Handle reposting logic
- Enable/disable levels dynamically
- Track level statistics

```rust
pub struct LevelManager {
    levels: HashMap<String, Level>,
    config_path: PathBuf,
}

impl LevelManager {
    /// Load levels from configuration file
    pub async fn load_from_config(&mut self, path: &Path) -> Result<(), EngineError>;
    
    /// Add a new level
    pub fn add_level(&mut self, config: LevelConfig) -> Result<(), EngineError>;
    
    /// Remove a level
    pub fn remove_level(&mut self, level_id: &str) -> Result<(), EngineError>;
    
    /// Enable a level
    pub fn enable_level(&mut self, level_id: &str) -> Result<(), EngineError>;
    
    /// Disable/pause a level
    pub fn disable_level(&mut self, level_id: &str) -> Result<(), EngineError>;
    
    /// Get level by ID
    pub fn get_level(&self, level_id: &str) -> Option<&Level>;
    
    /// Get all levels
    pub fn get_all_levels(&self) -> Vec<&Level>;
    
    /// Get levels by group
    pub fn get_levels_by_group(&self, group: &str) -> Vec<&Level>;
    
    /// Get levels ready to post (Created state, enabled)
    pub fn get_levels_ready_to_post(&self) -> Vec<&Level>;
    
    /// Update level state
    pub fn update_level_state(&mut self, level_id: &str, state: LevelState);
    
    /// Handle order filled for a level
    pub async fn handle_order_filled(
        &mut self,
        level_id: &str,
        order_id: &OrderId,
    ) -> Result<(), EngineError>;
    
    /// Handle order cancelled for a level
    pub async fn handle_order_cancelled(
        &mut self,
        level_id: &str,
        order_id: &OrderId,
    ) -> Result<(), EngineError>;
    
    /// Check if level should repost
    pub fn should_repost(&self, level_id: &str) -> Result<bool, EngineError>;
    
    /// Save configuration to file
    pub async fn save_config(&self) -> Result<(), EngineError>;
}
```

### 4. Level Executor (`trading_engine/level/executor.rs`)

**Purpose**: Executes levels by resolving indicator values and placing orders.

**Key Responsibilities**:
- Resolve indicator values for price/quantity
- Convert level config to OrderRequest
- Place orders via TradingClient
- Handle order placement errors

```rust
pub struct LevelExecutor {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    indicator_resolver: IndicatorResolver,
}

impl LevelExecutor {
    /// Resolve a level to an OrderRequest
    pub async fn resolve_level(
        &self,
        level: &Level,
        indicators: &HashMap<String, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<ResolvedLevel, EngineError>;
    
    /// Post a level (place order)
    pub async fn post_level(
        &mut self,
        level: &Level,
        indicators: &HashMap<String, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<Order, EngineError>;
    
    /// Cancel all orders for a level
    pub async fn cancel_level_orders(
        &self,
        level: &Level,
    ) -> Result<(), EngineError>;
}
```

### 5. Indicator Resolver (`trading_engine/level/resolver.rs`)

**Purpose**: Resolves indicator values for level configuration.

```rust
pub struct IndicatorResolver;

impl IndicatorResolver {
    /// Resolve price from PriceConfig
    pub fn resolve_price(
        &self,
        config: &PriceConfig,
        indicators: &HashMap<String, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<Decimal, EngineError>;
    
    /// Resolve quantity from QuantityConfig
    pub fn resolve_quantity(
        &self,
        config: &QuantityConfig,
        indicators: &HashMap<String, IndicatorValue>,
        balance: Decimal,
        current_price: Decimal,
    ) -> Result<Decimal, EngineError>;
}
```

### 6. Indicator Subscriber (`trading_engine/level/indicator_subscriber.rs`)

**Purpose**: Subscribes to indicator updates via gRPC streaming.

```rust
pub struct IndicatorSubscriber {
    grpc_client: IndicatorServiceClient,
    indicators: Arc<RwLock<HashMap<String, IndicatorValue>>>,
    cache_ttl: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorValue {
    pub name: String,
    pub value: Decimal,
    pub timestamp: u64,
    pub metadata: HashMap<String, String>,
}

impl IndicatorSubscriber {
    /// Start subscribing to indicator updates via gRPC stream
    pub async fn start(&mut self) -> Result<(), EngineError> {
        // Connect to indicator service via gRPC
        // Subscribe to indicator stream
        // Update local cache on each update
    }
    
    /// Get current indicator value (from cache)
    pub fn get_indicator(&self, name: &str) -> Option<IndicatorValue> {
        self.indicators.read().unwrap().get(name).cloned()
    }
    
    /// Get all indicators (from cache)
    pub fn get_all_indicators(&self) -> HashMap<String, IndicatorValue> {
        self.indicators.read().unwrap().clone()
    }
    
    /// Handle stream reconnection
    async fn handle_reconnection(&mut self) -> Result<(), EngineError>;
}
```

**Performance Optimizations**:
- **In-memory cache**: Fast lookups without network calls
- **gRPC streaming**: Low-latency, efficient binary protocol
- **Connection pooling**: Reuse gRPC connections
- **Automatic reconnection**: Handle disconnections gracefully

### 7. Risk Manager (`trading_engine/risk/manager.rs`)

**Purpose**: Orchestrates all risk checks before order placement.

```rust
pub struct RiskManager {
    position_checks: PositionChecks,
    balance_checks: BalanceChecks,
    pnl_checks: PnLChecks,
    daily_limits: DailyLimits,
}

#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub warnings: Vec<String>,
}

impl RiskManager {
    /// Perform all risk checks before posting a level
    pub async fn check_level(
        &self,
        level: &ResolvedLevel,
        current_state: &TradingState,
        balances: &[Balance],
    ) -> Result<RiskCheckResult, EngineError>;
    
    /// Check position limits
    pub fn check_position_limits(
        &self,
        level: &ResolvedLevel,
        current_state: &TradingState,
    ) -> RiskCheckResult;
    
    /// Check balance
    pub fn check_balance(
        &self,
        level: &ResolvedLevel,
        balances: &[Balance],
    ) -> RiskCheckResult;
    
    /// Check P&L
    pub fn check_pnl(
        &self,
        level: &ResolvedLevel,
        current_state: &TradingState,
    ) -> RiskCheckResult;
    
    /// Check daily loss limits
    pub fn check_daily_limits(
        &self,
        level: &ResolvedLevel,
        daily_pnl: Decimal,
    ) -> RiskCheckResult;
}
```

### 8. Risk Checks (`trading_engine/risk/`)

**Position Checks** (`position_checks.rs`):
- Per-level position limits
- Total position limits
- Order count limits

**Balance Checks** (`balance_checks.rs`):
- Sufficient balance for buy orders (quote currency)
- Sufficient balance for sell orders (base currency)
- Reserve balance requirements

**P&L Checks** (`pnl_checks.rs`):
- Current P&L validation
- P&L thresholds before posting

**Daily Limits** (`daily_limits.rs`):
- Daily loss limit (all pairs)
- Daily loss limit (per pair)
- Daily loss limit (per level)

### 9. Cancellation Manager (`trading_engine/cancellation/manager.rs`)

**Purpose**: Manages cancellation logic to prevent system wipeouts.

```rust
pub struct CancellationManager {
    conditions: Vec<CancellationCondition>,
}

#[derive(Debug, Clone)]
pub enum CancellationCondition {
    /// Cancel all orders if daily loss limit reached
    DailyLossLimitReached,
    
    /// Cancel all orders if total position exceeds limit
    TotalPositionExceeded(Decimal),
    
    /// Cancel all orders if balance below threshold
    BalanceBelowThreshold(Decimal),
    
    /// Cancel all orders if exchange connection lost
    ExchangeConnectionLost,
    
    /// Cancel orders for a specific level group
    LevelGroupPaused(String),
}

impl CancellationManager {
    /// Check cancellation conditions
    pub async fn check_conditions(
        &self,
        state: &TradingState,
        balances: &[Balance],
        daily_pnl: Decimal,
    ) -> Vec<CancellationAction>;
    
    /// Execute cancellation actions
    pub async fn execute_cancellations(
        &self,
        actions: Vec<CancellationAction>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    ) -> Result<(), EngineError>;
}
```

### 10. Event Loop (`trading_engine/event_loop.rs`)

**Purpose**: Refined event loop with risk checks and cancellation logic.

```rust
pub struct LevelBasedEventLoop {
    level_manager: Arc<RwLock<LevelManager>>,
    level_executor: Arc<RwLock<LevelExecutor>>,
    risk_manager: Arc<RiskManager>,
    cancellation_manager: Arc<CancellationManager>,
    indicator_subscriber: Arc<IndicatorSubscriber>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    market_data_stream: MarketDataStream,  // gRPC stream for orderbooks
}

impl LevelBasedEventLoop {
    /// Main event loop
    pub async fn run(&self) -> Result<(), EngineError> {
        // 1. Start indicator subscriber (gRPC stream)
        // 2. Start market data subscriber (gRPC stream)
        // 3. Subscribe to execution reports (WebSocket)
        
        loop {
            tokio::select! {
                // Handle orderbook updates from gRPC stream
                orderbook = self.market_data_stream.next() => {
                    self.handle_orderbook(orderbook?).await?;
                }
                // Handle execution reports
                report = self.execution_report_rx.recv() => {
                    self.handle_execution_report(report?).await?;
                }
            }
            
            // Periodic tasks
            // 1. Check cancellation conditions
            // 2. Get levels ready to post
            // 3. For each level:
            //    a. Resolve indicator values (from cache)
            //    b. Perform risk checks (cached metrics)
            //    c. Post if allowed
            // 4. Handle level reposting
        }
    }
    
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError> {
        // Get levels ready to post
        // Resolve indicators (from cache - fast)
        // Perform risk checks (cached metrics - fast)
        // Post levels if allowed
    }
}
```

**Performance Optimizations**:
- **gRPC streaming**: Efficient binary protocol for orderbooks
- **Cached indicators**: Fast lookups without network calls
- **Cached risk metrics**: Incremental updates, not full recalculation
- **Async processing**: Non-blocking operations

### 11. CLI (`cli/`)

**Purpose**: Command-line interface for level management.

**Commands**:
- `level add <config-file>` - Add level from config
- `level remove <level-id>` - Remove a level
- `level enable <level-id>` - Enable a level
- `level disable <level-id>` - Disable a level
- `level list` - List all levels
- `level status <level-id>` - Show level status
- `config load <path>` - Load configuration from file
- `config save` - Save current configuration
- `status` - Show system status

## Configuration Format

### JSON Configuration File

```json
{
  "levels": [
    {
      "id": "buy_level_1",
      "name": "Buy at SMA-2%",
      "symbol": {
        "base": "BTC",
        "quote": "USDT"
      },
      "side": "Buy",
      "order_type": "Limit",
      "quantity": {
        "type": "Fixed",
        "value": "0.001"
      },
      "price": {
        "type": "Indicator",
        "indicator_name": "sma_20",
        "offset_type": "PercentBelow",
        "offset_value": "2.0"
      },
      "risk": {
        "max_position": "0.01",
        "max_orders": 2,
        "daily_loss_limit": "50.0"
      },
      "lifecycle": {
        "auto_repost": true,
        "auto_repost_on_cancel": true,
        "max_reposts": null,
        "repost_delay_secs": 5,
        "repost_conditions": [
          "DailyLossLimitReached",
          "PositionLimitReached",
          "InsufficientBalance"
        ]
      },
      "enabled": true,
      "group": "main_strategy"
    },
    {
      "id": "sell_level_1",
      "name": "Sell at SMA+2%",
      "symbol": {
        "base": "BTC",
        "quote": "USDT"
      },
      "side": "Sell",
      "order_type": "Limit",
      "quantity": {
        "type": "Fixed",
        "value": "0.001"
      },
      "price": {
        "type": "Indicator",
        "indicator_name": "sma_20",
        "offset_type": "PercentAbove",
        "offset_value": "2.0"
      },
      "risk": {
        "max_position": "0.01",
        "max_orders": 2
      },
      "lifecycle": {
        "auto_repost": true,
        "auto_repost_on_cancel": true,
        "repost_delay_secs": 5
      },
      "enabled": true,
      "group": "main_strategy"
    }
  ],
  "risk_limits": {
    "total_max_position": "0.1",
    "max_total_orders": 20,
    "daily_loss_limit_all_pairs": "500.0",
    "daily_loss_limit_per_pair": {
      "BTCUSDT": "200.0"
    }
  }
}
```

## Integration with Indicator Service

### gRPC Service Definition

**Protobuf Definition** (`indicator_service.proto`):
```protobuf
syntax = "proto3";

service IndicatorService {
  // Stream indicator updates
  rpc StreamIndicators(StreamRequest) returns (stream IndicatorUpdate);
  
  // Request current indicator values
  rpc GetIndicators(GetIndicatorsRequest) returns (IndicatorResponse);
}

message IndicatorUpdate {
  string name = 1;
  string value = 2;
  int64 timestamp = 3;
  string symbol = 4;
  map<string, string> metadata = 5;
}
```

**Trading Engine Integration**:
- Connect to indicator service via gRPC
- Subscribe to indicator stream
- Update in-memory cache on each update
- Use cached values when resolving levels (fast, no network calls)

**Performance Benefits**:
- **Binary protocol**: 3-10x smaller than JSON
- **HTTP/2 multiplexing**: Lower latency
- **Streaming**: Real-time updates with backpressure
- **Caching**: Fast local lookups

## Migration Plan

### Phase 1: Core Level Infrastructure

1. **Create Level Types**
   - Implement `LevelConfig`, `Level`, `LevelState`
   - Create serialization/deserialization
   - Add validation logic

2. **Create Level Manager**
   - Implement lifecycle management
   - Add state transitions
   - Implement reposting logic

3. **Create Level Executor**
   - Implement indicator resolution
   - Convert levels to OrderRequest
   - Integrate with TradingClient

### Phase 2: Risk Management

1. **Create Risk Manager**
   - Implement risk check orchestration
   - Add position checks
   - Add balance checks
   - Add P&L checks
   - Add daily limit checks

2. **Integrate Risk Checks**
   - Add risk checks to event loop
   - Block order placement on risk violations
   - Log risk violations

### Phase 3: Cancellation Logic

1. **Create Cancellation Manager**
   - Implement cancellation conditions
   - Add cancellation logic
   - Integrate with TradingClient

2. **Integrate Cancellation**
   - Add cancellation checks to event loop
   - Execute cancellations when conditions met

### Phase 4: Indicator Integration

1. **Create Indicator Subscriber**
   - Subscribe to indicator channel (in-process) or gRPC (external)
   - Cache indicator values
   - Update cache on new values

2. **Create Indicator Resolver**
   - Resolve price from indicators
   - Resolve quantity from indicators
   - Handle missing indicators

### Phase 5: Event Loop Refinement

1. **Refine Event Loop**
   - Integrate level manager
   - Integrate risk manager
   - Integrate cancellation manager
   - Integrate indicator subscriber
   - Handle level reposting

### Phase 6: CLI

1. **Create CLI**
   - Implement level management commands
   - Implement config management commands
   - Implement status commands

2. **Create Config Loader**
   - Load JSON configuration
   - Validate configuration
   - Save configuration

## Testing Strategy

1. **Unit Tests**
   - Level configuration parsing
   - Level state transitions
   - Indicator resolution
   - Risk checks
   - Cancellation conditions

2. **Integration Tests**
   - Level lifecycle (create → post → fill → repost)
   - Risk check integration
   - Cancellation execution
   - Indicator subscription

3. **End-to-End Tests**
   - Full level-based trading flow
   - Multiple levels
   - Risk limit enforcement
   - Cancellation scenarios

## Success Criteria

- [ ] Levels can be configured from JSON
- [ ] Levels automatically repost after fill/cancel
- [ ] Risk checks prevent order placement when limits exceeded
- [ ] Cancellation logic prevents system wipeouts
- [ ] Indicators are resolved correctly for dynamic levels
- [ ] CLI can manage levels
- [ ] Configuration can be saved/loaded
- [ ] System handles missing indicators gracefully
- [ ] All existing tests pass
- [ ] New tests cover level functionality

## Future Enhancements (Out of Scope)

- UI for level management (separate proposal)
- MongoDB configuration storage (separate proposal)
- Advanced monitoring (separate proposal)
- Strategy-based trading (separate proposal)
- Python strategy integration (separate proposal)

## Performance Optimizations

### 1. Indicator Subscription (gRPC)
- **Binary protocol**: 3-10x smaller than JSON
- **Streaming**: Real-time updates with backpressure
- **In-memory cache**: Fast lookups without network calls
- **Connection pooling**: Reuse gRPC connections
- **Automatic reconnection**: Handle disconnections gracefully

### 2. Risk Check Optimization
- **Cached metrics**: Cache risk metrics with TTL (e.g., 1 second)
- **Incremental updates**: Update only changed metrics (post-trade)
- **State change detection**: Only recalculate if trading state changed
- **Check result caching**: Cache identical check requests (e.g., 100ms TTL)
- **Async processing**: Non-blocking risk checks

### 3. Level Execution
- **Cached indicators**: Fast lookups from in-memory cache
- **Efficient resolution**: Resolve price/quantity with cached exchange data
- **Batch processing**: Process multiple levels efficiently

### 4. Event Loop
- **gRPC streaming**: Efficient binary protocol for orderbooks
- **Select! macro**: Efficient async handling
- **Non-blocking**: All operations async

## Notes

- Indicator service is a separate crate/service (not in this proposal)
- Monitoring is a separate proposal (not in this document)
- Risk system details will be in a separate proposal
- Configuration format designed for future MongoDB migration
- Levels are exchange-agnostic through configuration abstraction
- Event loop includes built-in risk mitigation and cancellation logic
- **Channels for indicators** (in-process) - ultra-low latency streaming
- **gRPC for indicators** (external) - low-latency streaming if indicator service is separate
- **Performance optimized** - caching, batching, incremental updates

