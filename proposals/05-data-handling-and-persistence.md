# Proposal 05: State Management Architecture

## Overview

This proposal outlines the implementation of state management architecture across the trading system. It defines a clear separation between **State Managers** (full state management with publishing) and **Caching Only** (fast lookup, no state management). It defines a clear architecture where:

1. **State Managers** (full state management):
   - **ExchangeStateManager** (Exchange Engine): Manages raw exchange data (balances, orders, trades from WebSocket), performs WS vs REST reconciliation, publishes to channels
   - **MarketDataStateManager** (Market Data Engine): Manages market data state (orderbooks, public trades), publishes to channels
   - **TradingState** (Trading Engine): Manages trading-specific state (positions, active orders), derived from ExchangeStateManager via channels, does NOT call exchange directly

2. **Caching Only** (no state managers):
   - **Indicator Engine**: Caches calculated indicator values (fast lookup)
   - **Risk Engine**: Caches risk metrics (fast lookup, can also call exchange directly for verification)
   - **Asset Engine**: Caches static asset metadata (symbols, trading rules)

**Key Architectural Principle**: Trading Engine relies on Exchange Engine state via channels. Trading Engine does NOT call exchange directly - that's Exchange Engine's responsibility. Risk Engine acts as an independent checker that can call exchange directly to verify state correctness.

**Note**: 
- MongoDB persistence and configuration storage are covered in **Proposal 06 (MongoDB Persistence and Configuration Storage)**.
- Runtime configuration changes (LiveConfig Service) are covered in **Proposal 06b (LiveConfig Service)**.
- This proposal focuses **only on state management architecture** (in-memory state, channels, reconciliation).

## Key Requirements

### State Management Architecture

1. **ExchangeStateManager** (Exchange Engine):
   - Manages raw exchange data: balances, orders, trades from WebSocket
   - Performs WS vs REST reconciliation (exchange-level reconciliation)
   - Publishes all updates via broadcast channels to Trading Engine, Risk Engine, TUI
   - **Single point of contact with exchange** for account data
   - Trading Engine relies on this state (does NOT call exchange directly)

2. **MarketDataStateManager** (Market Data Engine):
   - Manages market data state: orderbooks, public trades
   - Publishes all updates via broadcast channels to Indicator Engine, Trading Engine, TUI
   - No reconciliation needed (market data is public)

3. **TradingState** (Trading Engine):
   - Manages trading-specific state: positions (derived from balances), active orders (only orders we placed)
   - **Derived from ExchangeStateManager** via channel subscriptions
   - **Does NOT call exchange directly** - relies on ExchangeStateManager
   - Reconciles with ExchangeStateManager (not with exchange directly)
   - Used by strategies and levels for trading decisions

4. **Caching Only** (no state managers):
   - **Indicator Engine**: Caches calculated indicator values (fast lookup, < 0.1ms)
   - **Risk Engine**: Caches risk metrics (fast lookup), can also call exchange directly for independent verification
   - **Asset Engine**: Caches static asset metadata (symbols, trading rules, TTL-based refresh)

### Publishing

5. **Publishing**: All state managers publish updates via channels (broadcast for fan-out)

## Architecture Overview

### State Management Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    STATE MANAGERS                                │
│  (Full state management with publishing via channels)             │
└─────────────────────────────────────────────────────────────────┘

Exchange Engine
    ↓
ExchangeStateManager
    ├── In-Memory State (raw exchange data)
    │   ├── Balances (all balances from WS)
    │   ├── Orders (all orders from WS)
    │   └── Trades (execution reports from WS)
    ├── WS vs REST Reconciliation (exchange-level)
    ├── Publish via Broadcast Channel
    │   ├──→ Trading Engine (subscribes)
    │   ├──→ Risk Engine (subscribes)
    │   └──→ TUI (via gRPC aggregation)

Market Data Engine
    ↓
MarketDataStateManager
    ├── In-Memory State (market data)
    │   ├── Orderbooks (latest per pair)
    │   └── Public Trades (recent trades)
    ├── Publish via Broadcast Channel
    │   ├──→ Indicator Engine (subscribes)
    │   ├──→ Trading Engine (subscribes)
    │   └──→ TUI (via gRPC aggregation)

Trading Engine
    ↓
