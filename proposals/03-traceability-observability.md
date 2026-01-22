# Proposal: Traceability & Observability

**Version**: 2.0  
**Date**: December 2024  
**Status**: **Ready for Implementation**  
**Priority**: **CRITICAL** - Must be implemented BEFORE Proposal 04 (Communication Layer)

**Prerequisites**: None (this proposal defines types that Proposal 04 will use)

**Implementation Order**: This proposal (03) must be implemented **BEFORE** Proposal 04 (Communication Layer)

---

## Executive Summary

This proposal defines a comprehensive traceability and observability system for the trading system using a **hybrid approach**: custom lightweight tracing for hot path operations (<1μs) and OpenTelemetry for cold path operations (<10μs). This foundational layer enables full request/response correlation, event chain tracking, performance monitoring, and production debugging.

**Approach**: Hybrid (Custom Hot Path + OpenTelemetry Cold Path)

- Custom lightweight tracing for hot path (market data, <1μs operations)
- OpenTelemetry for cold path (commands, events, state, <10μs operations)
- Correlation bridge to link hot and cold paths
- Unified `TraceContext` enum for networking layer integration

**Key Objectives:**

1. **Request Tracing** - Trace every external request through the entire system
2. **Event Correlation** - Correlate commands, events, and responses
3. **Performance Monitoring** - Track latency at every level
4. **Error Tracking** - Full error context with trace chains
5. **Distributed Tracing** - Export traces to Jaeger/Prometheus
6. **Structured Logging** - Logs with trace context for correlation

---

## Part 1: Current State Assessment

### What Exists

- ✅ Basic `tracing` crate integration
- ✅ Log levels (trace, debug, info, warn, error)
- ✅ File logging for CLI (`cli_debug.log`)
- ✅ Simple audit logging for config changes

### What's Missing

- ❌ No correlation IDs across services
- ❌ No distributed tracing
- ❌ No request/response correlation
- ❌ No span tracking
- ❌ No structured context propagation
- ❌ No performance tracing (latency, queue depths)
- ❌ No integration with observability tools (Jaeger, Zipkin, etc.)

---

## Part 2: What We're Tracing

### 2.1 Operation Types in the System

Based on architecture analysis, we have **fundamentally different types of operations**:

#### A. **Request/Response Operations** (Synchronous)
- **gRPC Requests** → Commands → Responses
  - `SetTradingEnabled` → Enable/disable trading → Response
  - `CreateLevel` → Create level → Response
  - `GetMonitoringData` → Query state → Response
- **Characteristics**:
  - Clear request/response boundaries
  - Single correlation ID
  - Can wait for response
  - Error handling needed

#### B. **Event-Driven Operations** (Asynchronous)
- **Market Data Events** → Processing → State Updates
  - Orderbook update → Indicator calculation → Trading decision
  - Trade event → Position update → Risk check
- **Characteristics**:
  - No request/response
  - Event chains (A → B → C)
  - Multiple consumers
  - Fire-and-forget

#### C. **State Management Operations** (Continuous)
- **Exchange State Updates** → Reconciliation → Publishing
  - WebSocket update → State update → Channel broadcast
  - REST reconciliation → State sync → Channel broadcast
- **Characteristics**:
  - Continuous stream
  - No clear start/end
  - Periodic operations
  - Background processes

#### D. **Command Execution** (Async with Callbacks)
- **Trading Commands** → Order Placement → Fill Events
  - Place order command → Exchange API → Fill event → Position update
- **Characteristics**:
  - Command initiates flow
  - Multiple async callbacks
  - Event correlation needed
  - Long-running operations

#### E. **Streaming Operations** (High-Frequency)
- **Market Data Streams** → Real-time Processing
  - Orderbook updates (100s/sec)
  - Trade ticks (1000s/sec)
- **Characteristics**:
  - Ultra-high frequency
  - Low latency critical (<1μs)
  - Sampling required
  - Performance-sensitive

#### F. **Background Operations** (Periodic)
- **Reconciliation** → State Sync
  - Periodic REST calls → State comparison → Updates
- **Characteristics**:
  - Scheduled operations
  - No external trigger
  - Background processes
  - Long-running

### 2.2 Data Flow Patterns

#### Pattern 1: Request → Command → Event → Response
```
gRPC Request (correlation_id=123)
  → Command Router
    → Trading Engine
      → Exchange Engine
        → Exchange API
      → Order Event (correlation_id=123)
    → Response (correlation_id=123)
```

#### Pattern 2: Event → Processing → Event Chain
```
Orderbook Update (no correlation)
  → Indicator Engine
    → Indicator Value Event
      → Trading Engine
        → Order Event
          → Fill Event
            → Position Event
```

#### Pattern 3: WebSocket → State → Broadcast
```
Exchange WebSocket
  → ExchangeStateManager
    → State Update
      → Broadcast Channel
        → Trading Engine (subscriber 1)
        → Risk Engine (subscriber 2)
        → TUI (subscriber 3)
```

#### Pattern 4: Command → Async Callbacks
```
Place Order Command (correlation_id=456)
  → Exchange API
    → [Async] Execution Report (order_id=789)
      → Fill Event (order_id=789, correlation_id=456)
        → Position Update
```

