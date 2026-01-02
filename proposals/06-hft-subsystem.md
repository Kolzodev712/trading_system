# Proposal: HFT Subsystem

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft  
**Prerequisites**: Proposal 05 (Communication Layer) recommended but not required

## Executive Summary

This proposal outlines the design and implementation of a **High-Frequency Trading (HFT) subsystem** as a separate, dedicated binary (`hft_daemon`) optimized for ultra-low latency. The HFT daemon operates as the **data plane** (feed → decision → order in a tight loop) while the existing trading system serves as the **control plane** (TUI, config, observability, audit, non-HFT strategies).

### Key Objectives

1. **Ultra-low latency** - Feed → decision → order in microseconds
2. **Jitter-free hot path** - No locks, no allocations, no async runtime in hot loop
3. **Control/Data separation** - HFT daemon independent from control plane
4. **Safety first** - Kill switches, circuit breakers, risk gates
5. **Observable** - Full telemetry and latency tracking
6. **Scalable** - Start with one venue, support multiple later

### Why Separate HFT Daemon?

- **Latency requirements**: HFT needs microsecond-level latency, incompatible with async runtime overhead
- **Resource isolation**: Dedicated threads, CPU pinning, memory isolation
- **Independence**: HFT continues running even if control plane stalls
- **Different patterns**: Lock-free, allocation-free hot path vs. existing async/stateful patterns
- **Safety**: Can be killed/flattened by control plane without affecting other trading

---

## Architecture Overview

### High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│              Control Plane (Existing System)                 │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Runner, TUI, Config, Observability                 │    │
│  │ - LiveConfig + risk config persistence             │    │
│  │ - UI (TUI/gRPC)                                    │    │
│  │ - Telemetry aggregation + storage                 │    │
│  │ - Non-HFT strategies/levels + DEX                 │    │
│  └────────────────────────────────────────────────────┘    │
│                    │                    │                    │
│                    │ Commands           │ Telemetry          │
│                    │ (low frequency)    │ (high frequency,   │
│                    │                    │  lossy)            │
│                    ▼                    ▼                    │
│         ┌──────────────────────────────────────┐            │
│         │      IPC Interface                    │            │
│         │  (Unix sockets / shared memory)       │            │
│         └──────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Data Plane (HFT Daemon)                         │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Feed Thread → Strategy Thread → Order Thread      │    │
│  │ - Lock-free, allocation-free hot path              │    │
│  │ - Dedicated CPU cores (optional)                  │    │
│  │ - Microsecond-level latency                       │    │
│  └────────────────────────────────────────────────────┘    │
│                    │                    │                    │
│                    ▼                    ▼                    │
│         Exchange Feed          Order Gateway                 │
│         (WebSocket/REST)       (REST/WebSocket)             │
└─────────────────────────────────────────────────────────────┘
```

### Process Model

```
┌─────────────────────────────────────────────────────────────┐
│                    Supervisor/Orchestrator                   │
│  (systemd, docker-compose, or trading_system runner)         │
└─────────────────────────────────────────────────────────────┘
         │                              │
         │                              │
         ▼                              ▼
┌──────────────────┐          ┌──────────────────┐
│ trading_system   │          │ hft_daemon        │
│ (control plane)  │◄──IPC───►│ (data plane)      │
│                  │          │                   │
│ - Config         │          │ - Feed ingest     │
│ - UI/TUI         │          │ - Strategy        │
│ - Observability  │          │ - Order execution │
│ - Non-HFT        │          │ - Risk gate       │
└──────────────────┘          └──────────────────┘
```

**Key Properties**:
- **Independent processes**: HFT daemon can run standalone
- **Control plane can kill/disable**: Via command channel
- **HFT survives control plane stalls**: Continues trading independently
- **Shared types/config**: Via shared crates, not runtime coupling

---

## Phase 1: Module Structure

### 1.1 Crate Organization

**Location**: New workspace member `hft_daemon/`

```
hft_daemon/
├── Cargo.toml
└── src/
    ├── main.rs                    # Entry point
    ├── lib.rs
    ├── feed/
    │   ├── mod.rs
    │   ├── client.rs              # Exchange feed client
    │   ├── parser.rs              # Message parsing
    │   ├── sequencer.rs           # Sequence validation
    │   └── types.rs               # Feed event types
    ├── book/
    │   ├── mod.rs
    │   ├── market_view.rs         # Minimal market view
    │   └── types.rs
    ├── strategy/
    │   ├── mod.rs
    │   ├── engine.rs              # Strategy execution
    │   ├── strategies/
    │   │   ├── mod.rs
    │   │   └── maker.rs           # Simple maker strategy
    │   └── types.rs
    ├── risk/
    │   ├── mod.rs
    │   ├── gate.rs                # Pre-trade risk checks
    │   ├── snapshot.rs            # Risk limits snapshot
    │   └── types.rs
    ├── order/
    │   ├── mod.rs
    │   ├── gateway.rs             # Order sending
    │   ├── state.rs               # Execution state
    │   └── types.rs
    ├── runtime/
    │   ├── mod.rs
    │   ├── threads.rs             # Thread management
    │   ├── queues.rs              # SPSC queues
    │   ├── timing.rs              # Latency timestamps
    │   └── backpressure.rs        # Drop policy
    ├── telemetry/
    │   ├── mod.rs
    │   ├── counters.rs            # Lock-free counters
    │   ├── histograms.rs           # Latency histograms
    │   ├── exporter.rs             # Telemetry export
    │   └── types.rs
    ├── ipc/
    │   ├── mod.rs
    │   ├── command.rs              # Control → HFT commands
    │   ├── telemetry.rs            # HFT → Control telemetry
    │   ├── client.rs               # IPC client
    │   └── server.rs               # IPC server
    └── config/
        ├── mod.rs
        └── hft_config.rs
