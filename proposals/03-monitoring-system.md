# Proposal 03: Monitoring System

## Overview

This proposal outlines the implementation of a monitoring system that acts as a transport layer between the trading system and a **web-based UI** (future enhancement). The trading system handles all data fetching, updates, reconciliation, and aggregation. The monitoring system receives this data, transforms it to UI-optimized format, and publishes it via Redis pub/sub for low-latency delivery to the web server.

**Important Note**: This monitoring system is **optional** and only required if implementing a web-based UI. The **TUI CLI (Proposal 07)** connects directly to the Trading System via gRPC, bypassing the Monitoring Service and Redis entirely. For initial production deployment with TUI CLI, Redis and the Monitoring Service are **not required**.

## Key Requirements

1. **Transport Layer Only**: Monitoring system receives data from trading system, transforms it, and publishes to UI
2. **No Data Fetching**: All fetching, updates, reconciliation happen in trading system
3. **Data Transformation**: System format → UI format transformation
4. **Pub/Sub Delivery**: Low-latency Redis pub/sub (not push-based) to web server
5. **Aggregation**: Receives aggregated data (per pair, per level, overall) from trading system
6. **No State Management**: Stateless - just receives, transforms, publishes
7. **Optional for TUI**: Not required for TUI CLI (Proposal 07) - TUI connects directly to Trading System via gRPC
8. **Required for Web UI**: Only needed when implementing web-based UI (future enhancement)

## Architecture Overview

### System Separation

```
┌─────────────────┐
│ Trading System  │
│  (trading_engine)│
│                 │
│ - Fetches data  │
│ - Updates state │
│ - Aggregates    │
│ - Reconciles    │
└────────┬────────┘
         │ Channel (in-process) OR gRPC (external)
         │ (orders, balances, positions, trades)
         ▼
┌─────────────────┐
│  Monitoring     │
│    Service      │
│                 │
│ - Receives data │
│ - Transforms    │
│ - Publishes UI  │
└────────┬────────┘
         │ Redis Pub/Sub (occasional snapshots)
         │ (monitoring:ui) - Only for web UI
         ▼
┌─────────────────┐
│   Web Server    │
│   (UI Backend)  │
└─────────────────┘
```

### Data Flow

```
1. Trading System:
   Exchange → Trading System (fetch, update, aggregate) → gRPC Stream → Monitoring Service

2. Monitoring System:
   gRPC Stream (trading:data) → Monitoring Service (transform) → Redis (monitoring:ui)

3. Web Server:
   Redis (monitoring:ui) → Web Server → UI Clients
```

**Communication Strategy**:
- **In-Process Channels**: Trading System → Monitoring Service (if in-process, ultra-low latency < 1μs)
- **External gRPC**: Trading System → Monitoring Service (if separate process, low-latency 0.5-2ms)
- **Redis Pub/Sub**: Monitoring Service → Web Server (broadcast, multiple subscribers, only for web UI)

**Alternative: TUI CLI (Proposal 07)**:
- **Direct gRPC**: TUI CLI → Trading System (external process, bypasses Monitoring Service and Redis)
- **No Redis Required**: TUI CLI does not use Redis or Monitoring Service
- **Simpler Architecture**: Direct connection for terminal-based interface

## Component Structure

```
trading_system/
├── src/
│   ├── trading_engine/
│   │   ├── monitoring/              # NEW: Monitoring data publisher in trading system
│   │   │   ├── mod.rs
│   │   │   ├── publisher.rs         # Publishes data to Redis (trading:data)
│   │   │   ├── aggregator.rs        # Aggregates data (per pair, per level, overall)
│   │   │   └── data_builder.rs      # Builds monitoring data from trading state
│   │   │
│   │   └── ... (existing trading engine)
│   │
│   ├── monitoring_service/          # NEW: Separate monitoring service (transport layer)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs              # Service entry point
│   │   │   ├── service.rs            # Main service orchestrator
│   │   │   ├── grpc/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── client.rs         # gRPC client for trading system
│   │   │   │   └── server.rs         # gRPC server (if needed)
│   │   │   ├── subscriber.rs        # Subscribes to trading:data via gRPC stream
│   │   │   ├── transformer.rs       # System → UI format transformation
│   │   │   ├── publisher.rs          # Publishes to Redis (monitoring:ui)
│   │   │   └── config.rs            # Monitoring configuration
│   │   │
│   │   └── config/
│   │       └── monitoring_config.json
│   │
│   └── ... (existing)
```