TradingState
    ├── In-Memory State (trading-specific)
    │   ├── Positions (derived from ExchangeStateManager balances)
    │   ├── Active Orders (only orders we placed)
    │   └── Pending Orders (orders we're tracking)
    ├── Derived from ExchangeStateManager (via channel subscriptions)
    ├── Reconciles with ExchangeStateManager (not with exchange directly)
    ├── Does NOT call exchange directly
    └── Used by strategies/levels for trading decisions

┌─────────────────────────────────────────────────────────────────┐
│                    CACHING ONLY                                  │
│  (Fast lookup, no state management, no publishing)              │
└─────────────────────────────────────────────────────────────────┘

Indicator Engine
    └── Cache (calculated indicator values)
        ├── Fast lookup (< 0.1ms)
        └── Updated from MarketDataStateManager (via channels)

Risk Engine
    └── Cache (risk metrics)
        ├── Fast lookup (cached calculations)
        ├── Updated from ExchangeStateManager (via channels)
        └── Can call exchange directly (independent verification)

Asset Engine
    └── Cache (static asset metadata)
        ├── Symbols, trading rules, exchange limits
        ├── TTL-based refresh from REST API
        └── No state management (static data)
```

### Data Flow

```
1. Exchange Updates:
   Exchange (WS) → ExchangeStateManager
       ├── Update in-memory state
       ├── WS vs REST Reconciliation (periodic)
       ├── Publish via Broadcast Channel
       │   ├──→ Trading Engine (updates TradingState)
       │   ├──→ Risk Engine (updates risk cache)
       │   └──→ TUI (via gRPC aggregation)
       └── (MongoDB persistence handled in Proposal 06)

2. Trading Engine State Updates:
   ExchangeStateManager (via channel) → Trading Engine
       ├── Receive execution reports, balances, orders
       ├── Update TradingState (positions, active orders)
       ├── Reconcile TradingState with ExchangeStateManager (periodic)
       └── Used by strategies/levels for trading decisions

3. Market Data Updates:
   Exchange (WS) → MarketDataStateManager
       ├── Update in-memory state
       ├── Publish via Broadcast Channel
       │   ├──→ Indicator Engine (updates indicator cache)
       │   ├──→ Trading Engine (for level execution)
       │   └──→ TUI (via gRPC aggregation)
       └── (MongoDB persistence handled in Proposal 06)

4. Risk Engine Verification (Independent Checker):
   Risk Engine
       ├── Receives data from ExchangeStateManager (via channels)
       ├── Can call exchange directly (TradingClient) for verification
       ├── Compares ExchangeStateManager state vs direct exchange calls
       └── Alarms if discrepancies detected

5. Configuration Updates:
   Level Manager → MongoDB (see Proposal 06)
```

### Key Architectural Principles

1. **Exchange Engine is the single point of contact with exchange** for account data
2. **Trading Engine relies on ExchangeStateManager** - does NOT call exchange directly
3. **TradingState reconciles with ExchangeStateManager** (not with exchange directly)
4. **Risk Engine is independent checker** - can call exchange directly to verify state correctness
5. **State Managers** (Exchange, Market Data, Trading) manage full state and publish
6. **Caching Only** (Indicator, Risk, Asset) - fast lookup, no state management

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
│   │   │           ├── publisher.rs    # Publishes via broadcast channels
│   │   │           └── reconciler.rs   # WS vs REST reconciliation
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
│   │   │           └── publisher.rs    # Publishes via broadcast channels
│   │   │
│   │   └── shared/
│   ├── trading_engine/
│   │   └── state/
│   │       ├── subscriber.rs         # Subscribes to ExchangeStateManager
│   │       └── reconciliation.rs     # TradingState reconciliation
│   │
│   └── ... (existing)
│
│   Note: LiveConfig Service is implemented in Proposal 06
```

## Detailed Component Design

### 1. Exchange State Manager (`exchange_engine/src/exchanges/binance/state/manager.rs`)

**Purpose**: Manages raw exchange data (balances, orders, trades from WebSocket), performs WS vs REST reconciliation, publishes to channels. **Single point of contact with exchange for account data.**

```rust
pub struct ExchangeStateManager {
    state: Arc<RwLock<ExchangeState>>,
    publisher: ChannelPublisher,  // Broadcast channel publisher
    reconciler: StateReconciler,  // WS vs REST reconciliation
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,  // For REST reconciliation
}

impl ExchangeStateManager {
    /// Update order from WebSocket and publish
    pub async fn update_order(&self, order: Order) -> Result<(), StateError> {
        // 1. Update in-memory state
        let mut state = self.state.write().await;
        state.update_order(order.clone());
        
        // 2. Publish via broadcast channel
        self.publisher.publish_order(&order).await?;
        
        Ok(())
    }
    
    /// Update balance from WebSocket and publish
    pub async fn update_balance(&self, balance: Balance) -> Result<(), StateError>;
    
    /// Add trade from WebSocket and publish
    pub async fn add_trade(&self, trade: Trade) -> Result<(), StateError>;
    
    /// Get current state (for Trading Engine reconciliation)
    pub fn get_state(&self) -> Arc<RwLock<ExchangeState>>;
    
    /// Get balance for a currency (for Trading Engine)
    pub async fn get_balance(&self, currency: &str) -> Option<Balance>;
    
    /// Get orders for a pair (for Trading Engine)
    pub async fn get_orders_for_pair(&self, pair: &Pair) -> Vec<Order>;
    
    /// Start periodic WS vs REST reconciliation
    pub async fn start_reconciliation(&self, interval: Duration);
}
```

### 2. Exchange State (`exchange_engine/src/exchanges/binance/state/state.rs`)

**Purpose**: In-memory state structure for raw exchange data (what the exchange says).

```rust
pub struct ExchangeState {
    /// Orders by order ID (all orders from exchange)
    orders: HashMap<OrderId, Order>,
    
    /// Orders by pair (all orders from exchange)
    orders_by_pair: HashMap<Pair, Vec<OrderId>>,
    
    /// Balances by currency symbol (all balances from exchange)
    balances: HashMap<String, Balance>,
    
    /// Recent private trades (last N from exchange)
    private_trades: Vec<Trade>,
    
    /// Last update timestamp
    last_update: u64,
}

impl ExchangeState {
    pub fn new() -> Self;
    
    pub fn update_order(&mut self, order: Order);
    
    pub fn remove_order(&mut self, order_id: &OrderId);
    
    pub fn update_balance(&mut self, balance: Balance);
    
    pub fn add_trade(&mut self, trade: Trade);
    
    pub fn get_orders(&self) -> Vec<&Order>;
    
    pub fn get_orders_for_pair(&self, pair: &Pair) -> Vec<&Order>;
    
    pub fn get_balances(&self) -> Vec<&Balance>;
    
    pub fn get_balance(&self, currency: &str) -> Option<&Balance>;
    
    pub fn get_trades(&self) -> Vec<&Trade>;
}
```

**Note**: ExchangeState does NOT track positions - positions are derived from balances by Trading Engine.

### 3. Exchange State Publisher (`exchange_engine/src/exchanges/binance/state/publisher.rs`)

**Purpose**: Publishes state updates via broadcast channels (in-process, ultra-low latency).

```rust
pub struct ChannelPublisher {
    /// Broadcast channel sender for exchange updates
    exchange_update_tx: tokio::sync::broadcast::Sender<ExchangeUpdate>,
}

#[derive(Debug, Clone)]
pub enum ExchangeUpdate {
    Order(Order),
    Balance(Balance),
    Trade(Trade),
}

impl ChannelPublisher {
    pub fn new(exchange_update_tx: tokio::sync::broadcast::Sender<ExchangeUpdate>) -> Self {
        Self { exchange_update_tx }
    }
    
    /// Publish order update via broadcast channel
    pub async fn publish_order(&self, order: &Order) -> Result<(), StateError> {
        self.exchange_update_tx
            .send(ExchangeUpdate::Order(order.clone()))
            .map_err(|e| StateError::PublishError(format!("Failed to publish order: {}", e)))?;
        Ok(())
    }
    
    /// Publish balance update via broadcast channel
    pub async fn publish_balance(&self, balance: &Balance) -> Result<(), StateError>;
    
    /// Publish trade update via broadcast channel
    pub async fn publish_trade(&self, trade: &Trade) -> Result<(), StateError>;
}
```

**Performance Optimizations**:
- **Broadcast channels**: Ultra-low latency (< 1μs), efficient fan-out
- **No batching needed**: Channels are fast enough without batching
- **Direct memory access**: No serialization overhead

### 4. Trading State (`trading_engine/src/state.rs`)

**Purpose**: Manages trading-specific state (positions, active orders). Derived from ExchangeStateManager via channels. **Does NOT call exchange directly.**

```rust
pub struct TradingState {
    /// Positions by pair (derived from ExchangeStateManager balances)
    pub positions: HashMap<Pair, Position>,
    
    /// Active orders by order ID (only orders we placed)
    pub active_orders: HashMap<OrderId, Order>,
    
    /// Pending orders with timestamps (orders we're tracking)
    pub pending_orders: HashMap<OrderId, u64>,
}

pub struct Position {
    pub symbol: Pair,
    pub quantity: Decimal,
    pub avg_price: Decimal,
}

impl TradingState {
    pub fn new() -> Self;
    
    /// Update position (called when order fills, derived from ExchangeStateManager)
    pub fn update_position(&mut self, symbol: Pair, quantity: Decimal, price: Decimal);
    
    /// Get position for a pair
    pub fn get_position(&self, symbol: &Pair) -> Option<&Position>;
    
    /// Update active order (called when order status changes from ExchangeStateManager)
    pub fn update_order(&mut self, order: Order);
    
    /// Remove order (called when order cancelled/filled)
    pub fn remove_order(&mut self, order_id: &OrderId);
    
    /// Reconcile with ExchangeStateManager (not with exchange directly)
    pub async fn reconcile_with_exchange_state(
        &mut self,
        exchange_state: &ExchangeState,
        pair: &Pair,
    ) -> ReconciliationResult;
}
```

**Key Points**:
- TradingState is **derived from ExchangeStateManager** via channel subscriptions
- TradingState **does NOT call exchange directly** - relies on ExchangeStateManager
- TradingState reconciles with ExchangeStateManager (not with exchange)
- Used by strategies and levels for trading decisions

### 5. Trading State Subscriber (`trading_engine/src/state/subscriber.rs`)

**Purpose**: Subscribes to ExchangeStateManager updates and updates TradingState.

```rust
pub struct ExchangeStateSubscriber {
    exchange_state_rx: tokio::sync::broadcast::Receiver<ExchangeUpdate>,
    trading_state: Arc<RwLock<TradingState>>,
    exchange_state_manager: Arc<ExchangeStateManager>,
}

impl ExchangeStateSubscriber {
    pub fn new(
        exchange_state_rx: tokio::sync::broadcast::Receiver<ExchangeUpdate>,
        trading_state: Arc<RwLock<TradingState>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
    ) -> Self {
        Self {
            exchange_state_rx,
            trading_state,
            exchange_state_manager,
        }
    }
    
    /// Start listening for ExchangeStateManager updates
    pub async fn run(&mut self) -> Result<(), StateError> {
        loop {
            match self.exchange_state_rx.recv().await {
                Ok(ExchangeUpdate::Order(order)) => {
                    // Update TradingState if this is our order
                    let mut state = self.trading_state.write().await;
                    if state.active_orders.contains_key(&order.id) {
                        state.update_order(order);
                    }
                }
                Ok(ExchangeUpdate::Balance(balance)) => {
                    // Derive position from balance if needed
                    // Update TradingState positions
                }
                Ok(ExchangeUpdate::Trade(trade)) => {
                    // Update TradingState if this is our trade
                    // Update positions based on trade
                }
                Err(_) => {
                    // Channel closed or lagged
                    break;
                }
            }
        }
        Ok(())
    }
}
```

### 6. Trading State Reconciler (`trading_engine/src/reconciliation.rs`)

**Purpose**: Reconciles TradingState with ExchangeStateManager (not with exchange directly).

```rust
pub struct PositionReconciler {
    exchange_state_manager: Arc<ExchangeStateManager>,
}

impl PositionReconciler {
    pub fn new(exchange_state_manager: Arc<ExchangeStateManager>) -> Self {
        Self { exchange_state_manager }
    }
    
    /// Reconcile TradingState with ExchangeStateManager
    pub async fn reconcile(
        &self,
        state: &mut TradingState,
        symbol: &Pair,
        current_price: Option<Decimal>,
    ) -> Result<ReconciliationResult, EngineError> {
        // Get balances from ExchangeStateManager (NOT from exchange directly)
        let exchange_state = self.exchange_state_manager.get_state().read().await;
        let balance = exchange_state.get_balance(&symbol.base.to_string());
        
        // Compare with TradingState
        let internal_qty = state.get_position(symbol)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);
        
        let exchange_qty = balance
            .map(|b| b.free + b.used)
            .unwrap_or(Decimal::ZERO);
        
        // Update TradingState if discrepancy
        if (exchange_qty - internal_qty).abs() > threshold {
            state.update_position(symbol.clone(), exchange_qty, current_price.unwrap_or(Decimal::ZERO));
        }
        
        Ok(ReconciliationResult { /* ... */ })
    }
}
```

**Key Points**:
- Reconciles TradingState with ExchangeStateManager (not with exchange)
- Trading Engine does NOT call exchange directly
- Exchange Engine is the single point of contact with exchange

### 7. Market Data State Manager (`market_data_engine/src/exchanges/binance/state/manager.rs`)

**Purpose**: Manages in-memory market data state and publishes updates.

```rust
pub struct MarketDataStateManager {
    state: Arc<RwLock<MarketDataState>>,
    publisher: StatePublisher,
}

impl MarketDataStateManager {
    /// Update orderbook and publish
    pub async fn update_orderbook(&self, pair: Pair, orderbook: Orderbook) -> Result<(), StateError> {
        // 1. Update in-memory state
        // 2. Publish to trading system
        // 3. Publish to UI
        // (MongoDB persistence handled in Proposal 06)
    }
    
    /// Add public trade and publish
    pub async fn add_public_trade(&self, pair: Pair, trade: Trade) -> Result<(), StateError> {
        // 1. Add to in-memory state
        // 2. Publish to trading system
        // 3. Publish to UI
        // (MongoDB persistence handled in Proposal 06)
    }
    
    /// Generate candle from trades (logic ready, unused)
    pub async fn generate_candle(&self, pair: Pair, interval: CandleInterval) -> Result<Candle, StateError>;
    
    /// Get current state
    pub fn get_state(&self) -> Arc<RwLock<MarketDataState>>;
    
    
}
```

### 8. Market Data State (`market_data_engine/src/exchanges/binance/state/state.rs`)

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

### 9. Market Data State Publisher (`market_data_engine/src/exchanges/binance/state/publisher.rs`)

**Purpose**: Publishes market data updates via broadcast channels (in-process, ultra-low latency).

```rust
pub struct ChannelPublisher {
    /// Broadcast channel sender for market data updates
    market_data_update_tx: tokio::sync::broadcast::Sender<MarketDataUpdate>,
}

#[derive(Debug, Clone)]
pub enum MarketDataUpdate {
    Orderbook(Pair, Orderbook),
    Trade(Pair, PublicTrade),
}

impl ChannelPublisher {
    pub fn new(market_data_update_tx: tokio::sync::broadcast::Sender<MarketDataUpdate>) -> Self {
        Self { market_data_update_tx }
    }
    
    /// Publish orderbook update via broadcast channel
    pub async fn publish_orderbook(&self, pair: &Pair, orderbook: &Orderbook) -> Result<(), StateError>;
    
    /// Publish trade update via broadcast channel
    pub async fn publish_trade(&self, pair: &Pair, trade: &PublicTrade) -> Result<(), StateError>;
}
```

**Performance Optimizations**:
- **Broadcast channels**: Ultra-low latency (< 1μs), efficient fan-out
- **No batching needed**: Channels are fast enough without batching

### 5. Risk Engine Cache (`trading_engine/src/risk/cache.rs`)

**Purpose**: Caches risk metrics for fast lookup. Can also call exchange directly for independent verification.

```rust
pub struct RiskMetricsCache {
    /// Cached risk metrics
    metrics: Arc<RwLock<RiskMetrics>>,
    
    /// Exchange state subscriber (receives updates from ExchangeStateManager)
    exchange_state_rx: tokio::sync::broadcast::Receiver<ExchangeUpdate>,
    
    /// Trading client (for independent verification)
    trading_client: Option<Arc<RwLock<dyn TradingClient<OrderRequest>>>>,
}

impl RiskMetricsCache {
    /// Update cache from ExchangeStateManager update
    pub async fn update_from_exchange_state(&self, update: ExchangeUpdate);
    
    /// Verify state correctness by calling exchange directly
    pub async fn verify_state(&self) -> Result<VerificationResult, EngineError> {
        if let Some(client) = &self.trading_client {
            // Call exchange directly for independent verification
            let balances = client.read().await.fetch_balance().await?;
            // Compare with ExchangeStateManager state
            // Alarm if discrepancies detected
        }
        Ok(VerificationResult::Ok)
    }
}
```

**Key Points**:
- Risk Engine caches risk metrics (not a state manager)
- Receives updates from ExchangeStateManager via channels
- Can call exchange directly for independent verification (second layer of security)
- Alarms if discrepancies detected between ExchangeStateManager and direct exchange calls

### 6. Indicator Engine Cache (`indicator_engine/src/cache.rs`)

**Purpose**: Caches calculated indicator values for fast lookup (< 0.1ms).

```rust
pub struct IndicatorCache {
    /// Cached indicator values by pair and indicator name
    cache: Arc<RwLock<HashMap<(Pair, String), IndicatorValue>>>,
    
    /// Market data subscriber (receives updates from MarketDataStateManager)
    market_data_rx: tokio::sync::broadcast::Receiver<MarketDataUpdate>,
}

impl IndicatorCache {
    /// Get cached indicator value
    pub fn get(&self, pair: &Pair, indicator_name: &str) -> Option<IndicatorValue>;
    
    /// Update cache from MarketDataStateManager update
    pub async fn update_from_market_data(&self, update: MarketDataUpdate);
}
```

**Key Points**:
- Indicator Engine caches calculated values (not a state manager)
- Receives updates from MarketDataStateManager via channels
- Fast lookup (< 0.1ms) for trading decisions

### 7. Asset Engine Cache (`asset_engine/src/cache.rs`)

**Purpose**: Caches static asset metadata (symbols, trading rules) with TTL-based refresh.

```rust
pub struct AssetDataCache {
    /// Cached asset data
    cache: Arc<RwLock<AssetData>>,
    
    /// TTL for cache refresh
    ttl: Duration,
    
    /// Last refresh timestamp
    last_refresh: Arc<RwLock<Instant>>,
}

impl AssetDataCache {
    /// Get cached asset data
    pub fn get(&self) -> Option<AssetData>;
    
    /// Refresh cache from REST API (TTL-based)
    pub async fn refresh(&self) -> Result<(), EngineError>;
}
```

**Key Points**:
- Asset Engine caches static metadata (not a state manager)
- TTL-based refresh from REST API
- No state management (static data)

## Integration Points

### 1. Exchange Engine Integration

```rust
// In exchange engine (Binance client)
let (exchange_update_tx, _) = tokio::sync::broadcast::channel(1000);
let state_manager = ExchangeStateManager::new(
    exchange_update_tx,
    trading_client,
    // ... other params
);

// On order update from WebSocket
state_manager.update_order(order).await?;

// On balance update
state_manager.update_balance(balance).await?;
```

### 2. Market Data Engine Integration

```rust
// In market data engine (Binance client)
let (market_data_update_tx, _) = tokio::sync::broadcast::channel(1000);
let state_manager = MarketDataStateManager::new(
    market_data_update_tx,
    // ... other params
);

// On orderbook update
state_manager.update_orderbook(pair, orderbook).await?;

// On public trade
state_manager.add_public_trade(pair, trade).await?;
```

### 3. Trading Engine Integration

```rust
// In trading engine
let (exchange_update_tx, exchange_update_rx) = tokio::sync::broadcast::channel(1000);
let trading_state = Arc::new(RwLock::new(TradingState::new()));
let exchange_state_manager = Arc::new(exchange_state_manager);

// Create subscriber
let mut subscriber = ExchangeStateSubscriber::new(
    exchange_update_rx,
    trading_state.clone(),
    exchange_state_manager.clone(),
);

// Start subscriber
tokio::spawn(async move {
    subscriber.run().await;
});
```

## Publishing Flow

### Exchange State Updates

```
Order Update (from Exchange WebSocket)
    ↓
ExchangeStateManager.update_order()
    ├── Update in-memory state (ExchangeState)
    ├── Publish via Broadcast Channel
    │   ├──→ Trading Engine (ExchangeStateSubscriber)
    │   │   └──→ Updates TradingState
    │   ├──→ Risk Engine (RiskMetricsCache)
    │   │   └──→ Updates risk cache
    │   └──→ TUI (via gRPC aggregation from Trading Engine)
    └── WS vs REST Reconciliation (periodic)
```

**Performance Optimizations**:
- **Broadcast channels**: Ultra-low latency (< 1μs), no batching needed
- **Direct memory access**: No serialization overhead

### Trading State Updates

```
ExchangeStateManager Update (via channel)
    ↓
ExchangeStateSubscriber.run()
    ├── Receive ExchangeUpdate from channel
    ├── Update TradingState
    │   ├── Update positions (derived from balances)
    │   ├── Update active orders (only orders we placed)
    │   └── Update pending orders
    └── Used by strategies/levels for trading decisions
```

**Key Points**:
- TradingState updates are triggered by ExchangeStateManager broadcasts
- Trading Engine does NOT call exchange directly
- TradingState reconciles with ExchangeStateManager (periodic)

### Market Data Updates

```
Orderbook Update (from Exchange WebSocket)
    ↓
MarketDataStateManager.update_orderbook()
    ├── Update in-memory state (MarketDataState)
    ├── Publish via Broadcast Channel
    │   ├──→ Indicator Engine (IndicatorCache)
    │   │   └──→ Updates indicator cache
    │   ├──→ Trading Engine (for level execution)
    │   └──→ TUI (via gRPC aggregation from Trading Engine)
```

**Performance Optimizations**:
- **Broadcast channels**: Ultra-low latency (< 1μs), no batching needed
- **Direct memory access**: No serialization overhead

### Risk Engine Verification

```
Risk Engine (Independent Checker)
    ├── Receives updates from ExchangeStateManager (via channels)
    ├── Updates risk cache
    ├── Periodic Verification (optional)
    │   ├── Call exchange directly (TradingClient.fetch_balance())
    │   ├── Compare with ExchangeStateManager state
    │   └── Alarm if discrepancies detected
    └── Used for risk calculations
```

**Key Points**:
- Risk Engine can call exchange directly for independent verification
- Acts as second layer of security
- Alarms if discrepancies detected

## Reconciliation Architecture

### Overview

Reconciliation ensures state consistency across the system. There are two levels of reconciliation:

1. **Exchange-Level Reconciliation**: ExchangeStateManager reconciles WebSocket state with REST state
2. **Trading-Level Reconciliation**: TradingState reconciles with ExchangeStateManager (not with exchange directly)

### Exchange-Level Reconciliation (ExchangeStateManager)

**Purpose**: Ensure ExchangeStateManager matches exchange authoritative state (REST is authoritative).

**Process**:
1. **Bootstrap**: At startup, fetch balances, open orders, and trades via REST API to initialize state
2. **Periodic**: Periodically (configurable interval) fetch REST data and compare with WebSocket state
3. **Discrepancy Detection**: Detect and log discrepancies between REST and WebSocket data
4. **State Correction**: Update in-memory state to match REST (REST takes priority)
5. **Publish Updates**: Publish corrected state via broadcast channels

**Implementation**:

```rust
// In ExchangeStateManager
impl ExchangeStateManager {
    /// Start periodic WS vs REST reconciliation
    pub async fn start_reconciliation(&self, interval: Duration) {
        let mut timer = tokio::time::interval(interval);
        loop {
            timer.tick().await;
            
            // Fetch from REST (authoritative)
            let rest_balances = self.trading_client.read().await.fetch_balance().await?;
            let rest_orders = self.trading_client.read().await.fetch_open_orders().await?;
            
            // Compare with WebSocket state
            let ws_state = self.state.read().await;
            
            // Reconcile balances
            for rest_balance in &rest_balances {
                let ws_balance = ws_state.get_balance(&rest_balance.symbol);
                if ws_balance != Some(rest_balance) {
                    // Discrepancy detected - update to REST value
                    warn!("Balance discrepancy for {}: WS={:?}, REST={:?}", 
                        rest_balance.symbol, ws_balance, rest_balance);
                    self.update_balance(rest_balance.clone()).await?;
                }
            }
            
            // Reconcile orders (similar logic)
            // ...
        }
    }
}
```

**Key Points**:
- REST is authoritative (exchange's official state)
- WebSocket state is updated to match REST when discrepancies found
- All updates published via broadcast channels after reconciliation

### Trading-Level Reconciliation (TradingState)

**Purpose**: Ensure TradingState matches ExchangeStateManager state.

**Process**:
1. **Periodic**: Periodically (configurable interval) compare TradingState with ExchangeStateManager
2. **Discrepancy Detection**: Detect and log discrepancies
3. **State Correction**: Update TradingState to match ExchangeStateManager
4. **No Direct Exchange Calls**: TradingState does NOT call exchange directly

**Implementation**:

```rust
// In PositionReconciler
impl PositionReconciler {
    /// Reconcile TradingState with ExchangeStateManager
    pub async fn reconcile(
        &self,
        state: &mut TradingState,
        symbol: &Pair,
        current_price: Option<Decimal>,
    ) -> Result<ReconciliationResult, EngineError> {
        // Get balances from ExchangeStateManager (NOT from exchange directly)
        let exchange_state = self.exchange_state_manager.get_state().read().await;
        let balance = exchange_state.get_balance(&symbol.base.to_string());
        
        // Compare with TradingState
        let internal_qty = state.get_position(symbol)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);
        
        let exchange_qty = balance
            .map(|b| b.free + b.used)
            .unwrap_or(Decimal::ZERO);
        
        let discrepancy = exchange_qty - internal_qty;
        
        // Update TradingState if discrepancy exceeds threshold
        if discrepancy.abs() > Decimal::new(1, 6) {
            warn!(
                "Position discrepancy for {}: TradingState={}, ExchangeStateManager={}, diff={}",
                symbol, internal_qty, exchange_qty, discrepancy
            );
            
            // Update TradingState to match ExchangeStateManager
            state.update_position(
                symbol.clone(),
                exchange_qty,
                current_price.unwrap_or(Decimal::ZERO),
            );
            
            Ok(ReconciliationResult {
                symbol: symbol.clone(),
                internal_quantity: internal_qty,
                exchange_quantity: exchange_qty,
                discrepancy,
                reconciled: true,
            })
        } else {
            Ok(ReconciliationResult {
                symbol: symbol.clone(),
                internal_quantity: internal_qty,
                exchange_quantity: exchange_qty,
                discrepancy,
                reconciled: false,
            })
        }
    }
    
    /// Start periodic reconciliation
    pub async fn start_periodic_reconciliation(
        &self,
        trading_state: Arc<RwLock<TradingState>>,
        interval: Duration,
        pairs: &[Pair],
    ) {
        let mut timer = tokio::time::interval(interval);
        loop {
            timer.tick().await;
            
            for pair in pairs {
                let mut state = trading_state.write().await;
                if let Err(e) = self.reconcile(&mut state, pair, None).await {
                    error!("Reconciliation failed for {}: {}", pair, e);
                }
            }
        }
    }
}
```

**Key Points**:
- TradingState reconciles with ExchangeStateManager (NOT with exchange directly)
- Trading Engine does NOT call exchange directly
- Exchange Engine is the single point of contact with exchange

### Risk Engine Verification (Independent Checker)

**Purpose**: Independent verification of state correctness by calling exchange directly.

**Process**:
1. **Periodic**: Periodically (optional, configurable) call exchange directly
2. **Comparison**: Compare ExchangeStateManager state with direct exchange calls
3. **Alarm**: Alarm if discrepancies detected

**Implementation**:

```rust
// In Risk Engine Cache
impl RiskMetricsCache {
    /// Verify state correctness by calling exchange directly
    pub async fn verify_state(&self) -> Result<VerificationResult, EngineError> {
        if let Some(client) = &self.trading_client {
            // Call exchange directly for independent verification
            let direct_balances = client.read().await.fetch_balance().await?;
            
            // Get ExchangeStateManager state
            let exchange_state = self.exchange_state_manager.get_state().read().await;
            let manager_balances = exchange_state.get_balances();
            
            // Compare
            for direct_balance in &direct_balances {
                let manager_balance = manager_balances.iter()
                    .find(|b| b.symbol == direct_balance.symbol);
                
                if manager_balance != Some(direct_balance) {
                    // Discrepancy detected - alarm
                    error!(
                        "State verification failed for {}: ExchangeStateManager={:?}, Direct={:?}",
                        direct_balance.symbol, manager_balance, direct_balance
                    );
                    return Ok(VerificationResult::Discrepancy);
                }
            }
        }
        Ok(VerificationResult::Ok)
    }
}
```

**Key Points**:
- Risk Engine can call exchange directly (independent checker)
- Acts as second layer of security
- Alarms if discrepancies detected

### Detailed Implementation

#### StateReconciler (`exchange_engine/src/state/reconciler.rs`)

**Purpose**: Core reconciliation logic for ExchangeStateManager.

```rust
pub struct StateReconciler {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    reconciliation_interval: Duration,
    enabled: bool,
}

impl StateReconciler {
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        reconciliation_interval_secs: u64,
        enabled: bool,
    ) -> Self {
        Self {
            trading_client,
            reconciliation_interval: Duration::from_secs(reconciliation_interval_secs),
            enabled,
        }
    }

    /// Perform initial bootstrap reconciliation
    pub async fn bootstrap_reconcile(
        &self,
        configured_pairs: &[Pair],
    ) -> Result<ReconciliationResult, EngineError> {
        if !self.enabled {
            return Ok(ReconciliationResult::default());
        }

        // Fetch balances, orders, trades from REST
        let balances = self.trading_client.read().await.fetch_balance().await?;
        let open_orders = self.trading_client.read().await.fetch_open_orders().await?;
        
        let mut all_trades = Vec::new();
        for pair in configured_pairs {
            if let Ok(trades) = self.trading_client.read().await.fetch_trades(Some(pair)).await {
                all_trades.extend(trades);
            }
        }

        Ok(ReconciliationResult {
            balances_fetched: balances.len(),
            orders_fetched: open_orders.len(),
            trades_fetched: all_trades.len(),
            discrepancies: Vec::new(),
        })
    }

    /// Start periodic reconciliation task
    pub fn start_periodic_reconciliation(
        &self,
        configured_pairs: Vec<Pair>,
        state_manager: Arc<dyn StateManager>,
    ) -> tokio::task::JoinHandle<()> {
        let mut interval = tokio::time::interval(self.reconciliation_interval);
        let trading_client = self.trading_client.clone();
        let enabled = self.enabled;

        tokio::spawn(async move {
            if !enabled {
                return;
            }

            loop {
                interval.tick().await;
                
                // Fetch from REST and compare with WebSocket state
                if let Err(e) = Self::reconcile_state(
                    &trading_client,
                    &configured_pairs,
                    &state_manager,
                ).await {
                    error!("Reconciliation failed: {}", e);
                }
            }
        })
    }

    /// Perform reconciliation between REST and WebSocket state
    async fn reconcile_state(
        trading_client: &Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        configured_pairs: &[Pair],
        state_manager: &Arc<dyn StateManager>,
    ) -> Result<ReconciliationResult, EngineError> {
        let mut result = ReconciliationResult::default();

        // Reconcile balances
        let rest_balances = trading_client.read().await.fetch_balance().await?;
        let ws_balances = state_manager.get_balances().await;
        let balance_discrepancies = Self::compare_balances(&rest_balances, &ws_balances);
        result.discrepancies.extend(balance_discrepancies);
        state_manager.update_balances(rest_balances).await?;

        // Reconcile orders
        let rest_orders = trading_client.read().await.fetch_open_orders().await?;
        let ws_orders = state_manager.get_open_orders().await;
        let order_discrepancies = Self::compare_orders(&rest_orders, &ws_orders);
        result.discrepancies.extend(order_discrepancies);
        state_manager.update_open_orders(rest_orders).await?;

        // Reconcile trades
        for pair in configured_pairs {
            if let Ok(rest_trades) = trading_client.read().await.fetch_trades(Some(pair)).await {
                let ws_trades = state_manager.get_trades(pair).await;
                let trade_discrepancies = Self::compare_trades(&rest_trades, &ws_trades);
                result.discrepancies.extend(trade_discrepancies);
                state_manager.update_trades(pair, rest_trades).await?;
            }
        }

        Ok(result)
    }

    /// Compare REST balances with WebSocket balances
    fn compare_balances(
        rest_balances: &[Balance],
        ws_balances: &[Balance],
    ) -> Vec<Discrepancy> {
        // Implementation: compare balances and detect discrepancies
        // Returns Vec<Discrepancy>
    }

    /// Compare REST orders with WebSocket orders
    fn compare_orders(rest_orders: &[Order], ws_orders: &[Order]) -> Vec<Discrepancy> {
        // Implementation: compare orders and detect discrepancies
    }

    /// Compare REST trades with WebSocket trades
    fn compare_trades(rest_trades: &[Trade], ws_trades: &[Trade]) -> Vec<Discrepancy> {
        // Implementation: compare trades and detect discrepancies
    }
}
```

#### Discrepancy Types

```rust
#[derive(Debug, Clone)]
pub enum Discrepancy {
    BalanceMismatch {
        symbol: String,
        rest_free: Decimal,
        rest_used: Decimal,
        ws_free: Decimal,
        ws_used: Decimal,
    },
    BalanceMissingInWs {
        symbol: String,
        rest_free: Decimal,
        rest_used: Decimal,
    },
    OrderMismatch {
        order_id: OrderId,
        rest_status: OrderStatus,
        rest_filled: Decimal,
        ws_status: OrderStatus,
        ws_filled: Decimal,
    },
    OrderMissingInWs {
        order_id: OrderId,
        symbol: Pair,
        status: OrderStatus,
    },
    TradeMissingInWs {
        trade_id: String,
        pair: Pair,
    },
}

#[derive(Debug, Default)]
pub struct ReconciliationResult {
    pub balances_fetched: usize,
    pub orders_fetched: usize,
    pub trades_fetched: usize,
    pub discrepancies: Vec<Discrepancy>,
}

impl ReconciliationResult {
    pub fn has_discrepancies(&self) -> bool {
        !self.discrepancies.is_empty()
    }
}
```

#### StateManager Trait

```rust
#[async_trait]
pub trait StateManager: Send + Sync {
    async fn get_balances(&self) -> Vec<Balance>;
    async fn get_open_orders(&self) -> Vec<Order>;
    async fn get_trades(&self, pair: &Pair) -> Vec<Trade>;
    
    async fn update_balances(&self, balances: Vec<Balance>) -> Result<(), EngineError>;
    async fn update_open_orders(&self, orders: Vec<Order>) -> Result<(), EngineError>;
    async fn update_trades(&self, pair: &Pair, trades: Vec<Trade>) -> Result<(), EngineError>;
}
```

#### TradingClient Trait Updates

```rust
#[async_trait]
pub trait TradingClient<OrderRequest>: Send + Sync + 'static {
    // ... existing methods ...
    
    /// Fetches all open orders for the account (mandatory for reconciliation)
    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError>;
    
    /// Fetches private trade history (mandatory for reconciliation)
    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError>;
}
```

### Reconciliation Configuration

```json
{
  "reconciliation": {
    "enabled": true,
    "exchange_level_interval_secs": 300,  // 5 minutes
    "trading_level_interval_secs": 60,    // 1 minute
    "risk_verification_interval_secs": 600,  // 10 minutes (optional)
    "discrepancy_threshold": "0.000001"   // Minimum discrepancy to trigger update
  }
}
```

## Error Handling

1. **Channel Publishing Failures**
   - Log error
   - Continue processing
   - Channel closure indicates consumer shutdown

2. **Reconciliation Failures**
   - Log error
   - Continue with current state
   - Retry on next reconciliation interval

3. **State Update Failures**
   - Log error
   - Continue with previous state
   - Retry update

## Testing Strategy

1. **Unit Tests**
   - State management
   - Channel publishing logic
   - Reconciliation logic
   - Discrepancy detection

2. **Integration Tests**
   - End-to-end: Exchange → State Manager → Channels → Trading Engine
   - State reconciliation
   - Channel fan-out (multiple consumers)

3. **Mock Tests**
   - Mock TradingClient for reconciliation
   - Mock channel receivers
   - Test error scenarios

## Success Criteria

### State Managers
- [ ] ExchangeStateManager manages raw exchange data (balances, orders, trades)
- [ ] ExchangeStateManager performs WS vs REST reconciliation
- [ ] ExchangeStateManager publishes via broadcast channels
- [ ] MarketDataStateManager manages market data state (orderbooks, public trades)
- [ ] MarketDataStateManager publishes via broadcast channels
- [ ] TradingState manages trading-specific state (positions, active orders)
- [ ] TradingState derived from ExchangeStateManager (via channels)
- [ ] TradingState does NOT call exchange directly
- [ ] TradingState reconciles with ExchangeStateManager (not with exchange)

### Caching
- [ ] Indicator Engine caches indicator values (not a state manager)
- [ ] Risk Engine caches risk metrics (not a state manager)
- [ ] Risk Engine can call exchange directly for verification
- [ ] Asset Engine caches static metadata (not a state manager)

### Publishing
- [ ] All state managers publish updates via broadcast channels
- [ ] Channel fan-out works correctly (multiple consumers)
- [ ] Lag monitoring implemented for slow consumers

## Performance Optimizations

### 1. Broadcast Channels
- **Ultra-low latency**: < 1μs per update
- **No serialization**: Direct memory access
- **Efficient fan-out**: Single broadcast to multiple consumers
- **Lag monitoring**: Track consumer lag to detect slow consumers

### 2. State Management Efficiency
- **In-memory state**: Fast O(1) lookups with HashMap
- **Arc<RwLock<>>**: Efficient concurrent access
- **Minimal allocations**: Reuse buffers where possible

## Summary: State Management Architecture

### State Managers (Full State Management)

1. **ExchangeStateManager** (Exchange Engine):
   - Manages raw exchange data (balances, orders, trades from WebSocket)
   - Performs WS vs REST reconciliation (exchange-level)
   - Publishes via broadcast channels
   - **Single point of contact with exchange** for account data

2. **MarketDataStateManager** (Market Data Engine):
   - Manages market data state (orderbooks, public trades)
   - Publishes via broadcast channels
   - No reconciliation needed (market data is public)

3. **TradingState** (Trading Engine):
   - Manages trading-specific state (positions, active orders)
   - **Derived from ExchangeStateManager** via channel subscriptions
   - **Does NOT call exchange directly** - relies on ExchangeStateManager
   - Reconciles with ExchangeStateManager (not with exchange)

### Caching Only (No State Managers)

4. **Indicator Engine**: Caches calculated indicator values (fast lookup, < 0.1ms)
5. **Risk Engine**: Caches risk metrics (fast lookup), can call exchange directly for verification
6. **Asset Engine**: Caches static asset metadata (TTL-based refresh)

### Key Architectural Principles

- **Exchange Engine is the single point of contact** with exchange for account data
- **Trading Engine relies on ExchangeStateManager** - does NOT call exchange directly
- **TradingState reconciles with ExchangeStateManager** (not with exchange directly)
- **Risk Engine is independent checker** - can call exchange directly to verify state correctness
- **State Managers** (Exchange, Market Data, Trading) manage full state and publish
- **Caching Only** (Indicator, Risk, Asset) - fast lookup, no state management

## Notes

- **State managers handle in-memory state** - orders, balances, orderbooks, trades
- **Trading Engine does NOT call exchange directly** - relies on ExchangeStateManager
- **Broadcast channels for publishing** - ultra-low latency (< 1μs), efficient fan-out
- **Reconciliation ensures state consistency** - Exchange-level and Trading-level
- **Risk Engine is independent checker** - can call exchange directly for verification
- **Performance optimized** - channels (no batching needed), direct memory access
- **MongoDB persistence** - See Proposal 06 for MongoDB persistence and configuration storage

