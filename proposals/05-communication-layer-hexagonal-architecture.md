# Proposal: Communication Layer & Hexagonal Architecture

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft  
**Prerequisites**: None (can be implemented independently)

## Executive Summary

This proposal outlines the transformation of the trading system to a **UI-agnostic, event-driven architecture** using **Hexagonal (Ports & Adapters) principles**. The core runtime will be completely decoupled from any UI, allowing unlimited pluggable consumers (TUI, web dashboards, analytics, automation, HFT daemon) without modifying the core system.

### Key Objectives

1. **Decouple core from UI** - Core runtime has zero knowledge of gRPC, databases, or UI frameworks
2. **Formalize Event/Command buses** - Internal communication via stable, well-defined contracts
3. **Introduce External Gateway** - Single boundary for all external communication
4. **Support multiple transports** - gRPC, Pub/Sub, IPC simultaneously
5. **Maintain ultra-low latency** - Internal channels remain <1μs
6. **Enable horizontal scaling** - Multiple consumers, multiple strategies, no tight coupling

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
│  │ Event Bus (Internal)                              │    │
│  │  - MarketDataEvent                                 │    │
│  │  - OrderEvent                                      │    │
│  │  - PositionEvent                                   │    │
│  │  - RiskEvent                                       │    │
│  │  - StrategyEvent                                   │    │
│  │  - Zero-copy, < 1μs latency                       │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Command Bus (Internal)                             │    │
│  │  - TradeCommand                                    │    │
│  │  - StrategyCommand                                 │    │
│  │  - ConfigCommand                                   │    │
│  │  - ControlCommand                                  │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              External Gateway Layer                           │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Event Gateway                                     │    │
│  │  - Subscribes to Event Bus                        │    │
│  │  - Filters, aggregates, serializes                │    │
│  │  - Publishes to external transports               │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Command Gateway                                   │    │
│  │  - Receives external commands                     │    │
│  │  - Authenticates, validates                       │    │
│  │  - Injects into Command Bus                       │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
         │              │              │              │
         ▼              ▼              ▼              ▼
    ┌────────┐    ┌────────┐    ┌────────┐    ┌────────┐
    │  gRPC  │    │ PubSub │    │  IPC   │    │  HTTP  │
    │ (TUI)  │    │(Analytics)│ │ (HFT)  │    │ (Web)  │
    └────────┘    └────────┘    └────────┘    └────────┘
```

### Key Principles

1. **Core is UI-agnostic** - No gRPC, no HTTP, no UI frameworks in core
2. **Internal buses never exposed** - External systems never see internal channels
3. **Gateway is the boundary** - Only place where serialization, networking, auth exist
4. **Multiple transports** - All can run simultaneously
5. **Event sourcing ready** - Events can be persisted for replay

---

## Phase 1: Formalize Event Bus

### 1.1 Event Type Definitions

**Location**: `libs/trading_core/src/events/mod.rs` (new module)

**Purpose**: Define stable, versioned event types that represent facts about the system state.

**Implementation**:
```rust
/// Core event types (facts about system state)
/// These are internal events - zero-copy, ultra-low latency
#[derive(Debug, Clone)]
pub enum Event {
    // Market Data Events
    MarketData(MarketDataEvent),
    