### 2.3 Tracing Requirements by Operation Type

**Different operation types need different tracing information:**

1. **Request/Response**: Need correlation ID, request ID, response ID, latency
2. **Event Chains**: Need trace ID to link events, event IDs, parent event IDs
3. **State Updates**: Need state version, update source, timestamp
4. **Commands**: Need command ID, correlation ID, execution status
5. **Streams**: Need sampling, aggregate metrics, not individual traces
6. **Background**: Need operation ID, schedule ID, execution time

**Key Insight**: A single `TraceContext` type is not sufficient. We need:
- **Hot path types** (custom, minimal overhead) for high-frequency operations
- **Cold path types** (OpenTelemetry) for commands, events, state
- **Unified interface** (`TraceContext` enum) for networking layer

---


## Part 3: Library Structure

### Library: `libs/trading_trace`

**Purpose**: Reusable tracing infrastructure for hot and cold path operations.

**Structure**:
```
libs/trading_trace/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── hot.rs          # HotTraceContext
    ├── cold.rs          # ColdTraceContext (OpenTelemetry wrapper)
    ├── context.rs       # TraceContext enum
    ├── correlation.rs   # CorrelationId
    ├── bridge.rs        # TraceBridge
    └── exporter.rs      # OpenTelemetry exporters (Jaeger, Prometheus)
```

**Dependencies**:
- `trading_core` - for domain types (Pair, OrderId, etc. used in trace tags)
- `opentelemetry` - OpenTelemetry SDK
- `opentelemetry-jaeger` - Jaeger exporter
- `opentelemetry-prometheus` - Prometheus exporter
- `tracing` - Rust tracing crate

**Usage**:
```rust
use trading_trace::{HotTraceContext, ColdTraceContext, TraceContext, CorrelationId};
```

**Why Separate Library**:
- Reusable across multiple trading systems or projects
- Clear boundaries and single responsibility
- Independent versioning
- Easier to test in isolation

---

## Part 4: Type Definitions

### 4.1 Hot Path Types (Custom, Minimal Overhead)

#### `HotTraceContext`
**Location**: `libs/trading_trace/src/hot.rs`

```rust
/// Minimal trace context for hot path (market data, high-frequency operations)
/// Copy semantics, no allocations, minimal overhead (~10-30ns)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotTraceContext {
    /// Root trace ID (generated at event entry point)
    pub trace_id: u64,
    
    /// Current span ID
    pub span_id: u64,
    
    /// Parent span ID (for nested operations)
    pub parent_span_id: Option<u64>,
    
    /// Correlation ID (links to cold path commands)
    pub correlation_id: Option<u64>,
}

impl HotTraceContext {
    /// Create new root trace context
    pub fn new() -> Self {
        Self {
            trace_id: generate_trace_id(),
            span_id: generate_span_id(),
            parent_span_id: None,
            correlation_id: None,
        }
    }
    
    /// Create child span (minimal overhead)
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: generate_span_id(),
            parent_span_id: Some(self.span_id),
            correlation_id: self.correlation_id,
        }
    }
    
    /// Link to cold path via correlation ID
    pub fn with_correlation_id(mut self, correlation_id: u64) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}

/// Sampling: only trace 1/100 events (configurable)
pub fn should_trace_hot(sample_rate: u32) -> bool {
    thread_rng().gen::<u32>() % sample_rate == 0
}
```

**Characteristics**:
- Overhead: ~10-30ns per operation
- Sampling: 1/100 (configurable)
- Copy semantics (no allocations)
- Used for: Market data events, high-frequency operations

### 4.2 Cold Path Types (OpenTelemetry)

#### `ColdTraceContext`
**Location**: `libs/trading_trace/src/cold.rs`

```rust
use opentelemetry::trace::{Span, Tracer};

/// Full trace context for cold path (commands, events, state updates)
/// Uses OpenTelemetry for rich observability (~100-500ns overhead)
pub struct ColdTraceContext {
    /// OpenTelemetry span (full metadata, tags, logs)
    pub span: Span,
    
    /// Correlation ID (links to hot path or other operations)
    pub correlation_id: Option<u64>,
    
    /// Trace ID (extracted from span for convenience)
    pub trace_id: u64,
    
    /// Span ID (extracted from span for convenience)
    pub span_id: u64,
}

impl ColdTraceContext {
    /// Create new root trace context
    pub fn new_root(tracer: &Tracer, operation: &str, correlation_id: Option<u64>) -> Self {
        let span = tracer.start(operation);
        let trace_id = extract_trace_id(&span);
        let span_id = extract_span_id(&span);
        
        if let Some(corr_id) = correlation_id {
            span.set_attribute("correlation_id", corr_id as i64);
        }
        
        Self {
            span,
            correlation_id,
            trace_id,
            span_id,
        }
    }
    
    /// Create child span
    pub fn child(&self, tracer: &Tracer, operation: &str) -> Self {
        let span = tracer.start_with_context(operation, &self.span.context());
        let trace_id = self.trace_id;
        let span_id = extract_span_id(&span);
        
        Self {
            span,
            correlation_id: self.correlation_id,
            trace_id,
            span_id,
        }
    }
    
    /// End span
    pub fn end(self) {
        self.span.end();
    }
}

impl Drop for ColdTraceContext {
    fn drop(&mut self) {
        // Ensure span is ended if not explicitly ended
        // OpenTelemetry handles this, but we ensure it
    }
}
```

