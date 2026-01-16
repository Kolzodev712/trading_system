# Proposal: Communication Layer & Hexagonal Architecture

**Version**: 2.0  
**Date**: December 2024  
**Status**: Revised (Based on Low-Latency Feedback)  
**Prerequisites**: None (can be implemented independently)

## Executive Summary

This proposal outlines the transformation of the trading system to a **UI-agnostic, event-driven architecture** using **Hexagonal (Ports & Adapters) principles**. The core runtime will be completely decoupled from any UI, allowing unlimited pluggable consumers (TUI, web dashboards, analytics, automation, HFT daemon) without modifying the core system.

### Key Objectives

1. **Decouple core from UI** - Core runtime has zero knowledge of gRPC, databases, or UI frameworks
2. **Formalize Event/Command buses** - Internal communication via stable, well-defined contracts
3. **Introduce External Gateway** - Single boundary for all external communication
4. **Support multiple transports** - gRPC, Pub/Sub, IPC simultaneously
5. **Maintain ultra-low latency** - Hot path channels optimized for <1μs, cold path <10μs
6. **Enable horizontal scaling** - Multiple consumers, multiple strategies, no tight coupling
7. **Allocation discipline** - Minimize heap allocations on hot path, use Arc for cold path

### Why This Architecture?

- **Future-proof**: Add new UIs/consumers without touching core
- **Scalable**: Multiple dashboards, analytics pipelines, automation tools
- **Testable**: Core logic testable without I/O concerns
- **Flexible**: Support HFT daemon, web UI, mobile app, all simultaneously
- **Maintainable**: Clear boundaries, single responsibility

---

## Current Architecture Analysis

### Current State

```
┌─────────────────────────────────────────────────────────────┐
│                    Core Engines                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Exchange     │  │ Market Data  │  │ Trading      │      │
│  │ Engine       │  │ Engine       │  │ Engine       │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│         │                │                    │              │
│         └────────────────┴────────────────────┘              │
│                    Broadcast Channels                         │
│                    (< 1μs latency)                            │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Runner (Orchestrator)                     │
│  ┌────────────────────────────────────────────────────┐    │
│  │ gRPC Server (embedded)                            │    │
│  │  - Direct access to state managers                │    │
│  │  - Aggregates data for TUI                        │    │
│  │  - Handles commands                               │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
                    TUI (gRPC Client)
```

### Issues with Current Architecture

1. **gRPC embedded in Runner** - Core knows about gRPC
2. **Direct state access** - gRPC service directly accesses state managers
3. **Single consumer** - Only TUI can connect
4. **Tight coupling** - Adding web UI requires modifying core
5. **No event sourcing** - Can't replay events for new consumers
6. **No multi-transport** - Can't add Pub/Sub, IPC, etc.

---

## Target Architecture

### High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│                    Core Runtime (UI-Agnostic)                │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Hot Lane (Market Data)                           │    │
│  │  - SPSC/Bounded MPSC channels                    │    │
│  │  - Compact structs (IDs + deltas)                 │    │
│  │  - Arc<Bytes> or pre-allocated buffers            │    │
│  │  - < 1μs latency (allocation-free)               │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Cold Lane (Orders/Fills/Positions/Risk/Health)   │    │
│  │  - broadcast::Sender<Arc<Event>>                  │    │
│  │  - Lean events (IDs + key fields + timestamps)   │    │
│  │  - < 10μs latency                                 │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Command Router (Single-Consumer)                  │    │
│  │  - Routes to specific handlers                    │    │
│  │  - Includes oneshot::Sender for replies          │    │
│  │  - Correlation IDs (u64)                         │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              External Gateway Layer                           │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Event Gateway Router                              │    │
│  │  - Subscribes to Hot/Cold lanes                   │    │
│  │  - Converts to DTP (once)                         │    │
│  │  - try_send to per-transport bounded queues       │    │
│  │  - Non-blocking (never stalls on slow transport)   │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Per-Transport Workers                             │    │
│  │  - Each transport has own task + queue           │    │
│  │  - Pulls from queue, sends to network            │    │
│  │  - Drop/sample policy when queue full             │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Command Gateway                                   │    │
│  │  - Receives external commands                     │    │
│  │  - Authenticates, validates                       │    │
│  │  - Routes to Command Router                       │    │
│  │  - Awaits reply via oneshot                       │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
         │              │              │              │
         ▼              ▼              ▼              ▼
    ┌────────┐    ┌────────┐    ┌────────┐    ┌────────┐
    │  gRPC  │    │ PubSub │    │  IPC   │    │  HTTP  │
    │ (DTP)  │    │ (DTP)  │    │ (DTP)  │    │ (DTP)  │
    └────────┘    └────────┘    └────────┘    └────────┘
```

### Key Principles

1. **Core is UI-agnostic** - No gRPC, no HTTP, no UI frameworks in core
2. **Internal buses never exposed** - External systems never see internal channels
3. **Gateway is the boundary** - Only place where serialization, networking, auth exist
4. **Multiple transports** - All can run simultaneously
5. **Event sourcing ready** - Events can be persisted for replay
6. **Hot/Cold lane separation** - Market data on hot path (allocation-free), state changes on cold path (Arc-wrapped)
7. **Allocation discipline** - Hot path avoids heap allocations, cold path uses Arc to share
8. **Non-blocking gateway** - Per-transport queues prevent head-of-line blocking
9. **Single canonical encoding** - Protobuf for DTP payload (JSON only as derived stream)

---

## Phase 1: Formalize Event Bus (Hot/Cold Lane Split)

### 1.1 Hot Lane Event Types (Market Data)

**Location**: `libs/trading_core/src/events/hot.rs` (new module)

**Purpose**: Ultra-low latency market data events with minimal allocations.

**Key Design**: Compact structs with IDs and deltas, not full objects. Use references to pre-allocated buffers when possible.

**Implementation**:
```rust
use bytes::Bytes;
use std::sync::Arc;

/// Hot lane market data event (allocation-light, compact)
#[derive(Debug, Clone)]
pub enum HotEvent {
    /// Orderbook snapshot (compact representation)
    /// For high-frequency updates, consider using Arc<Bytes> with pre-serialized data
    Orderbook {
        pair_id: u32,  // Numeric ID, not String
        best_bid: Decimal,
        best_ask: Decimal,
        bid_qty: Decimal,
        ask_qty: Decimal,
        timestamp: u64,
        // Full orderbook only if needed (rare) - use Arc<Bytes> for pre-serialized
        full_snapshot: Option<Arc<Bytes>>,
    },
    /// Trade tick (minimal data)
    Trade {
        pair_id: u32,
        price: Decimal,
        quantity: Decimal,
        side: Side,
        timestamp: u64,
    },
    /// Mark price (futures)
    MarkPrice {
        pair_id: u32,
        price: Decimal,
        timestamp: u64,
    },
    /// Funding rate (futures)
    FundingRate {
        pair_id: u32,
        rate: Decimal,
        timestamp: u64,
    },
}
```

### 1.2 Cold Lane Event Types (State Changes)

**Location**: `libs/trading_core/src/events/cold.rs` (new module)

**Purpose**: State change events (orders, fills, positions, risk). Lean payloads with IDs and deltas.

**Key Design**: Events carry identifiers + key fields, not full objects. Consumers query state stores for full state if needed.

**Implementation**:
```rust
use std::sync::Arc;