## Detailed Component Design

### 1. Trading System: Monitoring Data Publisher (`trading_engine/monitoring/publisher.rs`)

**Purpose**: Publishes monitoring data from trading system via gRPC streaming.

**Key Responsibilities**:
- Build monitoring data from trading state
- Aggregate data (per pair, per level, overall)
- Stream updates via gRPC to monitoring service

```rust
pub struct MonitoringPublisher {
    grpc_client: TradingMonitoringClient,
    aggregator: Aggregator,
    connection_pool: ConnectionPool,
    batch_buffer: Vec<TradingUpdate>,  // For batching
    batch_size: usize,
    flush_interval: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradingSystemData {
    /// All open orders
    orders: Vec<Order>,
    
    /// All balances
    balances: Vec<Balance>,
    
    /// All trades (last 24h)
    trades: Vec<Trade>,
    
    /// All positions
    positions: Vec<Position>,
    
    /// Aggregated data
    aggregated: AggregatedData,
    
    /// Timestamp
    timestamp: u64,
}

impl MonitoringPublisher {
    /// Start gRPC streaming server
    pub async fn start_streaming(&mut self) -> Result<(), EngineError>;
    
    /// Publish order update (buffered, batched)
    pub async fn publish_order_update(
        &mut self,
        order: &Order,
    ) -> Result<(), EngineError> {
        // Add to batch buffer
        // Flush if buffer full or interval elapsed
    }
    
    /// Publish balance update (buffered, batched)
    pub async fn publish_balance_update(
        &mut self,
        balance: &Balance,
    ) -> Result<(), EngineError>;
    
    /// Publish trade update (buffered, batched)
    pub async fn publish_trade_update(
        &mut self,
        trade: &Trade,
    ) -> Result<(), EngineError>;
    
    /// Publish position update (buffered, batched)
    pub async fn publish_position_update(
        &mut self,
        position: &Position,
    ) -> Result<(), EngineError>;
    
    /// Publish aggregated data update (buffered, batched)
    pub async fn publish_aggregated(
        &mut self,
        aggregated: &AggregatedData,
    ) -> Result<(), EngineError>;
    
    /// Flush batch buffer
    async fn flush_batch(&mut self) -> Result<(), EngineError>;
}
```

### 2. Trading System: Aggregator (`trading_engine/monitoring/aggregator.rs`)

**Purpose**: Aggregates data per pair, per level, and overall in trading system.

```rust
pub struct Aggregator;

#[derive(Debug, Clone, Serialize)]
pub struct AggregatedData {
    /// Overall statistics
    overall: OverallStatistics,
    
    /// Statistics per pair
    per_pair: HashMap<Pair, PairStatistics>,
    
    /// Statistics per level
    per_level: HashMap<String, LevelStatistics>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverallStatistics {
    total_orders: usize,
    total_positions: usize,
    total_position_value: Decimal,
    total_pnl: Decimal,
    total_balance: Decimal,
    active_pairs: usize,
    active_levels: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairStatistics {
    pair: Pair,
    orders: Vec<Order>,
    order_count: usize,
    position: Option<Position>,
    pnl: Decimal,
    trades: Vec<Trade>,
    trade_count: usize,
    base_balance: Decimal,
    quote_balance: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct LevelStatistics {
    level_id: String,
    orders: Vec<Order>,
    order_count: usize,
    filled_count_today: usize,
    filled_quantity_today: Decimal,
    pnl: Decimal,
    last_fill_at: Option<u64>,
}

impl Aggregator {
    /// Aggregate all data from trading state
    pub fn aggregate(
        &self,
        state: &TradingState,
        levels: Option<&HashMap<String, Level>>,
    ) -> AggregatedData;
    
    /// Calculate overall statistics
    fn calculate_overall(&self, state: &TradingState) -> OverallStatistics;
    
    /// Calculate per-pair statistics
    fn calculate_per_pair(&self, state: &TradingState) -> HashMap<Pair, PairStatistics>;
    
    /// Calculate per-level statistics
    fn calculate_per_level(
        &self,
        state: &TradingState,
        levels: Option<&HashMap<String, Level>>,
    ) -> HashMap<String, LevelStatistics>;
}
```