**Characteristics**:
- Overhead: ~100-500ns per operation
- Full metadata: Tags, logs, attributes
- 100% coverage (no sampling)
- Used for: Commands, events, state updates, gRPC handlers

### 4.3 Unified Trace Context (For Networking Layer)

#### `TraceContext`
**Location**: `libs/trading_trace/src/context.rs`

```rust
/// Unified trace context for networking layer
/// Can represent hot path, cold path, or no tracing
#[derive(Debug, Clone)]
pub enum TraceContext {
    /// Hot path trace (minimal overhead)
    Hot(HotTraceContext),
    
    /// Cold path trace (full observability)
    Cold(ColdTraceContext),
    
    /// No tracing (for backward compatibility)
    None,
}

impl TraceContext {
    /// Extract correlation ID if present
    pub fn correlation_id(&self) -> Option<u64> {
        match self {
            TraceContext::Hot(ctx) => ctx.correlation_id,
            TraceContext::Cold(ctx) => ctx.correlation_id,
            TraceContext::None => None,
        }
    }
    
    /// Convert to hot trace context (if hot path)
    pub fn as_hot(&self) -> Option<&HotTraceContext> {
        match self {
            TraceContext::Hot(ctx) => Some(ctx),
            _ => None,
        }
    }
    
    /// Convert to cold trace context (if cold path)
    pub fn as_cold(&self) -> Option<&ColdTraceContext> {
        match self {
            TraceContext::Cold(ctx) => Some(ctx),
            _ => None,
        }
    }
}
```

**Characteristics**:
- Unified interface for networking layer
- Can represent both hot and cold paths
- Backward compatible (None variant)
- Used in: DTP Envelope, Gateway

### 4.4 Correlation Types

#### `CorrelationId`
**Location**: `libs/trading_trace/src/correlation.rs`

```rust
/// Correlation ID type (u64)
/// Links related operations across hot and cold paths
pub type CorrelationId = u64;

/// Generate new correlation ID
pub fn generate_correlation_id() -> CorrelationId {
    // Use monotonic counter or UUID-based approach
    // For now: simple counter (can be enhanced)
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
```

**Where Used**:
- Commands: `Command { correlation_id: CorrelationId, ... }`
- Responses: `CommandResult { correlation_id: CorrelationId, ... }`
- Hot Trace Context: `HotTraceContext { correlation_id: Option<CorrelationId>, ... }`
- Cold Trace Context: `ColdTraceContext { correlation_id: Option<CorrelationId>, ... }`

**Purpose**: Links hot path events to cold path commands

### 4.5 Trace Bridge

#### `TraceBridge`
**Location**: `libs/trading_trace/src/bridge.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Bridge between hot and cold path traces
/// Links traces via correlation IDs for unified observability
pub struct TraceBridge {
    /// Map hot trace ID to cold trace ID
    hot_to_cold: Arc<RwLock<HashMap<u64, u64>>>,
    
    /// Map correlation ID to both hot and cold trace IDs
    correlation_map: Arc<RwLock<HashMap<CorrelationId, (Option<u64>, Option<u64>)>>>,
}

impl TraceBridge {
    pub fn new() -> Self {
        Self {
            hot_to_cold: Arc::new(RwLock::new(HashMap::new())),
            correlation_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Link hot path trace to cold path trace
    pub fn link(&self, hot_trace_id: u64, cold_trace_id: u64) {
        let mut map = self.hot_to_cold.write().unwrap();
        map.insert(hot_trace_id, cold_trace_id);
    }
    
    /// Register correlation ID with trace IDs
    pub fn register_correlation(
        &self,
        correlation_id: CorrelationId,
        hot_trace_id: Option<u64>,
        cold_trace_id: Option<u64>,
    ) {
        let mut map = self.correlation_map.write().unwrap();
        map.insert(correlation_id, (hot_trace_id, cold_trace_id));
    }
    
    /// Get cold trace ID for hot trace ID
    pub fn get_cold_trace(&self, hot_trace_id: u64) -> Option<u64> {
        let map = self.hot_to_cold.read().unwrap();
        map.get(&hot_trace_id).copied()
    }
    
    /// Get both trace IDs for correlation ID
    pub fn get_by_correlation(
        &self,
        correlation_id: CorrelationId,
    ) -> Option<(Option<u64>, Option<u64>)> {
        let map = self.correlation_map.read().unwrap();
        map.get(&correlation_id).copied()
    }
}
```

**Where Used**:
- Gateway (links hot and cold paths)
- Export (exports linked traces together)

**Purpose**: Links hot path traces to cold path traces for unified observability

---

## Part 5: Integration Points

### 6.1 Networking Layer Integration (Proposal 04)