/// Cold lane event (state changes, wrapped in Arc for sharing)
#[derive(Debug, Clone)]
pub enum ColdEvent {
    // Exchange Events (IDs + deltas only)
    Order(OrderEvent),
    Fill(FillEvent),
    Position(PositionEvent),
    Balance(BalanceEvent),
    
    // Trading Events
    Strategy(StrategyEvent),
    Level(LevelEvent),
    
    // Risk Events
    Risk(RiskEvent),
    
    // System Events
    Config(ConfigEvent),
    Health(HealthEvent),
    Error(ErrorEvent),
}

/// Order event (lean - IDs + key fields, not full Order object)
#[derive(Debug, Clone)]
pub enum OrderEvent {
    Placed {
        order_id: OrderId,
        pair_id: u32,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        order_type: OrderType,
        timestamp: u64,
        // Full order only if needed - query state store instead
    },
    Updated {
        order_id: OrderId,
        status: OrderStatus,
        filled_amount: Decimal,
        timestamp: u64,
    },
    Filled {
        order_id: OrderId,
        fill_price: Decimal,
        fill_quantity: Decimal,
        timestamp: u64,
    },
    Cancelled {
        order_id: OrderId,
        reason_code: u16,  // Numeric code, not String
        timestamp: u64,
    },
}

/// Fill event (lean)
#[derive(Debug, Clone)]
pub struct FillEvent {
    pub order_id: OrderId,
    pub pair_id: u32,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
    pub fee: Decimal,
    pub timestamp: u64,
}

/// Position event (lean)
#[derive(Debug, Clone)]
pub enum PositionEvent {
    Opened {
        pair_id: u32,
        quantity: Decimal,
        entry_price: Decimal,
        timestamp: u64,
    },
    Updated {
        pair_id: u32,
        quantity: Decimal,
        avg_price: Decimal,
        unrealized_pnl: Decimal,
        timestamp: u64,
    },
    Closed {
        pair_id: u32,
        realized_pnl: Decimal,
        timestamp: u64,
    },
}

/// Balance event (lean)
#[derive(Debug, Clone)]
pub struct BalanceEvent {
    pub asset_id: u16,  // Numeric ID
    pub free: Decimal,
    pub used: Decimal,
    pub total: Decimal,
    pub timestamp: u64,
}

/// Strategy event (lean)
#[derive(Debug, Clone)]
pub enum StrategyEvent {
    Started {
        strategy_id: u64,  // Numeric ID
        pair_id: u32,
        timestamp: u64,
    },
    Stopped {
        strategy_id: u64,
        reason_code: u16,  // Numeric code
        timestamp: u64,
    },
    Signal {
        strategy_id: u64,
        signal_type: u8,  // Numeric signal type
        timestamp: u64,
    },
}

/// Level event (lean)
#[derive(Debug, Clone)]
pub enum LevelEvent {
    Created { level_id: u64, pair_id: u32, timestamp: u64 },
    Updated { level_id: u64, timestamp: u64 },
    Deleted { level_id: u64, timestamp: u64 },
    Filled { level_id: u64, timestamp: u64 },
}

/// Risk event (lean)
#[derive(Debug, Clone)]
pub enum RiskEvent {
    LimitExceeded {
        limit_type: u8,  // Numeric type
        current: Decimal,
        limit: Decimal,
        timestamp: u64,
    },
    CircuitBreaker {
        action: u8,  // Numeric action code
        reason_code: u16,
        timestamp: u64,
    },
}

/// Config event
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    Updated { path_hash: u64, timestamp: u64 },  // Hash path, not String
    Reloaded { timestamp: u64 },
}

/// Health event
#[derive(Debug, Clone)]
pub struct HealthEvent {
    pub component_id: u8,  // Numeric component ID
    pub status: HealthStatus,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Error event (lean)
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    pub component_id: u8,
    pub error_code: u16,  // Numeric error code
    pub severity: ErrorSeverity,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    Warning,
    Error,
    Critical,
}
```


### 1.3 Hot Lane Bus (Market Data)

**Location**: `libs/trading_core/src/events/hot_bus.rs` (new file)

**Purpose**: Ultra-low latency channel for market data. Uses SPSC ring buffer or bounded MPSC.

**Key Design**: Single-consumer hot lane. Receiver is created at construction and passed directly to the consumer (typically TradingEngine). No trait needed.

**Implementation**:
```rust
use tokio::sync::mpsc;

/// Hot lane sender (multiple producers, single consumer)
pub struct HotEventTx {
    tx: mpsc::Sender<HotEvent>,
}

impl HotEventTx {
    /// Publish a hot event (non-blocking, may drop if full)
    pub fn try_publish(&self, event: HotEvent) -> Result<(), HotEventBusError> {
        self.tx.try_send(event)
            .map_err(|_| HotEventBusError::ChannelFull)?;
        Ok(())
    }
}

/// Hot lane receiver (single consumer)
pub type HotEventRx = mpsc::Receiver<HotEvent>;

/// Create hot lane channel (returns sender and receiver)
pub fn hot_event_channel(capacity: usize) -> (HotEventTx, HotEventRx) {
    let (tx, rx) = mpsc::channel(capacity);
    (HotEventTx { tx }, rx)
}

#[derive(Debug)]
pub enum HotEventBusError {
    ChannelFull,
    ChannelClosed,
}
```

**Usage**: Receiver is passed directly to the consumer (e.g., TradingEngine). No trait abstraction needed for single-consumer pattern.

**Note**: For ultra-low latency, consider implementing SPSC ring buffer using `ringbuf` crate or similar.

### 1.4 Cold Lane Bus (State Changes)

**Location**: `libs/trading_core/src/events/cold_bus.rs` (new file)

**Purpose**: Broadcast channel for state change events. Uses Arc to avoid clones.

**Implementation**:
```rust
use tokio::sync::broadcast;
use std::sync::Arc;

/// Cold lane bus trait (for state changes)
pub trait ColdEventBus: Send + Sync + 'static {
    /// Publish a cold event (wrapped in Arc for sharing)
    fn publish(&self, event: Arc<ColdEvent>) -> Result<(), ColdEventBusError>;
    
    /// Subscribe to all cold events
    fn subscribe(&self) -> broadcast::Receiver<Arc<ColdEvent>>;
    
    /// Subscribe to specific event types
    fn subscribe_filtered<F>(&self, filter: F) -> FilteredReceiver<Arc<ColdEvent>>
    where
        F: Fn(&ColdEvent) -> bool + Send + Sync + 'static;
}

/// Broadcast implementation using Arc-wrapped events
pub struct BroadcastColdEventBus {
    tx: broadcast::Sender<Arc<ColdEvent>>,
}

impl BroadcastColdEventBus {
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<Arc<ColdEvent>>) {
        let (tx, rx) = broadcast::channel(capacity);
        (Self { tx }, rx)
    }
}

impl ColdEventBus for BroadcastColdEventBus {
    fn publish(&self, event: Arc<ColdEvent>) -> Result<(), ColdEventBusError> {
        self.tx.send(event).map_err(|_| ColdEventBusError::ChannelClosed)?;
        Ok(())
    }
    
    fn subscribe(&self) -> broadcast::Receiver<Arc<ColdEvent>> {
        self.tx.subscribe()
    }
    
    fn subscribe_filtered<F>(&self, filter: F) -> FilteredReceiver<Arc<ColdEvent>>
    where
        F: Fn(&ColdEvent) -> bool + Send + Sync + 'static,
    {
        // Implementation filters events
        // ...
    }
}

