# Proposal: Traceability & Observability

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft - To be discussed  
**Prerequisites**: Proposal 03 (Communication Layer) should be implemented first, or at least designed with traceability in mind

## Executive Summary

This proposal addresses comprehensive observability and distributed tracing for the trading system. It enables full request/response correlation, event chain tracking, performance monitoring, and production debugging capabilities.

**Key Objectives:**

1. **Request Tracing** - Trace every external request through the entire system
2. **Event Correlation** - Correlate commands, events, and responses
3. **Performance Monitoring** - Track latency at every level
4. **Error Tracking** - Full error context with trace chains
5. **Distributed Tracing** - Export traces to Jaeger/Zipkin
6. **Structured Logging** - Logs with trace context for correlation

**Why This Matters:**

- **Debugging**: Without traceability, debugging production issues is extremely difficult
- **Performance**: Need visibility into bottlenecks before optimizing
- **Compliance**: Audit trails for trading operations
- **Observability**: Understand system behavior in production

---

## Current State Assessment

**What Exists:**
- ✅ Basic `tracing` crate integration
- ✅ Log levels (trace, debug, info, warn, error)
- ✅ File logging for CLI (`cli_debug.log`)
- ✅ Simple audit logging for config changes

**What's Missing:**
- ❌ No correlation IDs across services
- ❌ No distributed tracing
- ❌ No request/response correlation
- ❌ No span tracking
- ❌ No structured context propagation
- ❌ No performance tracing (latency, queue depths)
- ❌ No integration with observability tools (Jaeger, Zipkin, etc.)

---

## Design Principles

### 1. Trace Context Propagation

**Every message/operation should carry trace context:**
- Internal channels (Event Bus, Command Router)
- DTP envelopes (Gateway boundary)
- Transports (gRPC, Pub/Sub, IPC)

**Note**: This requires coordination with Proposal 03 to ensure DTP envelopes and channel messages have trace context support.

### 2. Minimal Overhead

- Hot path: <1% overhead
- Cold path: <5% overhead
- Optional sampling for high-frequency events (market data)

### 3. Zero-Copy Where Possible

- Trace context should be lightweight (IDs only)
- Full data in spans/logs, not in context

### 4. Backward Compatible

- Existing code continues to work
- Trace context is optional (can be `None`)
- Gradual migration path

---

## Architecture

### Trace Context

**Location**: `libs/trading_core/src/trace/context.rs`

```rust
/// Trace context that propagates through all operations
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// Root trace ID (generated at request entry point)
    pub trace_id: u64,
    
    /// Parent span ID (for nested operations)
    pub parent_span_id: Option<u64>,
    
    /// Current span ID
    pub span_id: u64,
    
    /// Correlation ID (for request/response matching)
    pub correlation_id: u64,
    
    /// Component that initiated the trace
    pub source_component: u8,
    
    /// Timestamp when trace started
    pub start_timestamp: u64,
}

impl TraceContext {
    /// Create new root trace context
    pub fn new_root(correlation_id: u64, source_component: u8) -> Self {
        let trace_id = generate_trace_id();
        let span_id = generate_span_id();
        Self {
            trace_id,
            parent_span_id: None,
            span_id,
            correlation_id,
            source_component,
            start_timestamp: current_timestamp_nanos(),
        }
    }
    
    /// Create child span
    pub fn child(&self, component: u8) -> Self {
        Self {
            trace_id: self.trace_id,
            parent_span_id: Some(self.span_id),
            span_id: generate_span_id(),
            correlation_id: self.correlation_id,
            source_component: component,
            start_timestamp: current_timestamp_nanos(),
        }
    }
}
```

### Span Manager

**Location**: `libs/trading_core/src/trace/span.rs`

```rust
pub struct SpanManager {
    active_spans: Arc<RwLock<HashMap<u64, Span>>>>,
}

pub struct Span {
    pub span_id: u64,
    pub trace_id: u64,
    pub parent_span_id: Option<u64>,
    pub component: u8,
    pub operation: String,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub tags: HashMap<String, String>,
    pub logs: Vec<SpanLog>,
}

impl SpanManager {
    /// Start a new span
    pub fn start_span(
        &self,
        trace_id: u64,
        parent_span_id: Option<u64>,
        component: u8,
        operation: String,
    ) -> u64 {
        // Generate span_id, create Span, store in active_spans
    }
    
    /// End a span
    pub fn end_span(&self, span_id: u64) -> Result<Span, TraceError> {
        // Mark end_time, remove from active_spans, return completed span
    }
}
```

### Context Propagation

**Location**: `libs/trading_core/src/trace/propagation.rs`

```rust
/// Propagate trace context through channels
pub trait TracePropagator {
    /// Inject trace context into message
    fn inject(&self, context: &TraceContext, message: &mut dyn Message);
    
    /// Extract trace context from message
    fn extract(&self, message: &dyn Message) -> Option<TraceContext>;
}
```

---

## Integration Points

### 1. DTP Envelope (Gateway)

**Requires coordination with Proposal 03:**

```rust
pub struct DtpEnvelope {
    // ... existing fields ...
    
    /// Trace context (optional, for distributed tracing)
    pub trace_context: Option<TraceContext>,
}
```