#### DTP Envelope
```rust
// libs/trading_core/src/dtp/mod.rs
pub struct DtpEnvelope {
    // ... existing fields ...
    pub trace_context: TraceContext,  // Unified (can be Hot, Cold, or None)
}
```

#### Hot Events
```rust
// libs/trading_core/src/events/hot.rs
pub enum HotEvent {
    Orderbook {
        // ... existing fields ...
        trace_context: Option<HotTraceContext>,  // Optional, sampled
    },
    Trade {
        // ... existing fields ...
        trace_context: Option<HotTraceContext>,  // Optional, sampled
    },
    MarkPrice {
        // ... existing fields ...
        trace_context: Option<HotTraceContext>,  // Optional, sampled
    },
    FundingRate {
        // ... existing fields ...
        trace_context: Option<HotTraceContext>,  // Optional, sampled
    },
}
```

#### Cold Events
```rust
// libs/trading_core/src/events/cold.rs
pub enum ColdEvent {
    Order(OrderEvent {
        // ... existing fields ...
        trace_context: Option<ColdTraceContext>,  // Optional, full
    }),
    Fill(FillEvent {
        // ... existing fields ...
        trace_context: Option<ColdTraceContext>,  // Optional, full
    }),
    Position(PositionEvent {
        // ... existing fields ...
        trace_context: Option<ColdTraceContext>,  // Optional, full
    }),
    // ... other event types ...
}
```

#### Commands
```rust
// libs/trading_core/src/commands/mod.rs
pub struct Command {
    // ... existing fields ...
    pub trace_context: Option<ColdTraceContext>,  // Optional, full
    pub correlation_id: CorrelationId,  // Required for request/response
}
```

### 6.2 Current System Integration

#### Market Data Channels
```rust
// libs/trading_core/src/channels/market_data.rs
pub enum MarketDataUpdate {
    Orderbook {
        orderbook: StructuredOrderbook,
        trace_context: Option<HotTraceContext>,  // NEW
    },
    Trade {
        trade: StructuredPublicTrade,
        trace_context: Option<HotTraceContext>,  // NEW
    },
}
```

#### Exchange Channels
```rust
// libs/trading_core/src/channels/exchange.rs
pub enum ExchangeUpdate {
    Order(Order),
    Balance(Balance),
    Position(Position),
    Trade(Trade),
    // All will carry trace_context: Option<ColdTraceContext>  // NEW
    // Note: This requires updating the enum to include trace_context
    // For backward compatibility, can use wrapper struct
}
```

#### Indicator Channels
```rust
// libs/trading_core/src/channels/indicators.rs
pub struct IndicatorUpdate {
    // ... existing fields ...
    trace_context: Option<ColdTraceContext>,  // NEW
}
```

#### gRPC Handlers
```rust
// src/runner/src/grpc/service/*.rs
// All gRPC handlers will:
// 1. Extract trace context from request (if present)
// 2. Create cold trace context
// 3. Propagate through command/event processing
// 4. Include in response
```

---

## Part 6: Usage by Operation Type

### 7.1 Request/Response (gRPC)
- **Type**: `ColdTraceContext`
- **Where**: gRPC handlers, commands, responses
- **Coverage**: 100%
- **Overhead**: ~100-500ns
- **Example**:
```rust
// gRPC handler
let tracer = global::tracer("trading_system");
let correlation_id = generate_correlation_id();
let span = tracer.start("grpc.set_trading_enabled")
    .with_attribute("correlation_id", correlation_id as i64)
    .with_attribute("enabled", enabled);
    
let trace_ctx = ColdTraceContext::new_root(&tracer, "grpc.set_trading_enabled", Some(correlation_id));
// ... execute ...
trace_ctx.end();
```

### 7.2 Events (Market Data)
- **Type**: `HotTraceContext`
- **Where**: Hot events, market data channels
- **Coverage**: 1% (sampled)
- **Overhead**: ~10-30ns
- **Example**:
```rust
// Market data event
if should_trace_hot(100) {
    let trace_ctx = HotTraceContext::new();
    let event = HotEvent::Orderbook {
        // ... fields ...
        trace_context: Some(trace_ctx),
    };
    // ... publish ...
}
```

### 7.3 Events (State Changes)
- **Type**: `ColdTraceContext`
- **Where**: Cold events, exchange channels, indicator channels
- **Coverage**: 100%
- **Overhead**: ~100-500ns
- **Example**:
```rust
// State change event
let tracer = global::tracer("trading_system");
let trace_ctx = ColdTraceContext::new_root(&tracer, "event.order_placed", None)
    .with_attribute("order_id", order_id)
    .with_attribute("pair", pair);
    
let event = ColdEvent::Order(OrderEvent {
    // ... fields ...
    trace_context: Some(trace_ctx),
});
// ... publish ...
```

### 7.4 Commands
- **Type**: `ColdTraceContext`
- **Where**: Command router, command handlers
- **Coverage**: 100%
- **Overhead**: ~100-500ns
- **Example**:
```rust
// Command execution
let tracer = global::tracer("trading_system");
let correlation_id = generate_correlation_id();
let trace_ctx = ColdTraceContext::new_root(&tracer, "command.place_order", Some(correlation_id));
let command = Command {
    // ... fields ...
    trace_context: Some(trace_ctx),
    correlation_id,
};
// ... execute ...
```