#[derive(Debug)]
pub enum ColdEventBusError {
    ChannelClosed,
    CapacityExceeded,
}
```

### 1.5 Update Existing Publishers

**Location**: `src/exchange_engine/src/state/publisher.rs` (extend existing)

**Changes**: Publish to cold lane bus with lean events (IDs + deltas, not full objects).

```rust
use std::sync::Arc;
use trading_core::events::cold::{ColdEvent, OrderEvent, FillEvent, BalanceEvent};

impl ExchangeStatePublisher {
    // ... existing methods ...
    
    /// Publish order event (lean - IDs + key fields only)
    pub fn publish_order_placed(&self, order: &Order) -> Result<(), EngineError> {
        use trading_core::events::cold::ColdEvent;
        
        // Convert pair to numeric ID (use registry)
        let pair_id = self.pair_registry.get_id(&order.symbol)?;
        
        let event = Arc::new(ColdEvent::Order(OrderEvent::Placed {
            order_id: order.order_id.clone(),
            pair_id,
            side: order.side,
            price: order.price,
            quantity: order.amount,
            order_type: order.order_type,
            timestamp: order.internal_created_at,
        }));
        
        self.cold_event_bus.publish(event)?;
        Ok(())
    }
    
    /// Publish fill event (lean)
    pub fn publish_fill(&self, fill: &Fill) -> Result<(), EngineError> {
        let pair_id = self.pair_registry.get_id(&fill.pair)?;
        
        let event = Arc::new(ColdEvent::Fill(FillEvent {
            order_id: fill.order_id.clone(),
            pair_id,
            side: fill.side,
            price: fill.price,
            quantity: fill.quantity,
            fee: fill.fee,
            timestamp: fill.timestamp,
        }));
        
        self.cold_event_bus.publish(event)?;
        Ok(())
    }
}
```

**Location**: `src/market_data_engine/src/state/publisher.rs` (extend existing)

**Changes**: Publish to hot lane bus with compact market data events.

```rust
use trading_core::events::hot::{HotEvent, HotEventTx};

impl MarketDataStatePublisher {
    /// Publish orderbook update (compact - best bid/ask only)
    pub fn publish_orderbook(&self, orderbook: &Orderbook) -> Result<(), EngineError> {
        let pair_id = self.pair_registry.get_id(&orderbook.pair)?;
        
        let best_bid = orderbook.bids.first().map(|(p, _)| *p).unwrap_or_default();
        let best_ask = orderbook.asks.first().map(|(p, _)| *p).unwrap_or_default();
        let bid_qty = orderbook.bids.first().map(|(_, q)| *q).unwrap_or_default();
        let ask_qty = orderbook.asks.first().map(|(_, q)| *q).unwrap_or_default();
        
        let event = HotEvent::Orderbook {
            pair_id,
            best_bid,
            best_ask,
            bid_qty,
            ask_qty,
            timestamp: orderbook.timestamp,
            full_snapshot: None,  // Only include if specifically needed
        };
        
        self.hot_event_tx.try_publish(event)?;
        Ok(())
    }
}
```

---

## Phase 2: Formalize Command Router (Single-Consumer)

### 2.1 Command Type Definitions

**Location**: `libs/trading_core/src/commands/mod.rs` (new module)

**Purpose**: Define command types with correlation IDs and reply channels for request/response.

**Key Design**: Commands are single-consumer, routed to specific handlers. Include oneshot::Sender for replies.

**Implementation**:
```rust
use tokio::sync::oneshot;

/// Command with correlation ID and reply channel
#[derive(Debug)]
pub struct Command {
    /// Correlation ID (u64, 0 = none)
    pub correlation_id: u64,
    
    /// Command payload
    pub payload: CommandPayload,
    
    /// Reply channel (None for fire-and-forget)
    pub reply: Option<oneshot::Sender<CommandResult>>,
}

/// Command payload types
#[derive(Debug, Clone)]
pub enum CommandPayload {
    // Trading Commands
    Trade(TradeCommand),
    
    // Strategy Commands
    Strategy(StrategyCommand),
    
    // Level Commands
    Level(LevelCommand),
    
    // Config Commands
    Config(ConfigCommand),
    
    // Control Commands
    Control(ControlCommand),
}

/// Trade command (extends existing TradeCommand)
/// Re-export from types::common for backward compatibility
pub use trading_core::types::common::TradeCommand;

/// Strategy command (lean - numeric IDs)
#[derive(Debug, Clone)]
pub enum StrategyCommand {
    Start {
        strategy_id: u64,
        pair_id: u32,
        config_hash: u64,  // Hash of config, not full JSON
    },
    Stop {
        strategy_id: u64,
        reason_code: u16,  // Numeric code
    },
    Update {
        strategy_id: u64,
        config_hash: u64,
    },
}

/// Level command (lean)
#[derive(Debug, Clone)]
pub enum LevelCommand {
    Create {
        level_config: LevelConfig,  // Keep full config for creation
    },
    Update {
        level_id: u64,
        updates: LevelUpdates,
    },
    Delete {
        level_id: u64,
    },
    Toggle {
        level_id: u64,
        enabled: bool,
    },
}

/// Config command (lean)
#[derive(Debug, Clone)]
pub enum ConfigCommand {
    Update {
        path_hash: u64,  // Hash path, not String
        value_hash: u64,  // Hash value, not full JSON
    },
    Reload {
        component_id: Option<u8>,  // Numeric component ID
    },
}

/// Control command
#[derive(Debug, Clone)]
pub enum ControlCommand {
    EnableTrading,
    DisableTrading,
    FlattenPositions {
        pair_id: Option<u32>,  // Numeric ID
    },
    KillSwitch, // Emergency stop
    Pause {
        component_id: u8,
    },
    Resume {
        component_id: u8,
    },
}
```

### 2.2 Command Router Trait

**Location**: `libs/trading_core/src/commands/router.rs` (new file)

**Purpose**: Single-consumer command router that dispatches commands to specific handlers.

**Key Design**: Commands are routed to exactly one handler. Request/response via oneshot channels.

**Implementation**:
```rust
use tokio::sync::{mpsc, oneshot};

/// Command router trait (single-consumer, routes to handlers)
pub trait CommandRouter: Send + Sync + 'static {
    /// Submit a command (non-blocking, fire-and-forget)
    fn submit(&self, command: Command) -> Result<(), CommandRouterError>;
    
    /// Submit a command and wait for result (request/response)
    async fn submit_and_wait(
        &self,
        payload: CommandPayload,
        correlation_id: u64,
    ) -> Result<CommandResult, CommandRouterError>;
}

/// Command result
#[derive(Debug)]
pub struct CommandResult {
    pub success: bool,
    pub error_code: Option<u16>,  // Numeric error code
    pub data: Option<bytes::Bytes>,  // Binary data, not JSON
}

/// Command router implementation
pub struct MpscCommandRouter {
    tx: mpsc::Sender<Command>,
}

impl MpscCommandRouter {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }
}

impl CommandRouter for MpscCommandRouter {
    fn submit(&self, command: Command) -> Result<(), CommandRouterError> {
        self.tx.try_send(command)
            .map_err(|_| CommandRouterError::ChannelFull)?;
        Ok(())
    }
    
    async fn submit_and_wait(
        &self,
        payload: CommandPayload,
        correlation_id: u64,
    ) -> Result<CommandResult, CommandRouterError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        
        let command = Command {
            correlation_id,
            payload,
            reply: Some(reply_tx),
        };
        