```

### 1.2 Shared Crates

**Reused from existing system** (via workspace dependencies):

```
libs/
├── trading_core/          # Shared types (Symbol, Pair, Decimal, etc.)
│   └── src/
│       ├── types/         # Reuse: Symbol, Pair, OrderId, etc.
│       └── config/        # Reuse: Config schemas
├── hft_shared/            # NEW: HFT-specific shared types
│   └── src/
│       ├── risk_policy.rs  # Risk limit definitions (not implementation)
│       ├── telemetry.rs    # Shared telemetry event schemas
│       └── ipc_protocol.rs # IPC message definitions
```

**Not reused in hot path**:
- `MarketDataEngine` (too heavy, async-based)
- `IndicatorEngine` (not needed for HFT)
- `RiskEngine` implementation (HFT has own lock-free version)
- gRPC, MongoDB, logging pipelines
- Async runtime

---

## Phase 2: Core Modules

### 2.1 hft_feed

**Location**: `hft_daemon/src/feed/`

**Purpose**: Connect to fastest available feed, parse messages in-place, validate sequencing, emit compact feed events.

**Key Requirements**:
- **Fastest feed**: Binary protocol if available, otherwise optimized WebSocket
- **In-place parsing**: No allocations in hot path
- **Sequence validation**: Detect gaps, handle reconnects
- **Compact events**: Minimal data structures

**Implementation**:
```rust
pub struct FeedClient {
    socket: TcpStream, // Or UnixStream for local feed
    buffer: [u8; 65536], // Pre-allocated buffer
    parser: MessageParser,
    sequencer: Sequencer,
    output_queue: spsc::Sender<FeedEvent>,
}

pub enum FeedEvent {
    TopOfBookUpdate {
        symbol: Symbol,
        bid_price: u64,      // Fixed-point integer
        bid_qty: u64,
        ask_price: u64,
        ask_qty: u64,
        timestamp_ns: u64,
    },
    TradePrint {
        symbol: Symbol,
        price: u64,
        qty: u64,
        side: Side,
        timestamp_ns: u64,
    },
    BookDelta {
        symbol: Symbol,
        updates: Vec<BookLevelUpdate>, // Small, fixed-size
    },
    Heartbeat {
        timestamp_ns: u64,
    },
}