### 7.5 Background Operations
- **Type**: `ColdTraceContext`
- **Where**: Reconciliation, scheduled tasks
- **Coverage**: 100%
- **Overhead**: ~100-500ns
- **Example**:
```rust
// Background operation
let tracer = global::tracer("trading_system");
let trace_ctx = ColdTraceContext::new_root(&tracer, "background.reconciliation", None)
    .with_attribute("schedule_id", schedule_id);
// ... execute ...
trace_ctx.end();
```

---

## Part 7: Implementation Phases

### Phase 1: Foundation - Type Definitions (Week 1)

**Location**: `libs/trading_trace/src/`

- [ ] Create `trace` module structure
- [ ] Implement `HotTraceContext` (`hot.rs`)
- [ ] Implement `ColdTraceContext` (`cold.rs`)
- [ ] Implement `TraceContext` unified enum (`context.rs`)
- [ ] Implement `CorrelationId` type (`correlation.rs`)
- [ ] Implement `TraceBridge` (`bridge.rs`)
- [ ] Add unit tests
- [ ] Document design decisions

**Deliverables**:
- All trace types defined
- Basic functionality working
- Unit tests passing

### Phase 2: OpenTelemetry Integration (Week 2)

**Location**: `libs/trading_trace/src/cold.rs`

- [ ] Add OpenTelemetry dependencies
- [ ] Integrate OpenTelemetry tracer
- [ ] Implement `ColdTraceContext` with OpenTelemetry
- [ ] Add Jaeger exporter
- [ ] Add Prometheus metrics
- [ ] Test export functionality

**Deliverables**:
- OpenTelemetry integrated
- Traces exportable to Jaeger
- Metrics exportable to Prometheus

### Phase 3: Hot Path Integration (Week 3)

**Location**: Various (market data, hot events)

- [ ] Add `HotTraceContext` to market data channels
- [ ] Add sampling mechanism
- [ ] Add metrics collection
- [ ] Test with high-frequency operations
- [ ] Benchmark overhead

**Deliverables**:
- Hot path tracing working
- Sampling functional
- Overhead validated (<1% impact)

### Phase 4: Cold Path Integration (Week 4-5)

**Location**: Various (commands, events, state, gRPC)

- [ ] Add `ColdTraceContext` to gRPC handlers
- [ ] Add `ColdTraceContext` to commands
- [ ] Add `ColdTraceContext` to cold events
- [ ] Add `ColdTraceContext` to exchange channels
- [ ] Add `ColdTraceContext` to indicator channels
- [ ] Test end-to-end tracing

**Deliverables**:
- Cold path tracing working
- All operation types traced
- End-to-end traces visible in Jaeger

### Phase 5: Correlation Bridge (Week 6)

**Location**: `libs/trading_trace/src/bridge.rs`

- [ ] Implement correlation ID propagation
- [ ] Link hot and cold paths
- [ ] Unified export (linked traces)
- [ ] Test correlation

**Deliverables**:
- Hot and cold paths linked
- Unified observability
- Correlation working

### Phase 6: Networking Layer Integration (Week 7)

**Location**: Integration with Proposal 04

- [ ] Add `TraceContext` to DTP Envelope
- [ ] Add trace context extraction/injection in Gateway
- [ ] Test with networking layer
- [ ] End-to-end validation

**Deliverables**:
- Trace context in DTP envelopes
- Gateway handles trace context
- Full integration working

### Phase 7: Logging Enhancement (Week 8)

**Location**: All services

- [ ] Update all log statements to include trace context
- [ ] Add structured logging format
- [ ] Add log aggregation (Loki, ELK, etc.)
- [ ] Create log correlation tools

**Deliverables**:
- All logs include trace context
- Logs correlated with traces
- Log aggregation working

---

## Part 8: Success Criteria

### Hot Path
- [ ] Overhead: <1% (with sampling)
- [ ] Latency: <1μs maintained
- [ ] Coverage: 1% of events traced (sampled)
- [ ] Metrics: 100% coverage

### Cold Path
- [ ] Overhead: <5%
- [ ] Latency: <10μs maintained
- [ ] Coverage: 100% of operations traced
- [ ] Full observability in Jaeger

### Overall
- [ ] Every external request has trace_id
- [ ] Every command has correlation_id
- [ ] Every event carries trace context (optional)
- [ ] Spans tracked for all major operations
- [ ] Traces exportable to Jaeger
- [ ] Metrics exportable to Prometheus
- [ ] Logs include trace context
- [ ] Can trace full request flow (gRPC → Command → Event → Response)
- [ ] Can trace event chains (Order → Fill → Position)
- [ ] Performance metrics available (latency, queue depths)
- [ ] Hot and cold paths linked via correlation IDs

---

## Part 9: Dependencies

### Requires
- OpenTelemetry crates (`opentelemetry`, `opentelemetry-jaeger`, `opentelemetry-prometheus`)
- `tracing` crate (already in use)
- Proposal 04 (Communication Layer) - for DTP envelope integration