        self.tx.send(command)
            .await
            .map_err(|_| CommandRouterError::ChannelClosed)?;
        
        reply_rx.await
            .map_err(|_| CommandRouterError::HandlerDropped)?
            .map_err(|e| CommandRouterError::ExecutionFailed(e))
    }
}

#[derive(Debug)]
pub enum CommandRouterError {
    ChannelFull,
    ChannelClosed,
    HandlerDropped,
    ExecutionFailed(String),
    Timeout,
}
```

### 2.3 Command Dispatcher

**Location**: `libs/trading_core/src/commands/dispatcher.rs` (new file)

**Purpose**: Routes commands to specific handlers based on command type.

**Implementation**:
```rust
use tokio::sync::mpsc;

pub struct CommandDispatcher {
    command_rx: mpsc::Receiver<Command>,
    trade_handler: Box<dyn TradeCommandHandler>,
    strategy_handler: Box<dyn StrategyCommandHandler>,
    level_handler: Box<dyn LevelCommandHandler>,
    config_handler: Box<dyn ConfigCommandHandler>,
    control_handler: Box<dyn ControlCommandHandler>,
}

impl CommandDispatcher {
    pub async fn run(&mut self) -> Result<(), DispatcherError> {
        while let Some(command) = self.command_rx.recv().await {
            let result = match &command.payload {
                CommandPayload::Trade(cmd) => {
                    self.trade_handler.handle(cmd).await
                }
                CommandPayload::Strategy(cmd) => {
                    self.strategy_handler.handle(cmd).await
                }
                CommandPayload::Level(cmd) => {
                    self.level_handler.handle(cmd).await
                }
                CommandPayload::Config(cmd) => {
                    self.config_handler.handle(cmd).await
                }
                CommandPayload::Control(cmd) => {
                    self.control_handler.handle(cmd).await
                }
            };
            
            // Send reply if requested
            if let Some(reply_tx) = command.reply {
                let _ = reply_tx.send(result);
            }
        }
        Ok(())
    }
}
```

---

## Phase 3: External Gateway Layer

### 3.1 Gateway Architecture

**Location**: `src/gateway/` (new crate)

**Purpose**: Single boundary between core and external world. Handles all serialization, networking, authentication.

**Structure**:
```
src/gateway/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── event_gateway.rs      # Subscribes to Event Bus, publishes externally
    ├── command_gateway.rs    # Receives external commands, injects to Command Bus
    ├── config.rs             # Gateway configuration
    ├── transports/
    │   ├── mod.rs
    │   ├── grpc.rs           # gRPC transport
    │   ├── pubsub.rs         # Pub/Sub transport (NATS/Kafka)
    │   ├── ipc.rs            # IPC transport (Unix sockets)
    │   └── http.rs           # HTTP/REST transport (optional)
    └── serialization/
        ├── mod.rs
        └── event_serializer.rs
```

### 3.2 Event Gateway

**See**: Updated Event Gateway with DTP (Non-Blocking) section below (Phase 3.5).

### 3.3 Command Gateway

**Location**: `src/gateway/src/command_gateway.rs`

**Purpose**: Receive external commands, authenticate, validate, inject into Command Bus.

**Implementation**:
```rust
use trading_core::commands::{Command, CommandBus};
use std::sync::Arc;

pub struct CommandGateway {
    command_bus: Arc<dyn CommandBus>,
    authenticator: Arc<dyn Authenticator>,
    validator: Arc<dyn CommandValidator>,
    transports: Vec<Box<dyn CommandTransport>>,
}

impl CommandGateway {
    pub fn new(
        command_bus: Arc<dyn CommandBus>,
        authenticator: Arc<dyn Authenticator>,
        validator: Arc<dyn CommandValidator>,
    ) -> Self {
        Self {
            command_bus,
            authenticator,
            validator,
            transports: Vec::new(),
        }
    }
    
    /// Add a transport (gRPC, HTTP, IPC, etc.)
    pub fn add_transport(&mut self, transport: Box<dyn CommandTransport>) {
        self.transports.push(transport);
    }
    
    /// Start gateway (listens on transports, injects commands)
    pub async fn start(&mut self) -> Result<(), GatewayError> {
        // Start listening on all transports
        // For each received command:
        //   1. Authenticate
        //   2. Validate
        //   3. Inject into Command Bus
        // ...
    }
    
    async fn handle_command(
        &self,
        cmd: ExternalCommand,
        context: CommandContext,
    ) -> Result<CommandResult, GatewayError> {
        // Authenticate
        self.authenticator.authenticate(&context)?;
        
        // Validate
        self.validator.validate(&cmd, &context)?;
        
        // Convert external command to internal command
        let internal_cmd = self.convert_command(cmd)?;
        
        // Submit to Command Bus
        self.command_bus.submit(internal_cmd)?;
        
        Ok(CommandResult::success())
    }
}

pub trait CommandTransport: Send + Sync {
    async fn receive(&mut self) -> Result<Option<ExternalCommand>, GatewayError>;
    async fn send_response(&mut self, result: CommandResult) -> Result<(), GatewayError>;
}

pub trait Authenticator: Send + Sync {
    fn authenticate(&self, context: &CommandContext) -> Result<(), AuthError>;
}

pub trait CommandValidator: Send + Sync {
    fn validate(&self, cmd: &ExternalCommand, context: &CommandContext) -> Result<(), ValidationError>;
}
```

### 3.4 Gateway Configuration

**Location**: `src/gateway/src/config.rs`

**Implementation**:
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayConfig {
    /// Event export configuration
    pub events: EventConfig,
    
    /// Command handling configuration
    pub commands: CommandConfig,
    
    /// Transport configurations
    pub transports: TransportConfigs,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventConfig {
    /// Sample market data events (every Nth event)
    pub sample_market_data: Option<u32>,
    
    /// Always export these event types
    pub export_all: Vec<String>,
    
    /// Buffer size for event gateway
    pub buffer_size: usize,
    
    /// Enable event persistence (for replay)
    pub persist_events: bool,
    pub event_store_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandConfig {
    /// Require authentication for commands
    pub require_auth: bool,
    
    /// Rate limiting per client
    pub rate_limit_per_client: Option<u32>,
    
    /// Command timeout
    pub command_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransportConfigs {
    pub grpc: Option<GrpcConfig>,
    pub pubsub: Option<PubSubConfig>,
    pub ipc: Option<IpcConfig>,
    pub http: Option<HttpConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrpcConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PubSubConfig {
    pub enabled: bool,
    pub provider: PubSubProvider, // NATS, Kafka, etc.
    pub url: String,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum PubSubProvider {
    Nats,
    Kafka,
    Redis,
}
```

---

## Phase 3.5: Data Transfer Protocol (DTP)

### Overview

A **Data Transfer Protocol (DTP)** provides a standardized message envelope that wraps all events and commands crossing the Gateway boundary. This enables versioning, observability, event sourcing, and unified handling across all transports while maintaining zero-copy performance for internal communication.

### Why DTP?

**Current Approach Issues**:
- No standardized message envelope
- No versioning mechanism
- No correlation IDs for tracing
- No metadata (source, destination, routing hints)
- Each transport handles serialization differently
- No event sourcing/replay support
- Difficult to add new transports without duplicating serialization logic