impl FeedClient {
    /// Hot path: read, parse, validate, emit
    pub fn run_feed_loop(&mut self) -> Result<(), FeedError> {
        loop {
            // Read from socket (non-blocking)
            let n = self.socket.read(&mut self.buffer)?;
            
            // Parse messages in-place
            let messages = self.parser.parse(&self.buffer[..n])?;
            
            for msg in messages {
                // Validate sequence
                self.sequencer.validate(&msg)?;
                
                // Convert to compact event
                let event = self.convert_to_event(msg)?;
                
                // Emit to queue (non-blocking, may drop if full)
                let _ = self.output_queue.try_send(event);
            }
        }
    }
}
```

**Key Features**:
- Pre-allocated buffers (no heap allocations)
- Fixed-point integers for prices (avoid Decimal overhead)
- Non-blocking queue sends (drop if full, never block)
- Sequence gap detection
- Reconnection logic (in background thread)

### 2.2 hft_book

**Location**: `hft_daemon/src/book/`

**Purpose**: Maintain minimal market view required by strategy (best bid/ask, spread, last trade, imbalance, microprice).

**Key Requirements**:
- **Constant-time updates**: O(1) operations
- **Fixed memory**: Pre-allocated structures
- **No locks**: Lock-free data structures

**Implementation**:
```rust
pub struct MarketView {
    symbol: Symbol,
    best_bid: u64,      // Price in fixed-point
    best_ask: u64,
    bid_qty: u64,
    ask_qty: u64,
    last_trade_price: u64,
    last_trade_qty: u64,
    spread: u64,        // Pre-computed
    mid_price: u64,     // Pre-computed
    imbalance: i64,     // Bid qty - Ask qty
    microprice: u64,    // Weighted mid
    last_update_ns: u64,
}

impl MarketView {
    /// Update from feed event (O(1), no allocations)
    pub fn update(&mut self, event: &FeedEvent) {
        match event {
            FeedEvent::TopOfBookUpdate { bid_price, bid_qty, ask_price, ask_qty, .. } => {
                self.best_bid = *bid_price;
                self.best_ask = *ask_price;
                self.bid_qty = *bid_qty;
                self.ask_qty = *ask_qty;
                self.spread = self.best_ask - self.best_bid;
                self.mid_price = (self.best_bid + self.best_ask) / 2;
                self.imbalance = self.bid_qty as i64 - self.ask_qty as i64;
                self.microprice = self.compute_microprice();
            }
            FeedEvent::TradePrint { price, qty, .. } => {
                self.last_trade_price = *price;
                self.last_trade_qty = *qty;
            }
            _ => {}
        }
    }
    
    #[inline(always)]
    fn compute_microprice(&self) -> u64 {
        // Weighted mid based on quantities
        // ...
    }
}
```

### 2.3 hft_strategy

**Location**: `hft_daemon/src/strategy/`

**Purpose**: Deterministic decision logic, no heap, no locks.

**Key Requirements**:
- **Deterministic**: Same inputs → same outputs
- **No allocations**: Stack-only
- **No locks**: Lock-free
- **Fast**: Microsecond-level decisions

**Implementation**:
```rust
pub struct StrategyEngine {
    market_view: MarketView,
    internal_state: StrategyState,
    config: StrategyConfig,
}

pub struct StrategyState {
    // Minimal state (stack-allocated)
    position: i64,              // Current position
    inventory: i64,             // Inventory
    last_order_price: u64,      // Last order price
    last_order_time_ns: u64,    // Last order timestamp
}

pub enum OrderIntent {
    None,
    PlaceLimit {
        side: Side,
        price: u64,
        qty: u64,
    },
    Cancel {
        order_id: OrderId,
    },
    Replace {
        order_id: OrderId,
        new_price: u64,
        new_qty: u64,
    },
}

impl StrategyEngine {
    /// Hot path: decision logic (no allocations, no locks)
    #[inline(always)]
    pub fn evaluate(&mut self) -> OrderIntent {
        // Simple maker strategy example:
        // - Place bid at mid - spread/4
        // - Place ask at mid + spread/4
        // - Cancel if spread tightens
        
        let spread = self.market_view.spread;
        let mid = self.market_view.mid_price;
        
        if spread < MIN_SPREAD {
            // Spread too tight, cancel existing orders
            return OrderIntent::Cancel(self.internal_state.last_order_id);
        }
        
        // Place maker orders
        let bid_price = mid - spread / 4;
        let ask_price = mid + spread / 4;
        
        OrderIntent::PlaceLimit {
            side: Side::Buy,
            price: bid_price,
            qty: DEFAULT_QTY,
        }
    }
}
```

**Initial Strategies**:
1. **Simple Maker**: Place limit orders at spread edges
2. **Microstructure**: React to order flow imbalance
3. **Pure market making**: Continuous two-sided quotes

**Not included initially**:
- Indicators (SMA/EMA) unless implemented as inline rolling stats
- Complex TA patterns
- Multi-symbol strategies (Phase 2)

### 2.4 hft_risk_gate

**Location**: `hft_daemon/src/risk/`

**Purpose**: Inline pre-trade checks, O(1), allocation-free.

**Key Requirements**:
- **O(1) checks**: Constant-time risk validation
- **No allocations**: Stack-only
- **Uses risk snapshot**: Pushed from control plane
- **Inline**: Called from hot path

**Implementation**:
```rust
pub struct RiskGate {
    snapshot: RiskSnapshot,  // Immutable, updated atomically
    counters: RiskCounters, // Lock-free counters
}