### Enables
- Production debugging
- Performance optimization
- Compliance/audit trails
- Future observability enhancements

### Coordination with Proposal 04

**Critical**: This proposal must be implemented **BEFORE** Proposal 04 (Communication Layer) so that:
1. Trace types are defined and available
2. DTP Envelope can include `TraceContext` from the start
3. Hot/Cold events can include trace context from the start
4. Commands can include trace context from the start
5. No retrofitting needed

**Design Coordination**:
- Proposal 04 should design DTP/Events/Commands with `Option<TraceContext>` fields
- This proposal provides the `TraceContext` types
- Implementation can happen in parallel, but types must be defined first

### Implementation Order

**This proposal (03) comes FIRST, then Proposal 04 (Communication Layer):**

1. **Phase 0: Define Trace Types** (This Proposal, Week 1)
   - Define `HotTraceContext`, `ColdTraceContext`, `TraceContext` types
   - Define `CorrelationId`, `TraceBridge` types
   - Add to `libs/trading_trace/src/`
   - **No implementation needed yet** - just type definitions

2. **Phase 1: Design Networking with Trace Slots** (Proposal 04, Week 1)
   - Design DTP Envelope with `trace_context: TraceContext` field
   - Design Hot/Cold events with `trace_context: Option<HotTraceContext>` / `Option<ColdTraceContext>`
   - Design Commands with `trace_context: Option<ColdTraceContext>`
   - All optional, can be `None` initially

3. **Phase 2: Implement Networking** (Proposal 04, Week 2-9)
   - Implement Event Bus, Command Router, Gateway, DTP
   - Use `None` for trace context everywhere initially
   - Networking works independently

4. **Phase 3: Implement Full Traceability** (This Proposal, Week 10-15)
   - Implement trace context creation/propagation
   - Fill trace context slots in networking layer
   - Integrate OpenTelemetry
   - Add exporters

**Key Principle**: Define types first, design networking with slots, implement networking, then fill slots.

---

## Part 10: Type Summary

### Core Types

1. **`HotTraceContext`** - Hot path (market data, <1μs operations)
   - Location: `libs/trading_core/src/trace/hot.rs`
   - Overhead: ~10-30ns
   - Sampling: 1/100

2. **`ColdTraceContext`** - Cold path (commands, events, state, <10μs operations)
   - Location: `libs/trading_core/src/trace/cold.rs`
   - Overhead: ~100-500ns
   - Coverage: 100%

3. **`TraceContext`** - Unified enum (for networking layer)
   - Location: `libs/trading_core/src/trace/context.rs`
   - Variants: `Hot(HotTraceContext)`, `Cold(ColdTraceContext)`, `None`

4. **`CorrelationId`** - Links hot and cold paths
   - Location: `libs/trading_core/src/trace/correlation.rs`
   - Type: `u64`

5. **`TraceBridge`** - Links traces for export
   - Location: `libs/trading_core/src/trace/bridge.rs`

### Integration Points

- **DTP Envelope**: `trace_context: TraceContext` (unified)
- **Hot Events**: `trace_context: Option<HotTraceContext>`
- **Cold Events**: `trace_context: Option<ColdTraceContext>`
- **Commands**: `trace_context: Option<ColdTraceContext>` + `correlation_id: CorrelationId`
- **Channels**: `trace_context: Option<HotTraceContext>` or `Option<ColdTraceContext>`
- **gRPC**: `ColdTraceContext`

---

## Part 11: Migration Plan

### Overview

This section provides a detailed, phase-by-phase migration plan for implementing traceability across the entire system. The migration is designed to be **non-breaking** and **incremental**, allowing the system to continue operating during the transition.

### Migration Strategy: Phased with Dual Publishing

**Key Principle**: During migration, publish to both old and new channels/event buses simultaneously. This ensures:
- ✅ No breaking changes during transition
- ✅ Gradual migration of subscribers
- ✅ Easy rollback if issues arise
- ✅ Zero downtime migration

### Phase 1: Foundation - Type Definitions (Week 1)

**Goal**: Define trace types without breaking existing code

**Changes**:

1. **Create `libs/trading_trace` crate**
   ```bash
   cargo new --lib libs/trading_trace
   ```

2. **Define core types**:
   - `HotTraceContext` (`libs/trading_trace/src/hot.rs`)
   - `ColdTraceContext` (`libs/trading_trace/src/cold.rs`)
   - `TraceContext` enum (`libs/trading_trace/src/context.rs`)
   - `CorrelationId` (`libs/trading_trace/src/correlation.rs`)
   - `TraceBridge` (`libs/trading_trace/src/bridge.rs`)

3. **Add to workspace** (`Cargo.toml`):
   ```toml
   [workspace]
   members = [
       # ... existing members ...
       "libs/trading_trace",
   ]
   ```

4. **Export from library** (`libs/trading_trace/src/lib.rs`):
   ```rust
   pub mod hot;
   pub mod cold;
   pub mod context;
   pub mod correlation;
   pub mod bridge;

   pub use hot::HotTraceContext;
   pub use cold::ColdTraceContext;
   pub use context::TraceContext;
   pub use correlation::CorrelationId;
   pub use bridge::TraceBridge;
   ```