### 3. Trading System: Data Builder (`trading_engine/monitoring/data_builder.rs`)

**Purpose**: Builds complete monitoring data from trading state.

```rust
pub struct DataBuilder;

impl DataBuilder {
    /// Build complete monitoring data from trading state
    pub fn build(
        &self,
        state: &TradingState,
        levels: Option<&HashMap<String, Level>>,
        aggregator: &Aggregator,
    ) -> TradingSystemData;
}
```

### 4. Monitoring Service: Main Service (`monitoring_service/src/service.rs`)

**Purpose**: Main orchestrator for monitoring service (transport layer).

```rust
pub struct MonitoringService {
    subscriber: Subscriber,
    transformer: Transformer,
    publisher: Publisher,
    config: MonitoringConfig,
}

impl MonitoringService {
    /// Start the monitoring service
    pub async fn start(&mut self) -> Result<(), MonitoringError> {
        // 1. Subscribe to trading:data from Redis
        // 2. Transform received data to UI format
        // 3. Publish to monitoring:ui channels
    }
    
    /// Stop the monitoring service
    pub async fn stop(&mut self) -> Result<(), MonitoringError>;
}
```

### 5. Monitoring Service: Subscriber (`monitoring_service/src/subscriber.rs`)

**Purpose**: Subscribes to trading system data via gRPC streaming.

**Key Responsibilities**:
- Connect to trading system via gRPC
- Subscribe to trading data stream
- Receive updates from trading system
- Pass data to transformer

```rust
pub struct Subscriber {
    grpc_client: TradingMonitoringClient,
    data_receiver: mpsc::Receiver<TradingSystemData>,
    connection_pool: ConnectionPool,
}

impl Subscriber {
    /// Start subscribing to trading:data via gRPC stream
    pub async fn start(&mut self) -> Result<(), MonitoringError> {
        // Connect to trading system gRPC service
        // Subscribe to trading data stream
        // Handle reconnection automatically
    }
    
    /// Receive next data update from gRPC stream
    pub async fn receive(&mut self) -> Option<TradingSystemData>;
    
    /// Handle stream reconnection
    async fn handle_reconnection(&mut self) -> Result<(), MonitoringError>;
}
```

**Performance Benefits**:
- **Binary protocol**: 3-10x smaller than JSON
- **HTTP/2 multiplexing**: Lower latency
- **Streaming**: Real-time updates with backpressure
- **Connection pooling**: Reuse connections
- **Automatic reconnection**: Handle disconnections gracefully

### 6. Monitoring Service: Transformer (`monitoring_service/src/transformer.rs`)

**Purpose**: Transforms system data format to UI-optimized format.