pub struct RiskSnapshot {
    max_orders_per_symbol: u32,
    max_exposure_per_symbol: Decimal,
    max_notional_per_symbol: Decimal,
    kill_switch: bool,
    throttle_orders_per_sec: u32,
    last_updated_ns: u64,
}

pub struct RiskCounters {
    orders_today: AtomicU32,
    orders_this_second: AtomicU32,
    current_exposure: AtomicI64, // Fixed-point
    last_second_reset_ns: AtomicU64,
}

impl RiskGate {
    /// Hot path: pre-trade risk check (O(1), no allocations)
    #[inline(always)]
    pub fn check(&mut self, intent: &OrderIntent, symbol: &Symbol) -> RiskCheckResult {
        let snapshot = self.snapshot.load(); // Atomic load
        
        // Kill switch check
        if snapshot.kill_switch {
            return RiskCheckResult::Rejected("Kill switch active");
        }
        
        // Throttle check
        let now_ns = time::now_ns();
        if now_ns - self.counters.last_second_reset_ns.load() > 1_000_000_000 {
            self.counters.orders_this_second.store(0);
            self.counters.last_second_reset_ns.store(now_ns);
        }
        
        if self.counters.orders_this_second.load() >= snapshot.throttle_orders_per_sec {
            return RiskCheckResult::Rejected("Throttle limit exceeded");
        }
        
        // Max orders check
        if self.counters.orders_today.load() >= snapshot.max_orders_per_symbol {
            return RiskCheckResult::Rejected("Max orders per symbol exceeded");
        }
        
        // Exposure check (simplified)
        // ...
        
        RiskCheckResult::Approved
    }
}
```

**Risk Checks**:
- Kill switch (external + internal)
- Max orders per symbol
- Max exposure per symbol
- Max notional per symbol
- Throttle (orders/second)
- Circuit breaker state

### 2.5 hft_order_gateway

**Location**: `hft_daemon/src/order/`

**Purpose**: Order protocol encoding, sending, tracking.

**Key Requirements**:
- **Fast encoding**: Pre-allocated buffers
- **Non-blocking sends**: Drop if queue full
- **Minimal state**: Track only open orders
- **Fast cancel/replace**: Optimized path

**Implementation**:
```rust
pub struct OrderGateway {
    client: ExchangeClient,      // Optimized HTTP/WebSocket client
    open_orders: OrderMap,       // Lock-free map
    output_queue: spsc::Sender<OrderEvent>,
}

pub struct OrderMap {
    // Lock-free hash map or array-based (if order IDs are sequential)
    orders: [Option<OpenOrder>; 1024], // Pre-allocated
}

pub struct OpenOrder {
    order_id: OrderId,
    symbol: Symbol,
    side: Side,
    price: u64,
    qty: u64,
    status: OrderStatus,
    submitted_ns: u64,
}

impl OrderGateway {
    /// Hot path: send order (non-blocking)
    pub fn send_order(&mut self, intent: OrderIntent) -> Result<(), OrderError> {
        // Encode order (pre-allocated buffer)
        let encoded = self.encode_order(&intent)?;
        
        // Send (non-blocking)
        self.client.send_non_blocking(encoded)?;
        
        // Track in open orders
        self.open_orders.insert(intent.order_id, OpenOrder { ... });
        
        Ok(())
    }
    
    /// Handle order response (from separate channel)
    pub fn handle_response(&mut self, response: OrderResponse) {
        match response {
            OrderResponse::Ack { order_id, .. } => {
                self.open_orders.update_status(order_id, OrderStatus::Acked);
            }
            OrderResponse::Fill { order_id, price, qty, .. } => {
                self.open_orders.update_status(order_id, OrderStatus::Filled);
                // Emit fill event
                self.output_queue.try_send(OrderEvent::Fill { ... });
            }
            OrderResponse::Reject { order_id, reason, .. } => {
                self.open_orders.remove(order_id);
                // Emit reject event
                self.output_queue.try_send(OrderEvent::Reject { ... });
            }
        }
    }
}
```

### 2.6 hft_runtime

**Location**: `hft_daemon/src/runtime/`

**Purpose**: Threading model, queues, timing, backpressure.

**Key Requirements**:
- **Dedicated threads**: One per stage (feed, strategy, order)
- **SPSC queues**: Single-producer, single-consumer for zero contention
- **CPU pinning**: Optional, for cache locality
- **Time source**: TSC or high-resolution clock
- **Backpressure**: Drop policy when queues full

**Implementation**:
```rust
pub struct HftRuntime {
    feed_thread: JoinHandle<()>,
    strategy_thread: JoinHandle<()>,
    order_thread: JoinHandle<()>,
    queues: Queues,
    timing: TimingSource,
}