**Files Created**: ~5 files  
**Files Modified**: `Cargo.toml` (workspace)  
**Impact**: None (new code, not used yet)  
**Testing**: Unit tests for type definitions

### Phase 2: OpenTelemetry Integration (Week 2)

**Goal**: Integrate OpenTelemetry for cold path tracing

**Changes**:

1. **Add dependencies** (`libs/trading_trace/Cargo.toml`):
   ```toml
   [dependencies]
   opentelemetry = "0.21"
   opentelemetry-jaeger = "0.20"
   opentelemetry-prometheus = "0.13"
   opentelemetry_sdk = "0.21"
   tracing = "0.1"
   tracing-opentelemetry = "0.22"
   ```

2. **Implement `ColdTraceContext`**:
   - Wrap OpenTelemetry `Span`
   - Provide methods for adding tags, logs, events
   - Handle span lifecycle (start, end, error)

3. **Implement exporters**:
   - Jaeger exporter (for distributed tracing)
   - Prometheus exporter (for metrics)

4. **Add configuration**:
   - Sampling rate configuration
   - Exporter endpoint configuration
   - Service name configuration

**Files Created**: ~3 files  
**Files Modified**: `libs/trading_trace/src/cold.rs`, `libs/trading_trace/src/exporter.rs`  
**Impact**: None (not integrated into system yet)  
**Testing**: Unit tests, integration tests with Jaeger

### Phase 3: Hot Path Integration (Week 3)

**Goal**: Implement lightweight hot path tracing

**Changes**:

1. **Implement `HotTraceContext`**:
   - Minimal struct (timestamp, trace_id, span_id)
   - No heap allocations
   - Copy semantics

2. **Implement sampling logic**:
   - Configurable sampling rate (e.g., 1% for hot path)
   - Thread-local random number generator
   - Fast path (no sampling) vs sampled path

3. **Add to market data publishers**:
   - Update `MarketDataStatePublisher` to optionally create `HotTraceContext`
   - Sample at configurable rate (e.g., 1 in 100 orderbook updates)
   - Add trace context to `MarketDataUpdate` (optional field)

**Files Modified**:
- `libs/trading_trace/src/hot.rs` (implementation)
- `src/market_data_engine/src/state/publisher.rs` (add sampling)

**Impact**: Minimal (optional field, backward compatible)  
**Testing**: Unit tests, performance benchmarks (ensure <1μs overhead)

### Phase 4: Cold Path Integration (Week 4-5)

**Goal**: Integrate cold path tracing into state management and commands

**Changes**:

1. **Update Exchange State Publisher**:
   - Create `ColdTraceContext` for order/balance/position updates
   - Propagate trace context through state updates
   - Add trace context to `ExchangeUpdate` (optional field)

2. **Update Trading Engine**:
   - Create trace context for trading decisions
   - Propagate through order placement
   - Link to exchange state traces

3. **Update Command Handlers**:
   - Extract trace context from commands
   - Create child spans for command execution
   - Propagate to responses

4. **Update gRPC Handlers**:
   - Extract trace context from gRPC requests
   - Create root spans for external requests
   - Propagate to commands/events

**Files Modified**:
- `src/exchange_engine/src/state/publisher.rs`
- `src/trading_engine/src/state/subscriber.rs`
- `src/trading_engine/src/strategies/executor.rs`
- `src/runner/src/grpc/service/*.rs`

**Impact**: Moderate (optional fields, backward compatible)  
**Testing**: Integration tests, end-to-end trace verification

### Phase 5: Correlation Bridge (Week 6)

**Goal**: Link hot and cold path traces via correlation IDs

**Changes**:

1. **Implement `TraceBridge`**:
   - Store correlation between hot and cold traces
   - Query interface for linking traces
   - Cleanup old correlations

2. **Update event publishers**:
   - Generate correlation IDs for events that span hot/cold
   - Store in `TraceBridge`
   - Include in both hot and cold trace contexts

3. **Update trace exporters**:
   - Include correlation IDs in exported traces
   - Enable cross-path trace linking in Jaeger