**DTP Benefits**:
- ✅ **Unified Message Format**: Same envelope structure across all transports
- ✅ **Versioning**: Protocol version enables evolution without breaking changes
- ✅ **Observability**: Correlation IDs, message IDs, source/destination metadata
- ✅ **Event Sourcing Ready**: Self-contained messages can be persisted and replayed
- ✅ **Transport Agnostic**: Same envelope, different wire formats
- ✅ **Performance**: Internal bus remains zero-copy (< 1μs); DTP only at Gateway boundary

### DTP Message Envelope (Fixed-Header, Allocation-Light)

**Location**: `libs/trading_core/src/dtp/mod.rs` (new module)

**Key Design**: Fixed-size header with numeric IDs, TLV metadata, payload as Bytes.

```rust
use bytes::Bytes;

/// Data Transfer Protocol Message Envelope (fixed-header, allocation-light)
#[derive(Debug, Clone)]
pub struct DtpEnvelope {
    /// Protocol version (for backward compatibility)
    pub version: u8,
    
    /// Message type (Event or Command)
    pub message_type: MessageType,
    
    /// Message ID (u64 - monotonic counter + node id)
    pub message_id: u64,
    
    /// Correlation ID (u64, 0 = none)
    pub correlation_id: u64,
    
    /// Source component ID (u8)
    pub source_id: u8,
    
    /// Destination/routing hint (u8, 0 = broadcast)
    pub destination_id: u8,
    
    /// Timestamp (nanoseconds since epoch)
    pub timestamp: u64,
    
    /// Payload (serialized event or command as Bytes)
    pub payload: Bytes,
    
    /// Metadata (TLV format - Vec<(tag: u16, value: Bytes)>)
    pub metadata: Vec<(u16, Bytes)>,
}

/// Fixed-size header (for efficient parsing)
/// Header is fixed layout and serialized/deserialized using bytes Buf/BufMut (no unsafe casts).
/// Do not use repr(C, packed) to avoid unaligned access bugs.
pub struct DtpHeader {
    pub version: u8,
    pub message_type: u8,  // MessageType as u8
    pub message_id: u64,
    pub correlation_id: u64,
    pub source_id: u8,
    pub destination_id: u8,
    pub timestamp: u64,
    pub metadata_count: u16,
    pub payload_length: u32,
}

impl DtpHeader {
    /// Serialize header to bytes using BufMut
    pub fn serialize<B: bytes::BufMut>(&self, buf: &mut B) {
        buf.put_u8(self.version);
        buf.put_u8(self.message_type);
        buf.put_u64(self.message_id);
        buf.put_u64(self.correlation_id);
        buf.put_u8(self.source_id);
        buf.put_u8(self.destination_id);
        buf.put_u64(self.timestamp);
        buf.put_u16(self.metadata_count);
        buf.put_u32(self.payload_length);
    }
    
    /// Deserialize header from bytes using Buf
    pub fn deserialize<B: bytes::Buf>(buf: &mut B) -> Result<Self, DtpError> {
        Ok(Self {
            version: buf.get_u8(),
            message_type: buf.get_u8(),
            message_id: buf.get_u64(),
            correlation_id: buf.get_u64(),
            source_id: buf.get_u8(),
            destination_id: buf.get_u8(),
            timestamp: buf.get_u64(),
            metadata_count: buf.get_u16(),
            payload_length: buf.get_u32(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Event = 1,
    Command = 2,
    Response = 3,
}

/// DTP Serializer (creates envelope from internal types)
/// Uses Protobuf as canonical encoding
pub struct DtpSerializer {
    version: u8,
    source_id: u8,
    message_id_generator: MessageIdGenerator,
}

impl DtpSerializer {
    pub fn new(version: u8, source_id: u8) -> Self {
        Self {
            version,
            source_id,
            message_id_generator: MessageIdGenerator::new(),
        }
    }
    
    /// Serialize hot event to DTP (Protobuf only)
    pub fn serialize_hot_event(
        &self,
        event: &HotEvent,
    ) -> Result<DtpEnvelope, DtpError> {
        // Serialize to Protobuf
        let payload = self.serialize_protobuf(event)?;
        
        Ok(DtpEnvelope {
            version: self.version,
            message_type: MessageType::Event,
            message_id: self.message_id_generator.next(),
            correlation_id: 0,
            source_id: self.source_id,
            destination_id: 0,  // Broadcast
            timestamp: current_timestamp_nanos(),
            payload: Bytes::from(payload),
            metadata: Vec::new(),
        })
    }
    
    /// Serialize cold event to DTP (Protobuf only)
    pub fn serialize_cold_event(
        &self,
        event: &Arc<ColdEvent>,
    ) -> Result<DtpEnvelope, DtpError> {
        let payload = self.serialize_protobuf(event.as_ref())?;
        
        Ok(DtpEnvelope {
            version: self.version,
            message_type: MessageType::Event,
            message_id: self.message_id_generator.next(),
            correlation_id: 0,
            source_id: self.source_id,
            destination_id: 0,
            timestamp: current_timestamp_nanos(),
            payload: Bytes::from(payload),
            metadata: Vec::new(),
        })
    }
    
    /// Serialize command to DTP (Protobuf only)
    pub fn serialize_command(
        &self,
        command: &CommandPayload,
        correlation_id: u64,
    ) -> Result<DtpEnvelope, DtpError> {
        let payload = self.serialize_protobuf(command)?;
        
        Ok(DtpEnvelope {
            version: self.version,
            message_type: MessageType::Command,
            message_id: self.message_id_generator.next(),
            correlation_id,
            source_id: self.source_id,
            destination_id: 0,
            timestamp: current_timestamp_nanos(),
            payload: Bytes::from(payload),
            metadata: Vec::new(),
        })
    }
    
    fn serialize_protobuf<T>(&self, value: &T) -> Result<Vec<u8>, DtpError>
    where
        T: prost::Message,
    {
        let mut buf = Vec::new();
        prost::Message::encode(value, &mut buf)
            .map_err(|e| DtpError::SerializationFailed(e.to_string()))?;
        Ok(buf)
    }
}

/// Message ID generator (monotonic counter + node id)
struct MessageIdGenerator {
    counter: std::sync::atomic::AtomicU64,
    node_id: u16,
}

impl MessageIdGenerator {
    fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
            node_id: 1,  // TODO: Get from config
        }
    }
    
    fn next(&self) -> u64 {
        let counter = self.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        (u64::from(self.node_id) << 48) | (counter & 0x0000_FFFF_FFFF_FFFF)
    }
}

/// DTP Deserializer (extracts internal types from envelope)
/// Protobuf only - single canonical encoding
pub struct DtpDeserializer;

impl DtpDeserializer {
    /// Deserialize hot event from DTP envelope (Protobuf only)
    pub fn deserialize_hot_event(
        &self,
        envelope: &DtpEnvelope,
    ) -> Result<HotEvent, DtpError> {
        self.deserialize_protobuf(&envelope.payload)
    }
    
    /// Deserialize cold event from DTP envelope (Protobuf only)
    pub fn deserialize_cold_event(
        &self,
        envelope: &DtpEnvelope,
    ) -> Result<ColdEvent, DtpError> {
        self.deserialize_protobuf(&envelope.payload)
    }
    
    /// Deserialize command from DTP envelope (Protobuf only)
    pub fn deserialize_command(
        &self,
        envelope: &DtpEnvelope,
    ) -> Result<CommandPayload, DtpError> {
        self.deserialize_protobuf(&envelope.payload)
    }
    
    fn deserialize_protobuf<T>(&self, payload: &Bytes) -> Result<T, DtpError>
    where
        T: prost::Message + Default,
    {
        prost::Message::decode(payload.as_ref())
            .map_err(|e| DtpError::DeserializationFailed(e.to_string()))
    }
}
```