pub struct Queues {
    feed_to_strategy: spsc::Queue<FeedEvent>,
    strategy_to_order: spsc::Queue<OrderIntent>,
    order_to_telemetry: spsc::Queue<OrderEvent>,
}

pub struct TimingSource {
    tsc_frequency: u64,
}

impl TimingSource {
    #[inline(always)]
    pub fn now_ns(&self) -> u64 {
        // Use TSC (Time Stamp Counter) or high-res clock
        // ...
    }
}

impl HftRuntime {
    pub fn start(config: HftConfig) -> Result<Self, RuntimeError> {
        let queues = Queues::new(10000, 1000, 1000); // Capacities
        
        // Pin threads to CPU cores (optional)
        if let Some(cores) = config.pinned_cores {
            set_thread_affinity(cores)?;
        }
        
        // Start feed thread
        let feed_thread = spawn_feed_thread(queues.feed_to_strategy.sender(), config)?;
        
        // Start strategy thread
        let strategy_thread = spawn_strategy_thread(
            queues.feed_to_strategy.receiver(),
            queues.strategy_to_order.sender(),
            config,
        )?;
        
        // Start order thread
        let order_thread = spawn_order_thread(
            queues.strategy_to_order.receiver(),
            queues.order_to_telemetry.sender(),
            config,
        )?;
        
        Ok(Self {
            feed_thread,
            strategy_thread,
            order_thread,
            queues,
            timing: TimingSource::new()?,
        })
    }
}

/// Feed thread (hot path)
fn feed_thread_loop(
    mut feed_client: FeedClient,
    queue: spsc::Sender<FeedEvent>,
    timing: TimingSource,
) -> Result<(), FeedError> {
    loop {
        let t0 = timing.now_ns();
        
        // Read and parse
        let event = feed_client.read_and_parse()?;
        
        let t1 = timing.now_ns();
        let parse_latency = t1 - t0;
        
        // Record latency
        telemetry::record_latency("feed.parse", parse_latency);
        
        // Send to queue (non-blocking, may drop)
        if queue.try_send(event).is_err() {
            telemetry::increment_counter("feed.queue_full");
            // Drop event (backpressure)
        }
    }
}

/// Strategy thread (hot path)
fn strategy_thread_loop(
    mut queue_rx: spsc::Receiver<FeedEvent>,
    mut queue_tx: spsc::Sender<OrderIntent>,
    mut strategy: StrategyEngine,
    mut risk_gate: RiskGate,
    timing: TimingSource,
) -> Result<(), StrategyError> {
    loop {
        // Receive market update
        let event = match queue_rx.try_recv() {
            Ok(e) => e,
            Err(_) => {
                // Queue empty, continue
                continue;
            }
        };
        
        let t0 = timing.now_ns();
        
        // Update market view
        strategy.update_market_view(&event);
        
        // Evaluate strategy
        let intent = strategy.evaluate();
        
        // Risk check
        if let OrderIntent::PlaceLimit { .. } = &intent {
            if risk_gate.check(&intent, &strategy.symbol()) != RiskCheckResult::Approved {
                continue; // Rejected, skip
            }
        }
        
        let t1 = timing.now_ns();
        let decision_latency = t1 - t0;
        telemetry::record_latency("strategy.decision", decision_latency);
        
        // Send to order gateway (non-blocking)
        if queue_tx.try_send(intent).is_err() {
            telemetry::increment_counter("strategy.queue_full");
        }
    }
}
```

### 2.7 hft_telemetry

**Location**: `hft_daemon/src/telemetry/`

**Purpose**: Lock-free counters and histograms, exporter thread for structured telemetry.

**Key Requirements**:
- **Lock-free**: Atomic operations only
- **No blocking**: Never block hot path
- **Export thread**: Converts raw counters to structured events
- **Sampling**: Always trace orders, sample market events

**Implementation**:
```rust
pub struct Telemetry {
    counters: LockFreeCounters,
    histograms: LockFreeHistograms,
    exporter: TelemetryExporter,
}