### 2. Event Bus Messages

**Hot Events:**
```rust
pub enum HotEvent {
    Orderbook {
        // ... existing fields ...
        trace_context: Option<TraceContext>,
    },
    // ...
}
```

**Cold Events:**
```rust
pub enum ColdEvent {
    Order(OrderEvent),
    // ...
}

pub struct OrderEvent {
    // ... existing fields ...
    trace_context: Option<TraceContext>,
}
```

### 3. Command Router

```rust
pub struct Command {
    // ... existing fields ...
    pub trace_context: Option<TraceContext>,
}
```

---

## Implementation Phases

### Phase 1: Foundation (Week 1-2)

**Location**: `libs/trading_core/src/trace/`

- [ ] Create `trace` module structure
- [ ] Implement `TraceContext`
- [ ] Implement `SpanManager`
- [ ] Implement context propagation
- [ ] Add unit tests
- [ ] Document design decisions

### Phase 2: Integration with Core Services (Week 2-4)

- [ ] Add trace context to Exchange Engine operations
- [ ] Add trace context to Trading Engine operations
- [ ] Add trace context to Market Data Engine operations
- [ ] Add trace context to Indicator Engine operations
- [ ] Add trace context to gRPC handlers
- [ ] Add span tracking to major operations

### Phase 3: Gateway Integration (Week 4-5)

**Requires Proposal 03 Gateway implementation:**

- [ ] Add trace context extraction from DTP envelopes
- [ ] Add trace context injection into DTP envelopes
- [ ] Add trace context propagation through Gateway
- [ ] Add span tracking for Gateway operations

### Phase 4: Observability Export (Week 5-6)

- [ ] Integrate Jaeger exporter
- [ ] Add Prometheus metrics for traces
- [ ] Create trace visualization dashboard
- [ ] Add trace search/filtering
- [ ] Configure trace sampling policies

### Phase 5: Logging Enhancement (Week 6-7)

- [ ] Update all log statements to include trace context
- [ ] Add structured logging format
- [ ] Add log aggregation (Loki, ELK, etc.)
- [ ] Create log correlation tools
- [ ] Add trace_id/span_id to all log entries

---

## Trace Patterns

### Pattern 1: Request Tracing

**Every external request should be traceable:**

```
[gRPC Request] trace_id=123, correlation_id=456
  └─> [Command Router] span_id=1, parent=None
      └─> [Trading Engine] span_id=2, parent=1
          └─> [Exchange Engine] span_id=3, parent=2
              └─> [Exchange API] span_id=4, parent=3
          └─> [Event Published] span_id=5, parent=2
      └─> [Response] correlation_id=456
```

### Pattern 2: Event Chain Tracing

**Every event chain should be traceable:**

```
[Order Placed] trace_id=123, order_id=789
  └─> [Fill Event] trace_id=123, order_id=789, fill_id=101
      └─> [Position Update] trace_id=123, position_id=202
          └─> [Risk Check] trace_id=123, risk_id=303
```

### Pattern 3: Performance Tracing

**Track performance at every level:**
- Channel latency (hot/cold lanes)
- Gateway processing time
- Transport send time
- Handler execution time
- Database query time

---

## Open Questions (To Be Discussed)

1. **Trace Storage**: Should we store traces locally or only export to Jaeger?
   - **Recommendation**: Export to Jaeger, optional local storage for recent traces

2. **Trace Sampling**: Should we sample traces to reduce overhead?
   - **Recommendation**: Sample low-priority traces (market data), always trace critical paths (orders, fills)

3. **Trace Retention**: How long should we keep traces?
   - **Recommendation**: 7 days in Jaeger, 24 hours locally

4. **Performance Impact**: What's acceptable overhead for tracing?
   - **Recommendation**: <1% overhead on hot path, <5% on cold path

5. **Trace Context Size**: How much data should we include in trace context?
   - **Recommendation**: Minimal (IDs only), full data in spans/logs

6. **Integration Timing**: Should traceability be implemented before or after Proposal 03?
   - **Recommendation**: Design together, implement after Proposal 03 (or in parallel if Proposal 03 includes trace context "slots")

7. **Tracing Patterns**: What specific patterns should we use for different operation types?
   - **To be discussed**: Request/response, event chains, async operations, error handling

---

## Success Criteria

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
- [ ] Overhead targets met (<1% hot path, <5% cold path)

---

## Dependencies

**Requires:**
- Proposal 03 (Communication Layer) - for DTP envelope and Gateway integration
- Or at minimum: Design coordination to ensure trace context can be added later

**Enables:**
- Production debugging
- Performance optimization
- Compliance/audit trails
- Future observability enhancements

---

## Next Steps

1. **Review and discuss** this proposal
2. **Decide on tracing patterns** for different operation types
3. **Coordinate with Proposal 03** to ensure trace context support in DTP/Gateway
4. **Design trace context structure** based on requirements
5. **Plan implementation phases** based on Proposal 03 timeline

---

**Status**: This proposal is ready for discussion. Key decisions needed:
- Tracing patterns for different operation types
- Integration timing with Proposal 03
- Trace sampling and retention policies
- Performance overhead targets