    // Exchange Events
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

/// Market data event
#[derive(Debug, Clone)]
pub enum MarketDataEvent {
    Orderbook {
        pair: Pair,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
        timestamp: u64,
    },
    Trade {
        pair: Pair,
        price: Decimal,
        quantity: Decimal,
        side: Side,
        timestamp: u64,
    },
    // Futures-specific
    MarkPrice { pair: Pair, price: Decimal, timestamp: u64 },
    FundingRate { pair: Pair, rate: Decimal, timestamp: u64 },
}

/// Order event
#[derive(Debug, Clone)]
pub enum OrderEvent {
    Placed {
        order: Order,
        timestamp: u64,
    },
    Updated {
        order: Order,
        timestamp: u64,
    },
    Filled {
        order: Order,
        fill_price: Decimal,
        fill_quantity: Decimal,
        timestamp: u64,
    },
    Cancelled {
        order_id: OrderId,
        reason: String,
        timestamp: u64,
    },
}

/// Fill event (separate from order for clarity)
#[derive(Debug, Clone)]
pub struct FillEvent {
    pub order_id: OrderId,
    pub pair: Pair,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
    pub fee: Decimal,
    pub timestamp: u64,
}

/// Position event
#[derive(Debug, Clone)]
pub enum PositionEvent {
    Opened {
        pair: Pair,
        quantity: Decimal,
        entry_price: Decimal,
        timestamp: u64,
    },
    Updated {
        pair: Pair,
        quantity: Decimal,
        avg_price: Decimal,
        unrealized_pnl: Decimal,
        timestamp: u64,
    },
    Closed {
        pair: Pair,
        realized_pnl: Decimal,
        timestamp: u64,
    },
}

/// Balance event
#[derive(Debug, Clone)]
pub struct BalanceEvent {
    pub asset: Symbol,
    pub free: Decimal,
    pub used: Decimal,
    pub total: Decimal,
    pub timestamp: u64,
}

/// Strategy event
#[derive(Debug, Clone)]
pub enum StrategyEvent {
    Started {
        strategy_id: String,
        symbol: Pair,
        timestamp: u64,
    },
    Stopped {
        strategy_id: String,
        reason: String,
        timestamp: u64,
    },
    Signal {
        strategy_id: String,
        signal: Signal,
        timestamp: u64,
    },
}

/// Level event
#[derive(Debug, Clone)]
pub enum LevelEvent {
    Created { level_id: String, pair: Pair, timestamp: u64 },
    Updated { level_id: String, timestamp: u64 },
    Deleted { level_id: String, timestamp: u64 },
    Filled { level_id: String, timestamp: u64 },
}

/// Risk event
#[derive(Debug, Clone)]
pub enum RiskEvent {
    LimitExceeded {
        limit_type: String,
        current: Decimal,
        limit: Decimal,
        timestamp: u64,
    },
    CircuitBreaker {
        action: String,
        reason: String,
        timestamp: u64,
    },
}

/// Config event
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    Updated { path: String, value: String, timestamp: u64 },
    Reloaded { timestamp: u64 },
}