pub struct LockFreeCounters {
    feed_messages: AtomicU64,
    feed_drops: AtomicU64,
    strategy_decisions: AtomicU64,
    orders_sent: AtomicU64,
    orders_filled: AtomicU64,
    orders_rejected: AtomicU64,
    sequence_gaps: AtomicU64,
    reconnects: AtomicU64,
}

pub struct LockFreeHistograms {
    feed_parse_latency: Histogram,      // p50/p95/p99
    strategy_decision_latency: Histogram,
    order_send_latency: Histogram,
    order_round_trip_latency: Histogram,
}

impl Telemetry {
    /// Record latency (hot path, lock-free)
    #[inline(always)]
    pub fn record_latency(&self, name: &str, latency_ns: u64) {
        match name {
            "feed.parse" => self.histograms.feed_parse_latency.record(latency_ns),
            "strategy.decision" => self.histograms.strategy_decision_latency.record(latency_ns),
            // ...
        }
    }
    
    /// Increment counter (hot path, lock-free)
    #[inline(always)]
    pub fn increment_counter(&self, name: &str) {
        match name {
            "feed.messages" => self.counters.feed_messages.fetch_add(1, Ordering::Relaxed),
            "feed.drops" => self.counters.feed_drops.fetch_add(1, Ordering::Relaxed),
            // ...
        }
    }
}

/// Exporter thread (background, not hot path)
pub struct TelemetryExporter {
    counters: Arc<LockFreeCounters>,
    histograms: Arc<LockFreeHistograms>,
    output: spsc::Sender<TelemetryEvent>,
}

impl TelemetryExporter {
    pub fn run_export_loop(&self) {
        let mut interval = interval(Duration::from_millis(100)); // 10 Hz
        
        loop {
            interval.tick().await;
            
            // Snapshot counters
            let snapshot = self.counters.snapshot();
            
            // Snapshot histograms (p50/p95/p99)
            let latencies = self.histograms.snapshot();
            
            // Emit structured telemetry event
            let event = TelemetryEvent {
                timestamp_ns: time::now_ns(),
                counters: snapshot,
                latencies,
                // ...
            };
            
            // Send to IPC (non-blocking)
            let _ = self.output.try_send(event);
        }
    }
}
```

---

## Phase 3: IPC Interface

### 3.1 Control → HFT Commands

**Location**: `hft_daemon/src/ipc/command.rs`

**Purpose**: Low-frequency commands from control plane to HFT daemon.

**Transport**: Unix domain sockets or shared memory queue.

**Commands**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HftCommand {
    EnableTrading {
        enabled: bool,
    },
    UpdateParams {
        strategy_id: String,
        params: serde_json::Value,
    },
    UpdateRiskSnapshot {
        snapshot: RiskSnapshot,
    },
    UpdateUniverse {
        symbols: Vec<Symbol>,
    },
    Flatten {
        symbol: Option<Symbol>, // None = all symbols
    },
    Shutdown,
}

pub struct CommandChannel {
    socket: UnixStream,
    buffer: [u8; 4096],
}

impl CommandChannel {
    /// Receive command (non-blocking)
    pub fn receive(&mut self) -> Result<Option<HftCommand>, IpcError> {
        // Read from Unix socket
        // Deserialize command
        // Validate version
        // ...
    }
}
```

### 3.2 HFT → Control Telemetry

**Location**: `hft_daemon/src/ipc/telemetry.rs`

**Purpose**: High-frequency, lossy telemetry from HFT to control plane.

**Transport**: Lossy ring buffer to exporter thread (never block hot path).

**Telemetry Events**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryEvent {
    OrderAck {
        order_id: OrderId,
        timestamp_ns: u64,
    },
    Fill {
        order_id: OrderId,
        price: u64,
        qty: u64,
        timestamp_ns: u64,
    },
    Reject {
        order_id: OrderId,
        reason: String,
        timestamp_ns: u64,
    },
    PositionSnapshot {
        symbol: Symbol,
        position: i64,
        inventory: i64,
        timestamp_ns: u64,
    },
    PnLSnapshot {
        realized_pnl: i64,
        unrealized_pnl: i64,
        timestamp_ns: u64,
    },
    LatencyStats {
        feed_parse_p50: u64,
        feed_parse_p95: u64,
        feed_parse_p99: u64,
        strategy_decision_p50: u64,
        strategy_decision_p95: u64,
        strategy_decision_p99: u64,
        order_round_trip_p50: u64,
        order_round_trip_p95: u64,
        order_round_trip_p99: u64,
        timestamp_ns: u64,
    },
    Health {
        feed_lag_ms: u64,
        seq_gaps: u64,
        reconnects: u64,
        drop_count: u64,
        timestamp_ns: u64,
    },
}