```rust
pub struct Transformer;

/// UI-optimized order model
#[derive(Debug, Clone, Serialize)]
pub struct UIOrder {
    pub id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
    pub filled_quantity: String,
    pub status: String,
    pub created_at: String,
    pub level_id: Option<String>,
}

/// UI-optimized balance model
#[derive(Debug, Clone, Serialize)]
pub struct UIBalance {
    pub symbol: String,
    pub free: String,
    pub used: String,
    pub total: String,
}

/// UI-optimized trade model
#[derive(Debug, Clone, Serialize)]
pub struct UITrade {
    pub id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
    pub timestamp: String,
    pub order_id: String,
}

/// UI-optimized position model
#[derive(Debug, Clone, Serialize)]
pub struct UIPosition {
    pub symbol: String,
    pub quantity: String,
    pub avg_price: String,
    pub current_price: String,
    pub pnl: String,
    pub pnl_percent: String,
}

/// Complete UI data model
#[derive(Debug, Clone, Serialize)]
pub struct UIMonitoringData {
    pub orders: Vec<UIOrder>,
    pub balances: Vec<UIBalance>,
    pub trades: Vec<UITrade>,
    pub positions: Vec<UIPosition>,
    pub aggregated: AggregatedData,  // Same as system format (already UI-friendly)
    pub timestamp: u64,
}

impl Transformer {
    /// Transform trading system data to UI format
    pub fn transform(&self, data: &TradingSystemData) -> UIMonitoringData;
    
    /// Transform order to UI format
    fn transform_order(&self, order: &Order) -> UIOrder;
    
    /// Transform balance to UI format
    fn transform_balance(&self, balance: &Balance) -> UIBalance;
    
    /// Transform trade to UI format
    fn transform_trade(&self, trade: &Trade) -> UITrade;
    
    /// Transform position to UI format
    fn transform_position(&self, position: &Position, current_price: Decimal) -> UIPosition;
}
```

### 7. Monitoring Service: Publisher (`monitoring_service/src/publisher.rs`)

**Purpose**: Publishes transformed UI data to Redis for web server consumption.

**Key Responsibilities**:
- Publish UI data to Redis channels
- Use pub/sub pattern (not push-based)
- Low-latency delivery

```rust
pub struct Publisher {
    redis_client: RedisClient,
}

/// Redis channels for UI pub/sub
pub const MONITORING_ORDERS_CHANNEL: &str = "monitoring:orders";
pub const MONITORING_BALANCES_CHANNEL: &str = "monitoring:balances";
pub const MONITORING_TRADES_CHANNEL: &str = "monitoring:trades";
pub const MONITORING_POSITIONS_CHANNEL: &str = "monitoring:positions";
pub const MONITORING_AGGREGATED_CHANNEL: &str = "monitoring:aggregated";
pub const MONITORING_FULL_CHANNEL: &str = "monitoring:full";  // Complete snapshot

impl Publisher {
    /// Publish full monitoring data
    pub async fn publish_full(
        &self,
        data: &UIMonitoringData,
    ) -> Result<(), MonitoringError>;
    
    /// Publish order update
    pub async fn publish_order_update(
        &self,
        order: &UIOrder,
    ) -> Result<(), MonitoringError>;
    
    /// Publish balance update
    pub async fn publish_balance_update(
        &self,
        balance: &UIBalance,
    ) -> Result<(), MonitoringError>;
    
    /// Publish trade update
    pub async fn publish_trade_update(
        &self,
        trade: &UITrade,
    ) -> Result<(), MonitoringError>;
    
    /// Publish position update
    pub async fn publish_position_update(
        &self,
        position: &UIPosition,
    ) -> Result<(), MonitoringError>;
    
    /// Publish aggregated data
    pub async fn publish_aggregated(
        &self,
        aggregated: &AggregatedData,
    ) -> Result<(), MonitoringError>;
}
```

### 8. Configuration (`monitoring_service/src/config.rs`)

**Purpose**: Monitoring service configuration.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Redis URL for subscribing to trading:data
    pub redis_url: String,
    
    /// Publishing interval (seconds) - how often to publish full snapshot
    pub publishing_interval_secs: u64,
}
```

## gRPC Service Definition

### Trading System → Monitoring System

**Protobuf Definition** (`trading_monitoring.proto`):
```protobuf
syntax = "proto3";

service TradingMonitoring {
  // Stream trading updates (high-frequency)
  rpc StreamUpdates(StreamRequest) returns (stream TradingUpdate);
  
  // Request current state (on-demand)
  rpc GetCurrentState(StateRequest) returns (TradingState);
}