### Updated Architecture with DTP

```
┌─────────────────────────────────────────────────────────────┐
│                    Core Runtime (UI-Agnostic)                │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Event Bus (Internal)                              │    │
│  │  - Zero-copy Rust types                           │    │
│  │  - < 1μs latency                                  │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              External Gateway Layer                           │
│  ┌────────────────────────────────────────────────────┐    │
│  │ DTP Serializer                                    │    │
│  │  - Wraps Event in DTP envelope                   │    │
│  │  - Adds metadata (version, correlation ID, etc.)  │    │
│  │  - Serializes payload (protobuf/JSON/msgpack)    │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Event Gateway                                     │    │
│  │  - Subscribes to Event Bus                       │    │
│  │  - Filters, aggregates                           │    │
│  │  - Wraps in DTP envelope                         │    │
│  │  - Publishes to transports                       │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
         │              │              │              │
         ▼              ▼              ▼              ▼
    ┌────────┐    ┌────────┐    ┌────────┐    ┌────────┐
    │  gRPC  │    │ PubSub │    │  IPC   │    │  HTTP  │
    │ (DTP)  │    │ (DTP)  │    │ (DTP)  │    │ (DTP)  │
    └────────┘    └────────┘    └────────┘    └────────┘
```

### Key Design Principles

1. **Internal Bus Remains Low-Allocation**: DTP envelope is only created at Gateway boundary
2. **Gateway Creates DTP Envelope**: Adds metadata, version, correlation IDs (numeric, not Strings)
3. **Single Canonical Encoding**: Protobuf for all DTP payloads (JSON only as derived stream for tooling)
4. **Consumers Deserialize DTP**: Extract envelope, deserialize Protobuf payload
5. **Fixed-Header Format**: DTP header is fixed-size for efficient parsing

### Updated Event Gateway with DTP (Non-Blocking)

**Location**: `src/gateway/src/event_gateway.rs`

**Key Design**: Non-blocking fanout with per-transport bounded queues. Each transport runs its own task.

```rust
use trading_core::dtp::{DtpSerializer, DtpEnvelope};
use trading_core::events::{HotEventRx, ColdEvent};
use tokio::sync::{mpsc, broadcast};
use std::sync::Arc;

pub struct EventGateway {
    dtp_serializer: DtpSerializer,
    transport_queues: Vec<TransportQueue>,
}

struct TransportQueue {
    name: String,
    queue: mpsc::Sender<DtpEnvelope>,
    handle: tokio::task::JoinHandle<()>,
}

impl EventGateway {
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            dtp_serializer: DtpSerializer::new(1, 1), // version, source_id
            transport_queues: Vec::new(),
        }
    }
    
    /// Add a transport with bounded queue
    pub fn add_transport(
        &mut self,
        name: String,
        transport: Box<dyn EventTransport>,
        queue_capacity: usize,
    ) {
        let (tx, mut rx) = mpsc::channel(queue_capacity);
        
        // Spawn worker task for this transport
        let handle = tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                if let Err(e) = transport.publish_dtp(&envelope).await {
                    warn!("Transport {} failed: {}", name, e);
                    // Continue - don't crash on transport errors
                }
            }
        });
        
        self.transport_queues.push(TransportQueue {
            name,
            queue: tx,
            handle,
        });
    }
    
    /// Start gateway (receives hot and cold receivers)
    pub async fn start(
        &mut self,
        mut hot_rx: HotEventRx,
        mut cold_rx: broadcast::Receiver<Arc<ColdEvent>>,
    ) -> Result<(), GatewayError> {
        loop {
            tokio::select! {
                // Handle hot events (mpsc returns Option, not Result)
                event = hot_rx.recv() => {
                    match event {
                        Some(event) => {
                            if self.should_export_hot(&event) {
                                if let Ok(dtp) = self.dtp_serializer.serialize_hot_event(&event) {
                                    self.fanout_dtp(dtp).await;
                                }
                            }
                        }
                        None => {
                            // Channel closed
                            error!("Hot event channel closed");
                            break;
                        }
                    }
                }
                
                // Handle cold events (broadcast returns Result, can be Lagged)
                result = cold_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if self.should_export_cold(&event) {
                                if let Ok(dtp) = self.dtp_serializer.serialize_cold_event(&event) {
                                    self.fanout_dtp(dtp).await;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("Cold event gateway lagged, skipped {} events", skipped);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            error!("Cold event bus closed");
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Fanout DTP to all transports (non-blocking)
    /// Note: DtpEnvelope.payload is Bytes (shared), so cloning the envelope is cheap
    async fn fanout_dtp(&self, envelope: DtpEnvelope) {
        for transport_queue in &self.transport_queues {
            // Try to send - if queue is full, drop for this transport only
            // Cloning envelope is cheap because payload is Bytes (shared reference)
            match transport_queue.queue.try_send(envelope.clone()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // Queue full - drop or sample based on policy
                    // For now, just drop (could implement sampling)
                    warn!("Transport {} queue full, dropping event", transport_queue.name);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    warn!("Transport {} queue closed", transport_queue.name);
                }
            }
        }
    }
    
    fn should_export_hot(&self, event: &HotEvent) -> bool {
        // Always export hot events (or implement sampling)
        true
    }
    
    fn should_export_cold(&self, event: &Arc<ColdEvent>) -> bool {
        match event.as_ref() {
            // Always export important events
            ColdEvent::Order(_) | ColdEvent::Fill(_) | ColdEvent::Position(_) => true,
            ColdEvent::Error(_) | ColdEvent::Risk(_) => true,
            // Always export slow events
            ColdEvent::Config(_) | ColdEvent::Health(_) => true,
            _ => true,
        }
    }
}

pub trait EventTransport: Send + Sync {
    /// Publish DTP envelope (transport sends to network)
    async fn publish_dtp(&mut self, envelope: &DtpEnvelope) -> Result<(), GatewayError>;
}
```

### Potential Concerns & Mitigations

**Concern 1: Overhead**
- **Mitigation**: Envelope only created at Gateway boundary (not internal). Internal buses remain low-allocation. Fixed-header format minimizes parsing overhead.

**Concern 2: Complexity**
- **Mitigation**: DTP is simple (fixed header + TLV metadata). Benefits outweigh complexity. Makes adding new transports easier.

**Concern 3: Serialization Performance**
- **Mitigation**: Single canonical format (Protobuf) eliminates format selection overhead. Protobuf is fast and compact. JSON only generated as derived stream for tooling.

**Concern 4: Gateway Blocking**
- **Mitigation**: Per-transport bounded queues with independent worker tasks. `try_send` prevents blocking. Drop/sample policy when queues are full.

---

## Phase 4: Transport Implementations

### 4.1 gRPC Transport

**Location**: `src/gateway/src/transports/grpc.rs`

**Purpose**: gRPC server for TUI and other gRPC clients.