/// Health event
#[derive(Debug, Clone)]
pub struct HealthEvent {
    pub component: String,
    pub status: HealthStatus,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Error event
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    pub component: String,
    pub error: String,
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

### 1.2 Event Bus Trait

**Location**: `libs/trading_core/src/events/bus.rs` (new file)

**Purpose**: Abstract interface for event bus (allows different implementations).

**Implementation**:
```rust
use tokio::sync::broadcast;

/// Event bus trait (allows different implementations)
pub trait EventBus: Send + Sync + 'static {
    /// Publish an event
    fn publish(&self, event: Event) -> Result<(), EventBusError>;
    
    /// Subscribe to all events
    fn subscribe(&self) -> broadcast::Receiver<Event>;
    
    /// Subscribe to specific event types
    fn subscribe_filtered<F>(&self, filter: F) -> FilteredReceiver<Event>
    where
        F: Fn(&Event) -> bool + Send + Sync + 'static;
}

/// Default implementation using broadcast channels
pub struct BroadcastEventBus {
    tx: broadcast::Sender<Event>,
}

impl BroadcastEventBus {
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<Event>) {
        let (tx, rx) = broadcast::channel(capacity);
        (Self { tx }, rx)
    }
}

impl EventBus for BroadcastEventBus {
    fn publish(&self, event: Event) -> Result<(), EventBusError> {
        self.tx.send(event).map_err(|_| EventBusError::ChannelClosed)?;
        Ok(())
    }
    
    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
    
    fn subscribe_filtered<F>(&self, filter: F) -> FilteredReceiver<Event>
    where
        F: Fn(&Event) -> bool + Send + Sync + 'static,
    {
        // Implementation filters events
        // ...
    }
}

#[derive(Debug)]
pub enum EventBusError {
    ChannelClosed,
    CapacityExceeded,
}
```

### 1.3 Update Existing Publishers

**Location**: `src/exchange_engine/src/state/publisher.rs` (extend existing)

**Changes**: Instead of publishing `ExchangeUpdate`, publish `Event::Order`, `Event::Balance`, etc.

```rust
impl ExchangeStatePublisher {
    // ... existing methods ...
    
    /// Publish order event (new)
    pub async fn publish_order_event(&self, order: &Order) -> Result<(), EngineError> {
        use trading_core::events::{Event, OrderEvent};
        
        let event = Event::Order(OrderEvent::Placed {
            order: order.clone(),
            timestamp: order.internal_created_at,
        });
        
        self.event_bus.publish(event)?;
        Ok(())
    }
    
    /// Publish fill event (new)
    pub async fn publish_fill_event(&self, fill: &FillEvent) -> Result<(), EngineError> {
        use trading_core::events::Event;
        
        let event = Event::Fill(fill.clone());
        self.event_bus.publish(event)?;
        Ok(())
    }
}
```

---

## Phase 2: Formalize Command Bus

### 2.1 Command Type Definitions

**Location**: `libs/trading_core/src/commands/mod.rs` (new module)

**Purpose**: Define command types that represent intent to change system state.

**Implementation**:
```rust
/// Command types (intent to change system state)
#[derive(Debug, Clone)]
pub enum Command {
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

/// Strategy command
#[derive(Debug, Clone)]
pub enum StrategyCommand {
    Start {
        strategy_id: String,
        symbol: Pair,
        config: serde_json::Value,
    },
    Stop {
        strategy_id: String,
        reason: String,
    },
    Update {
        strategy_id: String,
        config: serde_json::Value,
    },
}

/// Level command
#[derive(Debug, Clone)]
pub enum LevelCommand {
    Create {
        level: LevelConfig,
    },
    Update {
        level_id: String,
        updates: LevelUpdates,
    },
    Delete {
        level_id: String,
    },
    Toggle {
        level_id: String,
        enabled: bool,
    },
}

/// Config command
#[derive(Debug, Clone)]
pub enum ConfigCommand {
    Update {
        path: String,
        value: serde_json::Value,
    },
    Reload {
        component: Option<String>, // None = reload all
    },
}

/// Control command
#[derive(Debug, Clone)]
pub enum ControlCommand {
    EnableTrading,
    DisableTrading,
    FlattenPositions {
        pair: Option<Pair>, // None = all pairs
    },
    KillSwitch, // Emergency stop
    Pause {
        component: String,
    },
    Resume {
        component: String,
    },
}
```

### 2.2 Command Bus Trait

**Location**: `libs/trading_core/src/commands/bus.rs` (new file)

**Purpose**: Abstract interface for command bus.

**Implementation**:
```rust
use tokio::sync::mpsc;

/// Command bus trait
pub trait CommandBus: Send + Sync + 'static {
    /// Submit a command (non-blocking)
    fn submit(&self, command: Command) -> Result<(), CommandBusError>;
    
    /// Submit a command and wait for result
    async fn submit_and_wait(
        &self,
        command: Command,
    ) -> Result<CommandResult, CommandBusError>;
    
    /// Subscribe to commands
    fn subscribe(&self) -> mpsc::Receiver<Command>;
}

/// Command result
#[derive(Debug)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Default implementation using mpsc channel
pub struct MpscCommandBus {
    tx: mpsc::Sender<Command>,
}

impl MpscCommandBus {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }
}

impl CommandBus for MpscCommandBus {
    fn submit(&self, command: Command) -> Result<(), CommandBusError> {
        self.tx.try_send(command)
            .map_err(|_| CommandBusError::ChannelFull)?;
        Ok(())
    }
    
    async fn submit_and_wait(
        &self,
        command: Command,
    ) -> Result<CommandResult, CommandBusError> {
        // Implementation with response channel
        // ...
    }
    
    fn subscribe(&self) -> mpsc::Receiver<Command> {
        // Create new receiver (each subscriber gets own copy)
        // ...
    }
}

#[derive(Debug)]
pub enum CommandBusError {
    ChannelFull,
    ChannelClosed,
    Timeout,
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

**Location**: `src/gateway/src/event_gateway.rs`

**Purpose**: Subscribe to internal Event Bus, filter/aggregate, serialize, publish to external transports.

**Implementation**:
```rust
use trading_core::events::{Event, EventBus};
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct EventGateway {
    event_bus: Arc<dyn EventBus>,
    config: GatewayConfig,
    transports: Vec<Box<dyn EventTransport>>,
}

impl EventGateway {
    pub fn new(
        event_bus: Arc<dyn EventBus>,
        config: GatewayConfig,
    ) -> Self {
        Self {
            event_bus,
            config,
            transports: Vec::new(),
        }
    }
    
