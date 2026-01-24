# Trading Trace Library

**Version**: 0.1.0  
**Status**: Ready for Integration

A high-performance, hybrid tracing library for the trading system providing lightweight hot path tracing (<1μs) and full OpenTelemetry integration for cold path operations (<10μs).

## Overview

The `trading_trace` library implements a **hybrid tracing approach** optimized for trading systems:

- **Hot Path**: Ultra-lightweight custom tracing for high-frequency operations (market data, order processing)
- **Cold Path**: Full OpenTelemetry integration for commands, events, and state management
- **Bridge**: Correlation system to link hot and cold path traces
- **Metrics**: Prometheus metrics support for monitoring

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Usage Examples](#usage-examples)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [Performance Characteristics](#performance-characteristics)
- [Integration Guide](#integration-guide)

## Features

### Core Capabilities

- ✅ **Hybrid Tracing**: Hot path (<1μs) and cold path (<10μs) support
- ✅ **OpenTelemetry Integration**: Full OpenTelemetry SDK support
- ✅ **Distributed Tracing**: Jaeger exporter for trace visualization
- ✅ **Metrics**: Prometheus metrics for monitoring
- ✅ **Correlation IDs**: Link related operations across services
- ✅ **Zero-Copy Design**: Minimal allocations for hot path
- ✅ **Feature Gating**: Optional exporters via `full` feature flag
- ✅ **Strongly Typed**: Constants instead of magic strings

### Performance

- **Hot Path**: <1μs overhead per operation
- **Cold Path**: <10μs overhead per operation
- **Zero Allocations**: Static strings for hot path operations
- **Minimal Locks**: Efficient concurrent access patterns

## Architecture

### Hybrid Approach

The library uses two distinct tracing paths optimized for different use cases:

```
┌─────────────────────────────────────────────────────────┐
│                    Trading System                        │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Hot Path (High Frequency)          Cold Path (Commands)│
│  ┌──────────────────────┐          ┌──────────────────┐ │
│  │ HotTraceContext      │          │ ColdTraceContext │ │
│  │ - trace_id: u64      │          │ - OpenTelemetry  │ │
│  │ - span_id: u64       │          │ - Full metadata │ │
│  │ - correlation_id     │          │ - Attributes    │ │
│  │ Overhead: <1μs       │          │ Overhead: <10μs │ │
│  └──────────────────────┘          └──────────────────┘ │
│           │                                    │         │
│           └──────────┬─────────────────────────┘         │
│                      │                                   │
│              ┌───────▼────────┐                         │
│              │  TraceBridge  │                         │
│              │  (Correlation) │                         │
│              └────────────────┘                         │
│                      │                                   │
│           ┌──────────┴──────────┐                       │
│           │                     │                       │
│    ┌──────▼──────┐      ┌──────▼──────┐               │
│    │   Jaeger    │      │  Prometheus │               │
│    │  Exporter   │      │  Exporter   │                │
│    └─────────────┘      └─────────────┘                │
└─────────────────────────────────────────────────────────┘
```

### Hot Path

**Use Case**: High-frequency operations (market data, order updates)

- **Overhead**: <1μs per operation
- **Storage**: Copy types (u64 IDs only)
- **Allocations**: Zero (all stack-allocated)
- **Sampling**: Configurable sample rate

**Example**:
```rust
use trading_trace::HotTraceContext;

// Create hot trace context
let ctx = HotTraceContext::new()
    .with_correlation_id(correlation_id);

// Use in hot path operations
process_market_data(&ctx, orderbook_update);
```

### Cold Path

**Use Case**: Commands, events, state updates, gRPC handlers

- **Overhead**: <10μs per operation
- **Storage**: Full OpenTelemetry spans
- **Features**: Attributes, events, status codes
- **Export**: Jaeger, Prometheus

**Example**:
```rust
use trading_trace::{get_tracer, ColdTraceContext};

let tracer = get_tracer("trading-engine");
let ctx = ColdTraceContext::new_root_with_tracer(
    &tracer,
    "place_order",
    Some(correlation_id),
);

ctx.set_attribute("order_type", "limit");
ctx.set_attribute("symbol", "BTCUSDT");
ctx.add_event("order_validated");
ctx.end();
```

## Quick Start

### Basic Setup

```rust
use trading_trace::{init_exporters, TraceConfig, get_tracer, get_metrics};

// Initialize exporters
let config = TraceConfig::default();
let handles = init_exporters(
    &config.exporters,
    "trading-system",
    Some("0.1.0"),
)?;

// Get tracer for cold path
let tracer = get_tracer("trading-engine");

// Get metrics
let metrics = get_metrics("trading-engine");

// Cleanup on shutdown
trading_trace::shutdown_exporters();
```

### Hot Path Usage

```rust
use trading_trace::{HotTraceContext, should_trace_hot};

if should_trace_hot(100) {  // Sample 1% of operations
    let ctx = HotTraceContext::new();
    process_order_update(&ctx);
}
```

### Cold Path Usage

```rust
use trading_trace::{get_tracer, ColdTraceContext};

let tracer = get_tracer("trading-engine");
let mut ctx = ColdTraceContext::new_root_with_tracer(
    &tracer,
    "execute_strategy",
    Some(correlation_id),
);

ctx.set_attribute("strategy", "buy_low_sell_high");
ctx.set_attribute("symbol", "BTCUSDT");
ctx.add_event("strategy_triggered");
ctx.end();
```

## Usage Examples

### Example 1: Market Data Processing (Hot Path)

```rust
use trading_trace::HotTraceContext;

fn process_orderbook_update(ctx: &HotTraceContext, update: OrderbookUpdate) {
    // Fast path - no allocations
    let child_ctx = ctx.child();
    update_indicators(&child_ctx, update);
}

fn update_indicators(ctx: &HotTraceContext, update: OrderbookUpdate) {
    // Process indicators with trace context
    // ctx.trace_id and ctx.span_id available for correlation
}
```

### Example 2: Command Execution (Cold Path)

```rust
use trading_trace::{get_tracer, ColdTraceContext};

async fn execute_order_command(
    tracer: &impl Tracer,
    correlation_id: u64,
    order: OrderRequest,
) -> Result<Order> {
    let mut ctx = ColdTraceContext::new_root_with_tracer(
        tracer,
        "execute_order",
        Some(correlation_id),
    );

    ctx.set_attribute("order_type", order.order_type);
    ctx.set_attribute("symbol", &order.symbol);
    ctx.set_attribute("side", order.side);

    ctx.add_event("order_validated");

    match place_order(&order).await {
        Ok(order) => {
            ctx.set_status(Status::Ok);
            ctx.set_attribute("order_id", order.id);
            ctx.end();
            Ok(order)
        }
        Err(e) => {
            ctx.set_status(Status::error(e.to_string()));
            ctx.set_attribute("error", e.to_string());
            ctx.end();
            Err(e)
        }
    }
}
```

### Example 3: Metrics Recording

```rust
use trading_trace::get_metrics;

let metrics = get_metrics("trading-engine");

// Record command execution
metrics.record_command("place_order", true);

// Record event
metrics.record_event("order_filled");

// Record duration
let start = Instant::now();
execute_order();
metrics.record_duration("place_order", start.elapsed().as_secs_f64());

// Record error
metrics.record_error("insufficient_balance");
```

### Example 4: Linking Hot and Cold Paths

```rust
use trading_trace::{TraceBridge, HotTraceContext, get_tracer, ColdTraceContext};

let bridge = TraceBridge::new();

// Hot path operation
let hot_ctx = HotTraceContext::new()
    .with_correlation_id(correlation_id);

// Later, create cold path trace
let tracer = get_tracer("trading-engine");
let cold_ctx = ColdTraceContext::new_root_with_tracer(
    &tracer,
    "process_fill",
    Some(correlation_id),
);

// Link them via bridge
bridge.link(hot_ctx.trace_id, cold_ctx.span().unwrap().span_context().trace_id());
```

## Configuration

### TraceConfig

```rust
use trading_trace::TraceConfig;
use serde_json;

let config = TraceConfig {
    service_name: "trading-system".to_string(),
    service_version: Some("0.1.0".to_string()),
    hot_path: HotPathConfig {
        sample_rate: 100,  // Sample 1% (1 in 100)
    },
    cold_path: ColdPathConfig {
        enabled: true,
        sample_rate: 1.0,  // 100% sampling
    },
    exporters: ExportersConfig {
        jaeger: Some(JaegerConfig {
            enabled: true,
            agent_endpoint: "localhost:6831".to_string(),
        }),
        prometheus: Some(PrometheusConfig {
            enabled: true,
        }),
    },
};

// Serialize to JSON
let json = serde_json::to_string(&config)?;
```

### JSON Configuration

```json
{
  "service_name": "trading-system",
  "service_version": "0.1.0",
  "hot_path": {
    "sample_rate": 100
  },
  "cold_path": {
    "enabled": true,
    "sample_rate": 1.0
  },
  "exporters": {
    "jaeger": {
      "enabled": true,
      "agent_endpoint": "localhost:6831"
    },
    "prometheus": {
      "enabled": true
    }
  }
}
```

## API Reference

### Hot Path Types

#### `HotTraceContext`

Lightweight trace context for high-frequency operations.

```rust
pub struct HotTraceContext {
    pub trace_id: u64,
    pub span_id: u64,
    pub parent_span_id: Option<u64>,
    pub correlation_id: Option<u64>,
}
```

**Methods**:
- `new() -> Self` - Create new root context
- `child(&self) -> Self` - Create child context
- `with_correlation_id(self, id: u64) -> Self` - Add correlation ID

#### `should_trace_hot(sample_rate: u32) -> bool`

Determine if hot path operation should be traced based on sample rate.

### Cold Path Types

#### `ColdTraceContext`

Full OpenTelemetry trace context.

**Methods**:
- `new_root(operation, correlation_id) -> Self` - Create root without tracer
- `new_root_with_tracer(tracer, operation, correlation_id) -> Self` - Create root with tracer
- `child(&self, operation) -> Self` - Create child context
- `child_with_tracer(&self, tracer, operation) -> Self` - Create child with tracer
- `set_attribute(&mut self, key, value)` - Set span attribute
- `add_event(&mut self, name)` - Add event to span
- `set_status(&mut self, status)` - Set span status
- `end(self)` - End span explicitly

### Unified Context

#### `TraceContext`

Enum for unified trace context (used in networking layer).

```rust
pub enum TraceContext {
    Hot(HotTraceContext),
    Cold(ColdTraceContext),
    None,
}
```

### Metrics

#### `TradingMetrics`

Pre-configured metrics for trading operations.

**Methods**:
- `record_command(&self, command_type, success)` - Record command execution
- `record_event(&self, event_type)` - Record event
- `record_duration(&self, operation, duration_seconds)` - Record operation duration
- `record_error(&self, error_type)` - Record error

### Exporters

#### `init_exporters(exporters, service_name, service_version) -> Result<ExporterHandles>`

Initialize OpenTelemetry exporters (Jaeger, Prometheus).

#### `get_tracer(name) -> impl Tracer`

Get global OpenTelemetry tracer.

#### `get_metrics(name) -> TradingMetrics`

Get global metrics instance.

#### `shutdown_exporters()`

Shutdown all exporters gracefully.

### Bridge

#### `TraceBridge`

Links hot and cold path traces via correlation.

**Methods**:
- `link(&self, hot_trace_id, cold_trace_id)` - Link hot and cold traces
- `register_correlation(&self, correlation_id, hot_id, cold_id)` - Register correlation
- `get_cold_trace(&self, hot_trace_id) -> Option<u64>` - Get cold trace ID
- `get_by_correlation(&self, correlation_id) -> Option<CorrelationMapEntry>` - Get by correlation
- `cleanup_old_correlations(&self, threshold)` - Cleanup old entries
- `stats(&self) -> TraceBridgeStats` - Get statistics

## Performance Characteristics

### Performance Targets

The library is designed to meet strict performance requirements:

- **Hot Path**: <1μs overhead per operation (with sampling)
- **Cold Path**: <10μs overhead per operation
- **Overall System Impact**: <1% performance degradation
- **Memory**: Minimal allocations, zero-copy where possible

### Hot Path

- **Overhead**: <1μs per operation
- **Memory**: 32 bytes per context (Copy type)
- **Allocations**: Zero (all stack-allocated)
- **Locks**: None (Copy semantics)
- **Sampling**: Configurable (default 1% = 1 in 100 operations)
- **Use Cases**: Market data streams, high-frequency order updates

**Why Sampling?**
- Market data can reach 1000s of events per second
- Tracing every event would create massive overhead
- Sampling 1% provides representative data without performance impact

### Cold Path

- **Overhead**: <10μs per operation
- **Memory**: ~200-500 bytes per span (OpenTelemetry)
- **Allocations**: Minimal (OpenTelemetry internal)
- **Locks**: None (per-thread spans)
- **Coverage**: 100% (all commands, events, state updates)
- **Use Cases**: Commands, gRPC handlers, state changes, background operations

**Why 100% Coverage?**
- Commands and state changes are lower frequency
- Full observability is critical for debugging and compliance
- <10μs overhead is acceptable for these operations

### Metrics

- **Overhead**: <100ns per metric recording
- **Memory**: Shared meter (Arc)
- **Allocations**: Minimal (attribute arrays)
- **Coverage**: 100% (all operations record metrics)

### Success Criteria

The tracing system is considered successful when:

- ✅ **Hot Path**: <1% performance impact (with sampling)
- ✅ **Cold Path**: <5% performance impact
- ✅ **Coverage**: Every external request has trace_id
- ✅ **Correlation**: Every command has correlation_id
- ✅ **Observability**: Full request flow traceable (gRPC → Command → Event → Response)
- ✅ **Event Chains**: Event chains traceable (Order → Fill → Position)
- ✅ **Export**: Traces exportable to Jaeger, metrics to Prometheus
- ✅ **Linking**: Hot and cold paths linked via correlation IDs

## Integration Guide

### Integration Points

The tracing library integrates at several key points in the system:

1. **gRPC Handlers**: Create root spans for external requests
2. **Command Handlers**: Create spans for command execution
3. **Event Publishers**: Add trace context to events
4. **Market Data Publishers**: Add hot trace context (sampled)
5. **State Managers**: Add cold trace context to state updates
6. **Exchange Integrations**: Propagate trace context through API calls

### Step 1: Add Dependency

```toml
[dependencies]
trading_trace = { path = "libs/trading_trace", features = ["full"] }
```

### Step 2: Initialize at Startup

```rust
use trading_trace::{init_exporters, TraceConfig, shutdown_exporters};

// Load config
let config = TraceConfig::default();  // Or load from file

// Initialize exporters
let _handles = init_exporters(
    &config.exporters,
    &config.service_name,
    config.service_version.as_deref(),
)?;

// ... run application ...

// Shutdown on exit
shutdown_exporters();
```

### Step 3: Use in Hot Path (Market Data)

```rust
use trading_trace::{HotTraceContext, should_trace_hot};

fn process_market_data(update: MarketDataUpdate) {
    if should_trace_hot(100) {  // Sample 1%
        let ctx = HotTraceContext::new();
        // Use ctx in processing
        publish_orderbook_update(update, Some(ctx));
    } else {
        publish_orderbook_update(update, None);
    }
}
```

**When to Use Hot Path**:
- Market data streams (orderbooks, trades)
- High-frequency updates (>100/sec)
- Performance-critical paths
- Operations where <1μs overhead is required

### Step 4: Use in Cold Path (Commands, Events)

```rust
use trading_trace::{get_tracer, ColdTraceContext, generate_correlation_id};

async fn handle_command(command: Command) -> Result<Response> {
    let tracer = get_tracer("trading-engine");
    let correlation_id = generate_correlation_id();
    let mut ctx = ColdTraceContext::new_root_with_tracer(
        &tracer,
        "handle_command",
        Some(correlation_id),
    );
    
    ctx.set_attribute("command_type", command.command_type);
    ctx.set_attribute("symbol", &command.symbol);
    
    // ... process command ...
    
    ctx.set_status(Status::Ok);
    ctx.end();
    Ok(response)
}
```

**When to Use Cold Path**:
- gRPC handlers
- Command execution
- State changes
- Event publishing (non-market data)
- Background operations
- Any operation where full observability is needed

### Step 5: Link Hot and Cold Paths

```rust
use trading_trace::{TraceBridge, HotTraceContext, ColdTraceContext};

// In hot path
let hot_ctx = HotTraceContext::new()
    .with_correlation_id(correlation_id);

// Later, in cold path
let cold_ctx = ColdTraceContext::new_root_with_tracer(
    &tracer,
    "process_fill",
    Some(correlation_id),
);

// Link via bridge
let bridge = TraceBridge::new();
bridge.register_correlation(
    correlation_id,
    Some(hot_ctx.trace_id),
    Some(cold_ctx.span().unwrap().span_context().trace_id()),
);
```

### Step 6: Record Metrics

```rust
use trading_trace::get_metrics;

let metrics = get_metrics("trading-engine");

// Record operations
metrics.record_command("place_order", true);
metrics.record_duration("order_processing", duration);
metrics.record_event("order_filled");
metrics.record_error("insufficient_balance");
```

### Integration Checklist

- [ ] Initialize exporters at application startup
- [ ] Add hot path tracing to market data publishers (with sampling)
- [ ] Add cold path tracing to gRPC handlers
- [ ] Add cold path tracing to command handlers
- [ ] Add cold path tracing to event publishers
- [ ] Add correlation IDs to commands and responses
- [ ] Link hot and cold paths via TraceBridge
- [ ] Record metrics for key operations
- [ ] Test trace export to Jaeger
- [ ] Verify metrics in Prometheus
- [ ] Performance benchmark (ensure targets met)

## Feature Flags

### `full` Feature

Enables all optional dependencies:
- `opentelemetry-jaeger` - Jaeger exporter
- `opentelemetry-prometheus` - Prometheus exporter
- `tracing-opentelemetry` - Tracing integration
- `tokio` - Async runtime
- `serde_json` - JSON serialization

**Usage**:
```toml
[dependencies]
trading_trace = { path = "libs/trading_trace", features = ["full"] }
```

**Without `full` feature**:
- Core OpenTelemetry functionality still available
- Exporters disabled
- Reduced dependency footprint

## Constants

All attribute keys and metric names are strongly-typed constants:

```rust
use trading_trace::constants::*;

// Attribute keys
ATTR_SERVICE_NAME
ATTR_SERVICE_VERSION
ATTR_CORRELATION_ID
ATTR_COMMAND_TYPE
ATTR_EVENT_TYPE
ATTR_OPERATION
ATTR_ERROR_TYPE
ATTR_SUCCESS

// Metric names
METRIC_COMMANDS_TOTAL
METRIC_EVENTS_TOTAL
METRIC_OPERATION_DURATION_SECONDS
METRIC_ERRORS_TOTAL
```

## Best Practices

### Hot Path

1. **Use sampling**: Don't trace every operation
   ```rust
   if should_trace_hot(100) {  // 1% sampling
       let ctx = HotTraceContext::new();
   }
   ```

2. **Pass by reference**: Hot contexts are Copy, but prefer references for clarity
   ```rust
   fn process(&self, ctx: &HotTraceContext) { }
   ```

3. **Use correlation IDs**: Link related operations
   ```rust
   let ctx = HotTraceContext::new()
       .with_correlation_id(correlation_id);
   ```

### Cold Path

1. **Always end spans**: Use `end()` or rely on `Drop`
   ```rust
   ctx.end();  // Explicit
   // Or let Drop handle it
   ```

2. **Set attributes early**: Add context as soon as available
   ```rust
   ctx.set_attribute("order_id", order.id);
   ctx.set_attribute("symbol", &order.symbol);
   ```

3. **Use events for milestones**: Mark important points
   ```rust
   ctx.add_event("order_validated");
   ctx.add_event("risk_check_passed");
   ```

4. **Set status on completion**: Indicate success/failure
   ```rust
   ctx.set_status(Status::Ok);
   // or
   ctx.set_status(Status::error("insufficient_balance"));
   ```

### Metrics

1. **Record consistently**: Use same operation names
   ```rust
   metrics.record_duration("place_order", duration);
   ```

2. **Use static strings**: Prefer `&'static str` for operation names
   ```rust
   metrics.record_command("place_order", true);
   ```

## Troubleshooting

### Exporter Not Working

**Issue**: Traces not appearing in Jaeger

**Solution**:
1. Check Jaeger agent is running: `docker run -d -p 6831:6831/udp jaegertracing/all-in-one`
2. Verify endpoint in config: `agent_endpoint: "localhost:6831"`
3. Ensure `full` feature is enabled

### High Overhead

**Issue**: Tracing causing performance issues

**Solution**:
1. Increase hot path sample rate (trace fewer operations)
2. Disable cold path if not needed: `cold_path.enabled = false`
3. Use hot path for high-frequency operations

### Missing Traces

**Issue**: Some operations not traced

**Solution**:
1. Check sample rate configuration
2. Verify `should_trace_hot()` is called correctly
3. Ensure spans are created with `new_root_with_tracer()` or `child_with_tracer()`

## Design Decisions

### Why Hybrid Approach?

The trading system processes fundamentally different operation types:

1. **High-Frequency Operations** (100s-1000s/sec)
   - Market data streams, order updates
   - Requires <1μs overhead
   - Solution: Hot path with sampling

2. **Command/Event Operations** (10s-100s/sec)
   - Commands, state changes, gRPC handlers
   - Requires full observability
   - Solution: Cold path with 100% coverage

3. **Correlation Requirements**
   - Link market data events to trading decisions
   - Link commands to responses
   - Solution: Correlation IDs and TraceBridge

### Why Sampling for Hot Path?

- **Performance**: Tracing every market data event would create massive overhead
- **Representativeness**: 1% sampling provides statistically representative data
- **Cost**: Reduces storage and processing costs
- **Flexibility**: Sampling rate configurable per use case

### Why 100% Coverage for Cold Path?

- **Debugging**: Full traces critical for production debugging
- **Compliance**: Audit trails require complete command/event history
- **Observability**: Need to see all state changes and command flows
- **Acceptable Overhead**: <10μs is acceptable for lower-frequency operations

### Why Correlation IDs?

- **Cross-Path Linking**: Link hot path events to cold path commands
- **Request/Response**: Correlate commands with responses
- **Event Chains**: Link related events (Order → Fill → Position)
- **Debugging**: Trace full flow from market data to order execution

### Why OpenTelemetry?

- **Standard**: Industry-standard observability framework
- **Ecosystem**: Rich ecosystem of exporters and tools
- **Interoperability**: Works with Jaeger, Prometheus, Zipkin, etc.
- **Future-Proof**: Extensible to new exporters and formats

## See Also

- [Main README](../../README.md) - System overview
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/) - OpenTelemetry reference