**Files Created**: `libs/trading_trace/src/bridge.rs`  
**Files Modified**: Publishers, exporters  
**Impact**: Minimal (adds correlation, doesn't break existing)  
**Testing**: Integration tests for correlation

### Phase 6: Networking Layer Integration (Week 7)

**Goal**: Integrate trace context into DTP, events, and commands

**Prerequisite**: Proposal 04 (Communication Layer) must have trace context slots defined

**Changes**:

1. **Update DTP Envelope**:
   - Fill `trace_context: TraceContext` field (was `None`)
   - Serialize/deserialize trace context

2. **Update Hot Events**:
   - Fill `trace_context: Option<HotTraceContext>` fields
   - Propagate through hot event bus

3. **Update Cold Events**:
   - Fill `trace_context: Option<ColdTraceContext>` fields
   - Propagate through cold event bus

4. **Update Commands**:
   - Fill `trace_context: Option<ColdTraceContext>` field
   - Extract from external commands
   - Propagate to handlers

**Files Modified**: (From Proposal 04)
- `libs/trading_core/src/dtp/mod.rs`
- `libs/trading_core/src/events/hot.rs`
- `libs/trading_core/src/events/cold.rs`
- `libs/trading_core/src/commands/mod.rs`

**Impact**: Fills existing slots (no breaking changes)  
**Testing**: End-to-end trace propagation tests

### Phase 7: Logging Enhancement (Week 8)

**Goal**: Include trace context in structured logs

**Changes**:

1. **Update logging configuration**:
   - Include trace context in log format
   - Add trace_id, span_id, correlation_id to logs

2. **Update log statements**:
   - Include trace context in key log points
   - Use structured logging (key-value pairs)

3. **Add log correlation**:
   - Enable log-to-trace correlation in Jaeger
   - Enable trace-to-log correlation in log aggregators

**Files Modified**: Logging configuration, key log points  
**Impact**: Log format changes (backward compatible)  
**Testing**: Log correlation tests

### Phase 8: Full System Integration (Week 9-10)

**Goal**: Complete integration and remove old code

**Changes**:

1. **Remove dual publishing** (if used during migration):
   - Remove old channel publishing
   - Keep only new event bus publishing

2. **Remove optional trace context**:
   - Make trace context required (or keep optional for backward compat)
   - Update all publishers to always include trace context

3. **Performance optimization**:
   - Optimize hot path tracing (ensure <1μs)
   - Optimize cold path tracing (ensure <10μs)
   - Profile and optimize bottlenecks

4. **Documentation**:
   - Update architecture docs
   - Add tracing guide for developers
   - Add observability runbook

**Files Modified**: All publishers, documentation  
**Impact**: Final cleanup (can be done gradually)  
**Testing**: Full system tests, performance benchmarks

### Migration Checklist

#### Phase 1: Foundation
- [ ] Create `libs/trading_trace` crate in workspace
- [ ] Define `HotTraceContext`
- [ ] Define `ColdTraceContext`
- [ ] Define `TraceContext` enum
- [ ] Define `CorrelationId`
- [ ] Define `TraceBridge`
- [ ] Add to workspace
- [ ] Unit tests

#### Phase 2: OpenTelemetry
- [ ] Add OpenTelemetry dependencies
- [ ] Implement `ColdTraceContext` wrapper
- [ ] Implement Jaeger exporter
- [ ] Implement Prometheus exporter
- [ ] Configuration system
- [ ] Integration tests

#### Phase 3: Hot Path
- [ ] Implement `HotTraceContext`
- [ ] Implement sampling logic
- [ ] Integrate into market data publisher
- [ ] Performance benchmarks
- [ ] Unit tests

#### Phase 4: Cold Path
- [ ] Integrate into exchange state publisher
- [ ] Integrate into trading engine
- [ ] Integrate into command handlers
- [ ] Integrate into gRPC handlers
- [ ] Integration tests

#### Phase 5: Correlation
- [ ] Implement `TraceBridge`
- [ ] Update publishers with correlation
- [ ] Update exporters
- [ ] Integration tests

#### Phase 6: Networking
- [ ] Fill DTP trace context
- [ ] Fill hot event trace context
- [ ] Fill cold event trace context
- [ ] Fill command trace context
- [ ] End-to-end tests

#### Phase 7: Logging
- [ ] Update log format
- [ ] Include trace context in logs
- [ ] Log correlation setup
- [ ] Tests

#### Phase 8: Final Integration
- [ ] Remove dual publishing
- [ ] Performance optimization
- [ ] Documentation
- [ ] Full system tests

### Risk Mitigation

1. **Backward Compatibility**: All trace context fields optional during migration
2. **Performance**: Benchmark at each phase, ensure targets met
3. **Rollback**: Keep old code until migration complete
4. **Testing**: Extensive testing at each phase
5. **Gradual Rollout**: Migrate one component at a time

### Success Criteria

- ✅ All trace types defined and tested
- ✅ OpenTelemetry integrated and exporting to Jaeger
- ✅ Hot path tracing <1μs overhead
- ✅ Cold path tracing <10μs overhead
- ✅ Trace context propagated through entire system
- ✅ Correlation between hot and cold paths working
- ✅ Logs include trace context
- ✅ Full request/response traces visible in Jaeger
- ✅ Performance targets met
- ✅ Documentation complete

---

## Conclusion

**This proposal defines a comprehensive traceability system using a hybrid approach:**

- **Hot path**: Custom lightweight tracing for minimal overhead
- **Cold path**: OpenTelemetry for rich observability
- **Unified interface**: `TraceContext` enum for networking layer
- **Correlation**: Links hot and cold paths via correlation IDs

**Implementation Order**:
1. **This proposal (03)** - Define types and implement tracing
2. **Proposal 04** - Use these types in networking layer

**This ensures proper integration and avoids retrofitting.**

---

## References

- [OpenTelemetry Rust](https://opentelemetry.io/docs/instrumentation/rust/)
- [Dapper Paper](https://research.google/pubs/pub36356/)
- [Jaeger Architecture](https://www.jaegertracing.io/docs/1.50/architecture/)
- [Zipkin Architecture](https://zipkin.io/pages/architecture.html)