    /// Add a transport (gRPC, Pub/Sub, IPC, etc.)
    pub fn add_transport(&mut self, transport: Box<dyn EventTransport>) {
        self.transports.push(transport);
    }
    
    /// Start gateway (subscribes to Event Bus, publishes to transports)
    pub async fn start(&mut self) -> Result<(), GatewayError> {
        let mut rx = self.event_bus.subscribe();
        
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Filter events based on config
                    if self.should_export(&event) {
                        // Serialize event
                        let serialized = self.serialize_event(&event)?;
                        
                        // Publish to all transports
                        for transport in &mut self.transports {
                            transport.publish(&serialized).await?;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!("Event gateway lagged, skipped {} events", skipped);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    error!("Event bus closed, gateway shutting down");
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    fn should_export(&self, event: &Event) -> bool {
        match event {
            // Always export important events
            Event::Order(_) | Event::Fill(_) | Event::Position(_) => true,
            Event::Error(_) | Event::Risk(_) => true,
            
            // Sample high-frequency events
            Event::MarketData(_) => {
                // Sample based on config
                // ...
            }
            
            // Always export slow events
            Event::Config(_) | Event::Health(_) => true,
            
            _ => true,
        }
    }
    
    // Serialization now handled by DTP Serializer
    // Gateway wraps events in DTP envelope before publishing
}

pub trait EventTransport: Send + Sync {
    /// Publish DTP envelope (transport handles wire format)
    async fn publish_dtp(&mut self, envelope: &DtpEnvelope) -> Result<(), GatewayError>;
}
```

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

### DTP Message Envelope

**Location**: `libs/trading_core/src/dtp/mod.rs` (new module)

```rust
/// Data Transfer Protocol Message Envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtpEnvelope {
    /// Protocol version (for backward compatibility)
    pub version: u8,
    
    /// Message type (Event or Command)
    pub message_type: MessageType,
    
    /// Message ID (unique identifier)
    pub message_id: String,
    
    /// Correlation ID (for request/response tracking)
    pub correlation_id: Option<String>,
    
    /// Source component
    pub source: String,
    
    /// Destination/routing hint (optional)
    pub destination: Option<String>,
    
    /// Timestamp (nanoseconds since epoch)
    pub timestamp: u64,
    
    /// Serialization format (protobuf, json, msgpack)
    pub format: SerializationFormat,
    
    /// Payload (serialized event or command)
    pub payload: Vec<u8>,
    
    /// Metadata (key-value pairs for extensibility)
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Event,
    Command,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerializationFormat {
    Protobuf,
    Json,
    MessagePack,
}

/// DTP Serializer (creates envelope from internal types)
pub struct DtpSerializer {
    version: u8,
    source: String,
}

impl DtpSerializer {
    pub fn serialize_event(
        &self,
        event: &Event,
        format: SerializationFormat,
    ) -> Result<DtpEnvelope, DtpError> {
        // Serialize event to payload
        let payload = match format {
            SerializationFormat::Protobuf => self.serialize_protobuf(event)?,
            SerializationFormat::Json => self.serialize_json(event)?,
            SerializationFormat::MessagePack => self.serialize_msgpack(event)?,
        };
        
        // Create envelope
        Ok(DtpEnvelope {
            version: self.version,
            message_type: MessageType::Event,
            message_id: generate_message_id(),
            correlation_id: None,
            source: self.source.clone(),
            destination: None,
            timestamp: current_timestamp_nanos(),
            format,
            payload,
            metadata: HashMap::new(),
        })
    }
    
    pub fn serialize_command(
        &self,
        command: &Command,
        format: SerializationFormat,
    ) -> Result<DtpEnvelope, DtpError> {
        // Similar implementation for commands
        // ...
    }
}

/// DTP Deserializer (extracts internal types from envelope)
pub struct DtpDeserializer;

impl DtpDeserializer {
    pub fn deserialize_event(
        &self,
        envelope: &DtpEnvelope,
    ) -> Result<Event, DtpError> {
        match envelope.format {
            SerializationFormat::Protobuf => self.deserialize_protobuf(&envelope.payload),
            SerializationFormat::Json => self.deserialize_json(&envelope.payload),
            SerializationFormat::MessagePack => self.deserialize_msgpack(&envelope.payload),
        }
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

1. **Internal Bus Remains Zero-Copy**: DTP envelope is only created at Gateway boundary
2. **Gateway Creates DTP Envelope**: Adds metadata, version, correlation IDs
3. **Transports Handle Wire Format**: gRPC uses Protobuf, Pub/Sub uses JSON, IPC uses MessagePack
4. **Consumers Deserialize DTP**: Extract envelope, deserialize payload based on format

### Updated Event Gateway with DTP

**Location**: `src/gateway/src/event_gateway.rs`

```rust
use trading_core::dtp::{DtpSerializer, DtpEnvelope, SerializationFormat};

pub struct EventGateway {
    event_bus: Arc<dyn EventBus>,
    dtp_serializer: DtpSerializer,
    transports: Vec<Box<dyn EventTransport>>,
}

impl EventGateway {
    pub fn new(
        event_bus: Arc<dyn EventBus>,
        config: GatewayConfig,
    ) -> Self {
        Self {
            event_bus,
            dtp_serializer: DtpSerializer::new(
                1, // version
                "trading_system".to_string(),
            ),
            transports: Vec::new(),
        }
    }
    
    pub async fn start(&mut self) -> Result<(), GatewayError> {
        let mut rx = self.event_bus.subscribe();
        
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if self.should_export(&event) {
                        // Wrap in DTP envelope
                        let dtp_message = self.dtp_serializer.serialize_event(
                            &event,
                            SerializationFormat::Json, // or based on transport
                        )?;
                        
                        // Publish to all transports
                        for transport in &mut self.transports {
                            transport.publish_dtp(&dtp_message).await?;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!("Event gateway lagged, skipped {} events", skipped);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    error!("Event bus closed, gateway shutting down");
                    break;
                }
            }
        }
        
        Ok(())
    }
}

pub trait EventTransport: Send + Sync {
    /// Publish DTP envelope (transport handles wire format)
    async fn publish_dtp(&mut self, envelope: &DtpEnvelope) -> Result<(), GatewayError>;
}
```

### Potential Concerns & Mitigations

**Concern 1: Overhead**
- **Mitigation**: Envelope only created at Gateway boundary (not internal). Internal bus remains zero-copy. Overhead is minimal compared to network latency.

**Concern 2: Complexity**
- **Mitigation**: DTP is simple (just an envelope). Benefits outweigh complexity. Makes adding new transports easier.

**Concern 3: Serialization Performance**
- **Mitigation**: Choose format based on transport needs. Protobuf for gRPC, JSON for Pub/Sub, MessagePack for IPC. Internal bus doesn't serialize.

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
        // Serialize DTP envelope to JSON
        let json = serde_json::to_vec(envelope)?;
        
        // Publish to NATS (envelope is self-contained)
        for topic in &self.topics {
            self.client.publish(topic, json).await?;
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
        // Serialize DTP envelope to MessagePack (compact, fast)
        let msgpack = rmp_serde::to_vec(envelope)?;
        
        // Write to Unix socket
        self.socket.write_all(&msgpack).await?;
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
        event_bus: Arc<dyn EventBus>, // NEW
    ) -> Self {
        Self {
            exchange_update_tx,
            event_bus, // NEW
        }
    }
    
    pub async fn publish_order(&self, order: &Order) -> Result<(), EngineError> {
        // Existing: publish to broadcast channel (for backward compatibility)
        self.exchange_update_tx.send(ExchangeUpdate::Order(order.clone()))?;
        
        // NEW: publish to Event Bus
        let event = Event::Order(OrderEvent::Placed {
            order: order.clone(),
            timestamp: order.internal_created_at,
        });
        self.event_bus.publish(event)?;
        
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
    event_bus: Arc<dyn EventBus>, // NEW
    // ... existing fields
}

impl StrategyExecutor {
    pub async fn run(&mut self) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                // NEW: Handle commands from Command Bus
                cmd = self.command_rx.recv() => {
                    if let Some(command) = cmd {
                        self.handle_command(command).await?;
                    }
                }
                // ... existing event handling
            }
        }
    }
    
    async fn handle_command(&self, cmd: Command) -> Result<(), EngineError> {
        match cmd {
            Command::Trade(trade_cmd) => {
                // Execute trade
                let result = self.executor.execute_trade_command(trade_cmd).await?;
                
                // Publish event
                let event = Event::Order(OrderEvent::Placed { ... });
                self.event_bus.publish(event)?;
            }
            Command::Strategy(strategy_cmd) => {
                // Handle strategy command
                // ...
            }
            _ => {}
        }
        Ok(())
    }
}
```

### 5.3 Update Runner

**Location**: `src/runner/src/lib.rs`

**Changes**: Initialize Event Bus, Command Bus, Gateway instead of direct gRPC.

```rust
pub async fn strategy_runner(...) -> Result<(), EngineError> {
    // ... existing initialization ...
    
    // NEW: Create Event Bus and Command Bus
    let (event_bus, _) = BroadcastEventBus::new(10000);
    let (command_bus, command_rx) = MpscCommandBus::new(1000);
    
    // NEW: Create Gateway
    let mut gateway = EventGateway::new(Arc::new(event_bus.clone()), gateway_config);
    gateway.add_transport(Box::new(GrpcEventTransport::new(...)));
    gateway.add_transport(Box::new(NatsEventTransport::new(...)));
    
    let mut command_gateway = CommandGateway::new(
        Arc::new(command_bus.clone()),
        authenticator,
        validator,
    );
    command_gateway.add_transport(Box::new(GrpcCommandTransport::new(...)));
    
    // Pass Event Bus and Command Bus to engines
    // ...
    
    // Start Gateway
    tokio::spawn(async move {
        gateway.start().await;
    });
    
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

### Phase 1: Event Bus Foundation (Week 1-2)
- [ ] Create `trading_core::events` module
- [ ] Define all event types
- [ ] Implement `EventBus` trait and `BroadcastEventBus`
- [ ] Update `ExchangeStatePublisher` to publish events
- [ ] Update `MarketDataStatePublisher` to publish events
- [ ] Add unit tests

### Phase 2: Command Bus Foundation (Week 2-3)
- [ ] Create `trading_core::commands` module
- [ ] Define all command types
- [ ] Implement `CommandBus` trait and `MpscCommandBus`
- [ ] Update `StrategyExecutor` to subscribe to commands
- [ ] Add unit tests

### Phase 3: Gateway Layer (Week 3-5)
- [ ] Create `gateway` crate
- [ ] Implement DTP core library (`libs/trading_core/src/dtp/`)
  - [ ] Define `DtpEnvelope` structure
  - [ ] Implement `DtpSerializer` and `DtpDeserializer`
  - [ ] Add versioning support
- [ ] Implement `EventGateway` (with DTP integration)
- [ ] Implement `CommandGateway` (with DTP integration)
- [ ] Add authentication and validation
- [ ] Add configuration
- [ ] Add integration tests

### Phase 4: gRPC Transport (Week 5-6)
- [ ] Implement `GrpcEventTransport` (with DTP support)
- [ ] Implement `GrpcCommandTransport` (with DTP support)
- [ ] Update gRPC proto definitions (include DTP envelope)
- [ ] Migrate existing gRPC service to Gateway
- [ ] Test with existing TUI

### Phase 5: Additional Transports (Week 6-7)
- [ ] Implement `NatsEventTransport` (Pub/Sub with DTP)
- [ ] Implement `IpcEventTransport` (Unix sockets with DTP/MessagePack)
- [ ] Add HTTP transport (optional, with DTP)
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
- [ ] Event Bus and Command Bus fully functional (zero-copy, <1μs)
- [ ] DTP core library implemented and tested
- [ ] Gateway layer working with DTP and gRPC
- [ ] Existing TUI works without changes
- [ ] Can add new transports without modifying core
- [ ] Internal latency remains <1μs
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

**Next Steps**: Review proposal, approve architecture, begin Phase 1 implementation.