**Implementation**:
```rust
use tonic::{Request, Response, Status};

pub struct GrpcEventTransport {
    // gRPC streaming service
}

impl EventTransport for GrpcEventTransport {
    async fn publish_dtp(&mut self, envelope: &DtpEnvelope) -> Result<(), GatewayError> {
        // Convert DTP envelope to gRPC message
        let grpc_msg = DtpToGrpcConverter::convert(envelope)?;
        
        // Stream to connected clients
        self.stream_to_clients(grpc_msg).await?;
        
        Ok(())
    }
}

pub struct GrpcCommandTransport {
    // gRPC service for commands
}

impl CommandTransport for GrpcCommandTransport {
    async fn receive(&mut self) -> Result<Option<ExternalCommand>, GatewayError> {
        // Receive from gRPC client
        // ...
    }
    
    async fn send_response(&mut self, result: CommandResult) -> Result<(), GatewayError> {
        // Send response to gRPC client
        // ...
    }
}
```

### 4.2 Pub/Sub Transport

**Location**: `src/gateway/src/transports/pubsub.rs`

**Purpose**: Publish events to message bus (NATS, Kafka, etc.) for analytics, alerting, external consumers.

**Implementation**:
```rust
pub struct NatsEventTransport {
    client: nats::Client,
    topics: Vec<String>,
}

impl EventTransport for NatsEventTransport {
    async fn publish_dtp(&mut self, envelope: &DtpEnvelope) -> Result<(), GatewayError> {
        // Serialize DTP envelope to Protobuf (canonical format)
        let bytes = envelope.serialize_to_protobuf()?;
        
        // Publish to NATS (envelope is self-contained)
        for topic in &self.topics {
            self.client.publish(topic, bytes.clone()).await?;
        }
        Ok(())
    }
}
```

### 4.3 IPC Transport

**Location**: `src/gateway/src/transports/ipc.rs`

**Purpose**: Unix domain sockets or shared memory for HFT daemon integration.

**Implementation**:
```rust
pub struct IpcEventTransport {
    socket: UnixStream,
}

impl EventTransport for IpcEventTransport {
    async fn publish_dtp(&mut self, envelope: &DtpEnvelope) -> Result<(), GatewayError> {
        // Serialize DTP envelope to Protobuf (canonical format)
        let bytes = envelope.serialize_to_protobuf()?;
        
        // Write to Unix socket
        self.socket.write_all(&bytes).await?;
        Ok(())
    }
}
```

---

## Phase 5: Core Engine Integration

### 5.1 Update Exchange Engine

**Location**: `src/exchange_engine/src/state/publisher.rs`

**Changes**: Publish to Event Bus instead of (or in addition to) direct channels.

```rust
impl ExchangeStatePublisher {
    pub fn new(
        exchange_update_tx: ExchangeUpdateSender,
        cold_event_bus: Arc<dyn ColdEventBus>, // NEW
    ) -> Self {
        Self {
            exchange_update_tx,
            cold_event_bus, // NEW
        }
    }
    
    pub fn publish_order_placed(&self, order: &Order) -> Result<(), EngineError> {
        // Existing: publish to broadcast channel (for backward compatibility)
        self.exchange_update_tx.send(ExchangeUpdate::Order(order.clone()))?;
        
        // NEW: publish to Cold Event Bus (lean event)
        let pair_id = self.pair_registry.get_id(&order.symbol)?;
        let event = Arc::new(ColdEvent::Order(OrderEvent::Placed {
            order_id: order.order_id.clone(),
            pair_id,
            side: order.side,
            price: order.price,
            quantity: order.amount,
            order_type: order.order_type,
            timestamp: order.internal_created_at,
        }));
        self.cold_event_bus.publish(event)?;
        
        Ok(())
    }
}
```

### 5.2 Update Trading Engine

**Location**: `src/trading_engine/src/strategies/executor.rs`

**Changes**: Subscribe to Command Bus, execute commands.

```rust
pub struct StrategyExecutor {
    executor: Arc<dyn ExecutionInterface>,
    command_rx: mpsc::Receiver<Command>, // NEW
    cold_event_bus: Arc<dyn ColdEventBus>, // NEW
    // ... existing fields
}

impl StrategyExecutor {
    pub async fn run(&mut self) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                // NEW: Handle commands from Command Router
                cmd = self.command_rx.recv() => {
                    if let Some(command) = cmd {
                        let result = self.handle_command(command).await;
                        // Send reply if requested
                        if let Some(reply_tx) = command.reply {
                            let _ = reply_tx.send(result);
                        }
                    }
                }
                // ... existing event handling
            }
        }
    }
    
    async fn handle_command(&self, cmd: Command) -> Result<CommandResult, EngineError> {
        match cmd.payload {
            CommandPayload::Trade(trade_cmd) => {
                // Execute trade
                let result = self.executor.execute_trade_command(trade_cmd).await?;
                
                // Publish event (lean)
                let event = Arc::new(ColdEvent::Order(OrderEvent::Placed { ... }));
                self.cold_event_bus.publish(event)?;
                
                Ok(CommandResult { success: true, error_code: None, data: None })
            }
            CommandPayload::Strategy(strategy_cmd) => {
                // Handle strategy command
                // ...
                Ok(CommandResult { success: true, error_code: None, data: None })
            }
            _ => Ok(CommandResult { success: false, error_code: Some(1), data: None })
        }
    }
}
```

### 5.3 Update Runner

**Location**: `src/runner/src/lib.rs`

**Changes**: Initialize Event Bus, Command Bus, Gateway instead of direct gRPC.

```rust
pub async fn strategy_runner(...) -> Result<(), EngineError> {
    // ... existing initialization ...
    
    // NEW: Create Hot and Cold Event Buses
    let (hot_event_tx, hot_rx) = hot_event_channel(10000);
    let (cold_event_bus, _) = BroadcastColdEventBus::new(10000);
    
    // NEW: Create Command Router
    let (command_router, command_rx) = MpscCommandRouter::new(1000);
    
    // NEW: Create Gateway
    let mut gateway = EventGateway::new(gateway_config);
    gateway.add_transport("grpc".to_string(), Box::new(GrpcEventTransport::new(...)), 1000);
    gateway.add_transport("nats".to_string(), Box::new(NatsEventTransport::new(...)), 1000);
    
    // Subscribe to cold lane
    let cold_rx = cold_event_bus.subscribe();
    
    // Start gateway with receivers
    tokio::spawn(async move {
        gateway.start(hot_rx, cold_rx).await;
    });
    
    let mut command_gateway = CommandGateway::new(
        Arc::new(command_router.clone()),
        authenticator,
        validator,
    );
    command_gateway.add_transport(Box::new(GrpcCommandTransport::new(...)));
    
    // Pass Event Bus and Command Bus to engines
    // ...
    
    // Gateway already started above with receivers
    
    tokio::spawn(async move {
        command_gateway.start().await;
    });
    
    // ... rest of runner ...
}
```

---

## Phase 6: Backward Compatibility

### 6.1 Migration Strategy

**Key Principle**: No breaking changes. Existing code continues to work.

1. **Phase 1**: Add Event Bus alongside existing channels (dual publish)
2. **Phase 2**: Gateway subscribes to Event Bus, publishes via gRPC (existing TUI works)
3. **Phase 3**: Gradually migrate consumers to Event Bus
4. **Phase 4**: Remove direct channel access (optional, future)

### 6.2 Dual Publishing

During migration, publish to both:
- Existing broadcast channels (for backward compatibility)
- New Event Bus (for Gateway)

This ensures zero downtime migration.

---

## Implementation Phases