pub struct TelemetryChannel {
    ring_buffer: RingBuffer<TelemetryEvent>, // Lock-free, lossy
}

impl TelemetryChannel {
    /// Send telemetry (non-blocking, may drop if full)
    pub fn send(&self, event: TelemetryEvent) {
        // Try to write to ring buffer
        // If full, drop (never block)
        // ...
    }
}
```

---

## Phase 4: Safety Controls

### 4.1 Kill Switch

**Implementation**:
```rust
pub struct KillSwitch {
    external: AtomicBool,  // Set by control plane
    internal: AtomicBool,  // Set by circuit breakers
}

impl KillSwitch {
    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.external.load(Ordering::Acquire) || 
        self.internal.load(Ordering::Acquire)
    }
    
    pub fn check_before_trade(&self) -> Result<(), KillSwitchError> {
        if self.is_active() {
            return Err(KillSwitchError::Active);
        }
        Ok(())
    }
}
```

### 4.2 Circuit Breakers

**Triggers**:
- Repeated rejects (N rejects in M seconds)
- Feed lag (lag > threshold)
- Sequence gaps (gaps > threshold)
- Excessive order rate (rate > throttle limit)

**Implementation**:
```rust
pub struct CircuitBreaker {
    reject_count: AtomicU32,
    feed_lag_ns: AtomicU64,
    seq_gaps: AtomicU64,
    order_rate: AtomicU32,
    tripped: AtomicBool,
}

impl CircuitBreaker {
    pub fn check(&self) -> Result<(), CircuitBreakerError> {
        if self.tripped.load(Ordering::Acquire) {
            return Err(CircuitBreakerError::Tripped);
        }
        
        // Check conditions
        if self.reject_count.load(Ordering::Relaxed) > MAX_REJECTS {
            self.tripped.store(true, Ordering::Release);
            return Err(CircuitBreakerError::TooManyRejects);
        }
        
        if self.feed_lag_ns.load(Ordering::Relaxed) > MAX_FEED_LAG_NS {
            self.tripped.store(true, Ordering::Release);
            return Err(CircuitBreakerError::FeedLag);
        }
        
        // ... other checks
        
        Ok(())
    }
}
```

### 4.3 Flatten Command

**Implementation**:
```rust
impl HftRuntime {
    pub fn handle_flatten(&mut self, symbol: Option<Symbol>) {
        // Cancel all open orders for symbol (or all)
        // Close positions
        // Stop new orders
        // ...
    }
}
```

### 4.4 No-Trade Mode

**Implementation**:
```rust
pub struct NoTradeMode {
    enabled: AtomicBool,
}

impl NoTradeMode {
    /// Run full simulation pipeline without sending orders
    pub fn simulate(&self, intent: OrderIntent) -> SimulatedFill {
        // Simulate fill based on market view
        // Update internal state
        // Emit telemetry as if real
        // ...
    }
}
```

---

## Phase 5: Integration with Control Plane

### 5.1 IPC Server (Control Plane Side)

**Location**: `src/runner/src/hft/` (new module)

**Purpose**: Control plane interface to HFT daemon.

**Implementation**:
```rust
pub struct HftController {
    command_channel: UnixStream,
    telemetry_rx: Receiver<TelemetryEvent>,
}

impl HftController {
    /// Send command to HFT daemon
    pub async fn send_command(&mut self, cmd: HftCommand) -> Result<(), HftError> {
        // Serialize and send via Unix socket
        // ...
    }
    
    /// Receive telemetry from HFT daemon
    pub async fn receive_telemetry(&mut self) -> Result<TelemetryEvent, HftError> {
        // Read from telemetry channel
        // ...
    }
    