message TradingUpdate {
  oneof update {
    OrderUpdate order = 1;
    BalanceUpdate balance = 2;
    PositionUpdate position = 3;
    TradeUpdate trade = 4;
    AggregatedUpdate aggregated = 5;
  }
  int64 timestamp = 6;
}

message OrderUpdate {
  string order_id = 1;
  string symbol = 2;
  string side = 3;
  string price = 4;
  string quantity = 5;
  string status = 6;
}

// ... other message types
```

**Benefits**:
- **Binary protocol**: Efficient serialization
- **Streaming**: Real-time updates with backpressure
- **Type-safe**: Compile-time checks
- **Lower latency**: HTTP/2 multiplexing

### Monitoring System → Web Server

**Redis Pub/Sub Channels** (for broadcast to multiple web server instances):
- `monitoring:orders` - Order updates
- `monitoring:balances` - Balance updates
- `monitoring:trades` - Trade updates
- `monitoring:positions` - Position updates
- `monitoring:aggregated` - Aggregated data
- `monitoring:full` - Complete snapshot

**Publishing Strategy**:
- **Batched updates**: Publish every N updates or every X milliseconds (configurable)
- **Debouncing**: Avoid publishing duplicate data
- **Full snapshots**: Periodic full snapshots (e.g., every 5 seconds)
- **Incremental updates**: Publish only changed data

**Message Format** (UI-optimized):
```json
{
  "type": "full_snapshot",
  "data": {
    "orders": [
      {
        "id": "12345",
        "symbol": "BTCUSDT",
        "side": "Buy",
        "price": "45000.00",
        "quantity": "0.001",
        "filled_quantity": "0.000",
        "status": "New",
        "created_at": "2024-01-15T10:00:00Z",
        "level_id": "buy_level_1"
      }
    ],
    "balances": [...],
    "trades": [...],
    "positions": [...],
    "aggregated": {
      "overall": {...},
      "per_pair": {...},
      "per_level": {...}
    }
  },
  "timestamp": 1234567890
}
```

## Integration in Trading System

### Trading System Updates

The trading system needs to:

1. **Bootstrap Data Fetching** (in trading system):
   - Fetch open orders for configured pairs
   - Fetch balances
   - Fetch trades (last 24h) for configured pairs
   - Publish to `trading:data` channel

2. **Real-time Updates** (in trading system):
   - Subscribe to WebSocket updates
   - Update internal state
   - Publish updates to `trading:data` channel

3. **Reconciliation** (in trading system):
   - Periodic REST reconciliation when WS unavailable
   - Publish updates to `trading:data` channel

4. **Aggregation** (in trading system):
   - Aggregate data periodically
   - Publish aggregated data to `trading:data` channel

### Example: Trading System Event Loop Integration

```rust
// In trading_engine event loop
let monitoring_publisher = MonitoringPublisher::new(redis_client);

// On bootstrap
let data = data_builder.build(&trading_state, &levels, &aggregator);
monitoring_publisher.publish_full(&data).await?;

// On order update
monitoring_publisher.publish_order_update(&order).await?;