### Phase 1: Event Bus Foundation (Hot/Cold Split) (Week 1-2)
- [ ] Create `trading_core::events::hot` module (hot lane events)
- [ ] Create `trading_core::events::cold` module (cold lane events)
- [ ] Implement `hot_event_channel()` function returning `(HotEventTx, HotEventRx)` (or SPSC ring buffer)
- [ ] Implement `ColdEventBus` trait and `BroadcastColdEventBus` (Arc-wrapped)
- [ ] Update `ExchangeStatePublisher` to publish lean cold events
- [ ] Update `MarketDataStatePublisher` to publish compact hot events (using HotEventTx)
- [ ] Add numeric ID registries (pair_id, asset_id, component_id mappings)
- [ ] Add unit tests

### Phase 2: Command Router Foundation (Week 2-3)
- [ ] Create `trading_core::commands` module
- [ ] Define command types with correlation IDs and oneshot replies
- [ ] Implement `CommandRouter` trait and `MpscCommandRouter`
- [ ] Implement `CommandDispatcher` for routing to handlers
- [ ] Update `StrategyExecutor` to handle commands via dispatcher
- [ ] Add unit tests

### Phase 3: Gateway Layer (Week 3-5)
- [ ] Create `gateway` crate
- [ ] Implement DTP core library (`libs/trading_core/src/dtp/`)
  - [ ] Define `DtpEnvelope` with fixed header (numeric IDs, TLV metadata)
  - [ ] Implement `DtpSerializer` (Protobuf only)
  - [ ] Implement `DtpDeserializer`
  - [ ] Add message ID generator (monotonic counter + node id)
  - [ ] Add versioning support
- [ ] Implement `EventGateway` (non-blocking, per-transport queues)
- [ ] Implement `CommandGateway` (with DTP integration, oneshot replies)
- [ ] Add authentication and validation
- [ ] Add configuration
- [ ] Add integration tests

### Phase 4: gRPC Transport (Week 5-6)
- [ ] Implement `GrpcEventTransport` (with DTP Protobuf support)
- [ ] Implement `GrpcCommandTransport` (with DTP support, oneshot replies)
- [ ] Update gRPC proto definitions (DTP envelope as Protobuf)
- [ ] Migrate existing gRPC service to Gateway
- [ ] Test with existing TUI

### Phase 5: Additional Transports (Week 6-7)
- [ ] Implement `NatsEventTransport` (Pub/Sub with DTP Protobuf)
- [ ] Implement `IpcEventTransport` (Unix sockets with DTP Protobuf)
- [ ] Add HTTP transport (optional, with DTP Protobuf)
- [ ] Add JSON derived stream for tooling (optional, separate from canonical)
- [ ] Test all transports

### Phase 6: Integration & Testing (Week 7-8)
- [ ] End-to-end testing
- [ ] Performance testing (ensure <1μs internal latency maintained)
- [ ] Backward compatibility testing
- [ ] Documentation
- [ ] Migration guide

---

## Success Criteria

- [ ] Core runtime has zero UI/transport dependencies
- [ ] Hot lane bus functional (target <1μs under expected load, allocation-free)
- [ ] Cold lane bus functional (target <10μs under expected load, Arc-wrapped)
- [ ] Command router functional (single-consumer, oneshot replies)
- [ ] DTP core library implemented with fixed-header format
- [ ] Gateway layer working with non-blocking per-transport queues
- [ ] Existing TUI works without changes
- [ ] Can add new transports without modifying core
- [ ] Hot path latency targets met (measured via benchmarks)
- [ ] Cold path latency targets met (measured via benchmarks)
- [ ] Multiple consumers can connect simultaneously
- [ ] Authentication and validation working
- [ ] DTP enables versioning, observability, and event sourcing
- [ ] Documentation complete
- [ ] Ready for additional UIs/consumers

---

## Future Enhancements

1. **Event Persistence**: Store events for replay (event sourcing)
2. **Event Replay**: New consumers can replay historical events
3. **Web UI**: 
   - HTTP/REST transport for web dashboard
   - WebSocket for real-time updates
   - Authentication & authorization (API keys, session management)
   - Portfolio dashboard (positions, balances, performance charts)
   - Historical data visualization (charts, reports)
4. **Mobile App**: Add WebSocket transport for mobile
5. **Analytics Pipeline**: Pub/Sub → Data warehouse
6. **Alerting**: Pub/Sub → Alerting system
7. **HFT Daemon**: IPC transport for ultra-low latency (see Proposal 10)
8. **Multi-tenant**: Gateway handles multiple tenants/clients
9. **Observability Enhancements**:
   - Prometheus metrics integration
   - Health check endpoints (readiness, liveness)
   - Structured logging with correlation IDs
   - Performance tracking (latency, queue depths, memory)
10. **TUI Enhancements** (via Gateway):
    - Real-time log streaming (filterable, searchable)
    - System metrics dashboard
    - Component health status
    - Performance profiling
    - Emergency controls (kill switch, circuit breaker controls)

---

## Revision Summary (v2.0)

This proposal has been revised based on low-latency system feedback. Key changes:

### 1. Hot/Cold Lane Separation
- **Hot Lane**: Market data events use SPSC/bounded MPSC channels with compact structs (IDs + deltas)
- **Cold Lane**: State change events use `broadcast::Sender<Arc<ColdEvent>>` for sharing without clones
- **Rationale**: Market data is ultra-high frequency and needs allocation-free path; state changes are lower frequency and can use Arc

### 2. Fixed-Header DTP Envelope
- **Numeric IDs**: `message_id: u64`, `correlation_id: u64` (not Strings)
- **TLV Metadata**: `Vec<(u16, Bytes)>` instead of `HashMap<String, String>`
- **Fixed Header**: Packed struct for efficient parsing
- **Rationale**: Eliminates heap allocations per message, reduces cache misses

### 3. Command Router (Single-Consumer)
- **Routing**: Commands routed to specific handlers, not pub-sub
- **Request/Response**: Uses `oneshot::Sender<CommandResult>` for replies
- **Correlation IDs**: `u64` for tracking request/response pairs
- **Rationale**: Commands should have single execution point, not broadcast

### 4. Lean Event Payloads
- **Hot Events**: Best bid/ask only (not full orderbook Vec)
- **Cold Events**: IDs + key fields only (not full Order objects)
- **Numeric Codes**: Error codes, reason codes as `u16`/`u8`, not Strings
- **Rationale**: Reduces allocation overhead, consumers query state stores for full objects

### 5. Non-Blocking Gateway Fanout
- **Per-Transport Queues**: Each transport has bounded `mpsc::channel`
- **Independent Workers**: Each transport runs its own task
- **Try-Send**: `try_send()` prevents blocking; drop/sample when full
- **Rationale**: Prevents head-of-line blocking when one transport is slow

### 6. Single Canonical Encoding
- **Protobuf Only**: All DTP payloads use Protobuf
- **JSON as Derived**: JSON only generated as separate stream for tooling
- **Rationale**: Eliminates format selection overhead, reduces maintenance burden

### Performance Characteristics

- **Hot Lane**: Target <1μs latency under expected load (allocation-free, compact structs, measured via benchmarks)
- **Cold Lane**: Target <10μs latency under expected load (Arc-wrapped, lean payloads, measured via benchmarks)
- **Gateway**: Non-blocking, per-transport queues prevent head-of-line blocking
- **DTP**: Fixed-header format minimizes parsing overhead

### Migration Notes

- Existing code continues to work (dual publishing during migration)
- Numeric ID registries needed (pair_id, asset_id, component_id mappings)
- State stores may be needed for consumers that need full objects (not just event deltas)

---

**Next Steps**: Review revised proposal, approve architecture, begin Phase 1 implementation.