    /// Get HFT status (for TUI)
    pub async fn get_status(&self) -> HftStatus {
        // Query HFT daemon for current status
        // ...
    }
}
```

### 5.2 TUI Integration

**Location**: `cli/src/hft/` (new module)

**Purpose**: Display HFT status in TUI.

**Display**:
- Feed lag (ms)
- Decision latency (µs/ms)
- Order round trip (ms)
- Reject reasons distribution
- PnL and inventory
- Why trading is disabled (which breaker tripped)

---

## Implementation Phases

### Phase 1: Skeleton + Telemetry (Week 1-2)
- [ ] Create `hft_daemon` crate structure
- [ ] Implement IPC command/telemetry channels
- [ ] Implement feed ingest (mock feed first)
- [ ] Implement book view (minimal market view)
- [ ] Implement telemetry system (counters, histograms)
- [ ] Test IPC communication with control plane
- [ ] Measure latency/lag (no trading yet)

### Phase 2: Paper Execution (Week 3-4)
- [ ] Implement strategy engine (simple maker strategy)
- [ ] Implement risk gate
- [ ] Implement order gateway (simulate fills or test venue)
- [ ] Verify risk gate and state transitions
- [ ] Test full pipeline in paper mode
- [ ] Measure end-to-end latency

### Phase 3: Limited Live (Week 5-6)
- [ ] Connect to real exchange feed
- [ ] Connect to real order gateway
- [ ] Single symbol trading
- [ ] Strict caps (max orders, max exposure)
- [ ] Hard circuit breaker
- [ ] Monitor stability
- [ ] Measure real-world latency

### Phase 4: Scale (Week 7-8)
- [ ] Support multiple symbols
- [ ] Support multiple strategies
- [ ] Optimize hot path (CPU pinning, cache optimization)
- [ ] Add more sophisticated strategies
- [ ] Performance tuning

---

## Success Criteria

### Latency Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Feed parse latency | < 10µs | p99 |
| Strategy decision latency | < 50µs | p99 |
| Order send latency | < 100µs | p99 |
| Order round trip | < 10ms | p99 |
| Feed lag | < 100ms | p99 |

### Observability

Must be able to answer in TUI at any time:
- [ ] Feed lag (ms)
- [ ] Decision latency (µs/ms)
- [ ] Order round trip (ms)
- [ ] Reject reasons distribution
- [ ] PnL and inventory
- [ ] Why trading is disabled (which breaker tripped)

### Safety

- [ ] Kill switch works (external + internal)
- [ ] Circuit breakers trip correctly
- [ ] Flatten command works
- [ ] No-trade mode works
- [ ] Risk gates enforce limits

### Performance

- [ ] Hot path has zero allocations
- [ ] Hot path has zero locks
- [ ] Queue depths stay bounded
- [ ] No blocking in hot path
- [ ] CPU utilization reasonable

---

## Configuration

### HFT Daemon Config

**Location**: `config/hft.json`

```json
{
  "hft": {
    "feed": {
      "url": "wss://stream.binance.com/ws",
      "symbols": ["BTCUSDT", "ETHUSDT"],
      "buffer_size": 65536
    },
    "strategy": {
      "type": "maker",
      "symbol": "BTCUSDT",
      "spread_bps": 5,
      "default_qty": 0.001
    },
    "risk": {
      "max_orders_per_symbol": 100,
      "max_exposure_per_symbol": 1000.0,
      "throttle_orders_per_sec": 10
    },
    "runtime": {
      "pinned_cores": [2, 3, 4],
      "queue_capacities": {
        "feed_to_strategy": 10000,
        "strategy_to_order": 1000
      }
    },
    "telemetry": {
      "export_interval_ms": 100,
      "sample_market_events": 100
    },
    "safety": {
      "kill_switch": false,
      "circuit_breaker": {
        "max_rejects_per_minute": 10,
        "max_feed_lag_ms": 1000,
        "max_seq_gaps": 5
      }
    }
  }
}
```

---

## Dependencies

### Rust Crates

```toml
[dependencies]
# Core
tokio = { version = "1.35", features = ["rt", "net", "time"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Networking
tokio-tungstenite = "0.21"
reqwest = "0.11"

# Lock-free data structures
crossbeam = "0.8"
lockfree = "0.5"

# Timing
rdtsc = "0.3"  # TSC (Time Stamp Counter)

# IPC
interprocess = "1.1"  # Unix domain sockets

# Shared crates (workspace)
trading_core = { path = "../../libs/trading_core" }
hft_shared = { path = "../../libs/hft_shared" }
```

---

## Future Enhancements

1. **Multiple Venues**: Support multiple exchanges simultaneously
2. **Advanced Strategies**: More sophisticated microstructure strategies
3. **Dedicated Infrastructure**: Colocation, dedicated network, FPGA acceleration
4. **Multi-Symbol Strategies**: Cross-symbol arbitrage, spread trading
5. **Machine Learning**: On-device ML models for decision making
6. **Hardware Acceleration**: GPU/FPGA for specific operations

---

**Next Steps**: Review proposal, approve architecture, begin Phase 1 (Skeleton + Telemetry).