// Periodic full snapshot
tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let aggregated = aggregator.aggregate(&trading_state, &levels);
        let data = data_builder.build(&trading_state, &levels, &aggregator);
        monitoring_publisher.publish_full(&data).await?;
    }
});
```

## Service Lifecycle

### Monitoring Service Startup

1. **Load Configuration**
   - Load gRPC endpoint for trading system
   - Load Redis URL for publishing to web server
   - Initialize gRPC client with connection pooling
   - Initialize Redis client

2. **Start gRPC Subscriber**
   - Connect to trading system via gRPC
   - Subscribe to trading data stream
   - Handle reconnection automatically

3. **Start Publishing Loop**
   - Transform received data to UI format
   - Batch updates (configurable interval)
   - Publish to Redis `monitoring:ui` channels
   - Publish full snapshots periodically

### Update Flow

```
Trading System (updates state) → gRPC Stream → 
Monitoring Service (receives) → Transform → Batch → Publish to Redis (monitoring:ui) → 
Web Server (subscribes) → UI Clients
```

**Performance Optimizations**:
- **gRPC streaming**: Efficient binary protocol for high-frequency updates
- **Batching**: Reduce Redis publish frequency
- **Debouncing**: Avoid duplicate publishes
- **Caching**: Cache transformed data to avoid re-transformation

## Error Handling

1. **Redis Connection Loss**
   - Retry with exponential backoff
   - Log errors
   - Continue when reconnected

2. **Missing Data**
   - Handle gracefully
   - Use default/empty values
   - Log warnings

3. **Transformation Errors**
   - Log errors
   - Skip problematic data
   - Continue processing

## Performance Considerations

1. **Efficient Transformation**
   - Batch transformations where possible
   - Use efficient serialization
   - Cache transformed data if unchanged

2. **Publishing**
   - Publish only on changes (not every tick)
   - Batch updates where possible
   - Use efficient message formats

3. **Stateless Design**
   - No state management in monitoring service
   - Just transform and forward
   - Minimal memory footprint

## Testing Strategy

1. **Unit Tests**
   - Transformation logic
   - Publisher logic
   - Subscriber logic

2. **Integration Tests**
   - End-to-end: Trading system → Monitoring → Redis
   - Test Redis pub/sub flow
   - Test error scenarios

3. **Mock Tests**
   - Mock Redis client
   - Mock trading system data
   - Test transformation edge cases

## Success Criteria

- [ ] Monitoring service receives data from trading system via Redis
- [ ] Data transformed to UI format correctly
- [ ] Data published to Redis channels for UI
- [ ] Web server can subscribe and receive updates
- [ ] No state management in monitoring service (stateless)
- [ ] Low latency (pub/sub, not push-based)
- [ ] Service is simple transport layer only

## Future Enhancements (Out of Scope)

- Database persistence (separate proposal)
- Historical data storage
- Advanced analytics
- Alerting system
- Performance metrics

## Performance Optimizations

### 1. gRPC Streaming (Trading → Monitoring)
- **Binary protocol**: 3-10x smaller than JSON (protobuf)
- **HTTP/2 multiplexing**: Lower latency
- **Streaming**: Real-time updates with backpressure
- **Connection pooling**: Reuse gRPC connections
- **Batching**: Batch updates (e.g., every 50ms or 20 updates)

### 2. Redis Pub/Sub (Monitoring → Web Server)
- **Batching**: Batch updates (e.g., every 100ms or 10 updates)
- **Debouncing**: Avoid publishing duplicate data
- **Incremental updates**: Publish only changed data
- **Full snapshots**: Periodic full snapshots (e.g., every 5 seconds)
- **Connection pooling**: Reuse Redis connections

### 3. Transformation
- **Caching**: Cache transformed data to avoid re-transformation
- **Incremental transformation**: Only transform changed data
- **Lazy evaluation**: Transform only when needed

### 4. Stateless Design
- **No state management**: Minimal memory footprint
- **Fast forwarding**: Just transform and publish
- **Efficient serialization**: Use efficient JSON serialization

## Notes

- **Monitoring service is a transport layer only** - no fetching, no state, just transform and publish
- **All data fetching happens in trading system** - bootstrap, updates, reconciliation
- **All aggregation happens in trading system** - per pair, per level, overall
- **gRPC streaming for high-frequency** - trading system → monitoring (efficient, batched)
- **Redis pub/sub for broadcasts** - monitoring → web server (occasional snapshots, debounced)
- **Monitoring service publishes to monitoring:ui** - UI-optimized format
- **Stateless design** - monitoring service has no state, just transforms and forwards
- **Performance optimized** - batching, debouncing, caching, connection pooling
- **Optional for TUI CLI** - TUI CLI (Proposal 07) connects directly to Trading System via gRPC, bypassing Monitoring Service and Redis
- **Required only for web UI** - Redis and Monitoring Service are only needed when implementing web-based UI (future enhancement)
