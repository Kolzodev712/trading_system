# Architecture Summary: Complete System Overview

## Overview

This document provides a comprehensive overview of the entire trading system architecture, incorporating all proposals with performance optimizations and efficient communication patterns.

## Prerequisites & Infrastructure Requirements

To run the trading system in production, the following infrastructure and services must be set up and running:

### Required Services

#### 1. MongoDB Instance
- **Purpose**: Configuration persistence (levels, risk limits)
- **Version**: MongoDB 5.0+ (or compatible)
- **Connection**: Connection string required in configuration
- **Collections**: 
  - `trading_config` - Level configurations
  - `risk_limits` - Risk management limits
  - `bot_config` - Per-bot configuration (e.g., `binance_bot`)
- **Persistence**: Initially disabled (lazy initialization), can be enabled via config
- **Backup**: Recommended for production (config backup only, not state)
- **Example Connection**: `mongodb://localhost:27017/trading_system`

#### 2. Redis Instance (Optional - Feature Flag)
- **Purpose**: Currently used for internal communication (to be migrated to channels per Proposal 04)
- **Version**: Redis 6.0+ (or compatible)
- **Connection**: Connection string required in configuration
- **Note**: **Will be removed** after Proposal 04 (Redis → Channels migration). Redis code will be kept behind feature flag for potential future Web UI use (if needed).

#### 3. Exchange API Access
- **Exchange**: Binance (spot trading)
- **API Credentials Required**:
  - API Key
  - API Secret
  - Testnet/Production environment selection
- **Permissions Needed**:
  - Read account information
  - Place orders
  - Cancel orders
  - View order history
  - View trade history
- **Rate Limits**: System handles rate limiting automatically
- **Network**: Outbound HTTPS/WebSocket access to Binance API endpoints

#### 4. Trading System (gRPC Server)
- **Purpose**: Core trading engine, level management, risk system
- **Port**: Configurable (default: 50051)
- **Protocol**: gRPC (HTTP/2)
- **Endpoints**:
  - Level management (CRUD operations)
  - Monitoring data streaming
  - System health endpoints
  - Protocol health endpoints
- **State**: Stateful (single instance recommended)

### Optional Services

#### 5. Indicator Service (In-Process)
- **Purpose**: Calculate and provide indicator values
- **Protocol**: Channels (in-process, same workspace)
- **Note**: Separate crate but same monolith, uses channels for communication
- **Required**: **Prerequisite for Proposal 03** (Level-Based Trading System). Must be implemented before level-based trading. See **Proposal 02** for detailed implementation.

#### 6. TUI CLI (Terminal User Interface and Monitoring)
- **Purpose**: Terminal-based interface for trading operations and monitoring
- **Protocol**: gRPC client (connects to Trading System gRPC server in runner)
- **Dependencies**: 
  - Terminal with color support
  - Keyboard input support
- **Note**: **Primary interface** - TUI IS the monitoring interface. No separate monitoring service needed.

### System Requirements

#### Hardware
- **CPU**: 2+ cores recommended
- **Memory**: 512MB+ RAM (1GB+ recommended)
- **Network**: Stable internet connection with low latency to exchange
- **Disk**: Minimal (config files, logs)

#### Software
- **Rust**: 1.70+ (stable)
- **Cargo**: Included with Rust
- **Operating System**: Linux, macOS, or Windows
- **Terminal**: ANSI-compatible terminal (for TUI CLI)

### Configuration Files

#### 1. Main Configuration File
- **Location**: `config/trading_system.json` (or specified path)
- **Contains**:
  - System configuration
  - Exchange configuration (API keys, endpoints)
  - Strategy configuration
  - Server configuration
  - Redis configuration
  - MongoDB configuration
  - Risk limits
  - Communication settings

#### 2. Level Configuration
- **Location**: MongoDB (primary) or JSON files (fallback)
- **Contains**: Level definitions, indicators, pricing rules
- **Format**: JSON (if using files)

#### 3. Environment Variables (Optional)
- **API Keys**: Can be provided via environment variables for security
- **Connection Strings**: Can override config file values
- **Example**:
  ```bash
  BINANCE_API_KEY=your_api_key
  BINANCE_API_SECRET=your_api_secret
  MONGODB_URI=mongodb://localhost:27017
  REDIS_URI=redis://localhost:6379
  ```

### Network Requirements

#### Outbound Connections
- **Binance REST API**: `https://api.binance.com` (production) or `https://testnet.binance.vision` (testnet)
- **Binance WebSocket**: `wss://stream.binance.com` (production) or `wss://testnet.binance.vision` (testnet)
- **MongoDB**: Connection to MongoDB instance (if enabled)
- **Redis**: Connection to Redis instance

#### Inbound Connections (if using TUI CLI or Web UI)
- **gRPC Server**: Port 50051 (default, configurable)
- **HTTP Server**: Port 8080 (default, configurable, if using web UI)

#### Firewall Rules
- Allow outbound HTTPS (443) to Binance
- Allow outbound WebSocket (443) to Binance
- Allow inbound connections to gRPC/HTTP ports (if accessing remotely)
- Allow connections to MongoDB and Redis (if remote)

### Security Requirements

#### API Key Security
- **Storage**: Encrypted storage or environment variables
- **Permissions**: Minimum required permissions (read, trade)
- **Rotation**: Regular key rotation recommended
- **Testnet**: Use testnet for development/testing

#### Network Security
- **TLS/SSL**: All external connections use TLS
- **Authentication**: gRPC authentication (if needed)
- **Authorization**: API key-based authentication with exchange

#### Data Security
- **Sensitive Data**: API keys, secrets stored securely
- **Logging**: Avoid logging sensitive data
- **Backup**: Encrypted backups for MongoDB (config only)

### Startup Sequence

1. **Start MongoDB** (if using persistence)
   ```bash
   mongod --dbpath /path/to/data
   ```

2. **Start Redis** (only if using web UI, not required for TUI CLI)
   ```bash
   redis-server
   ```

3. **Start Indicator Service** (if separate, optional)
   ```bash
   ./indicator_service --config config/indicators.json
   ```

4. **Start Trading System**
   ```bash
   ./trading_system --config config/trading_system.json
   ```

5. **Start TUI CLI** (for interaction and monitoring)
   ```bash
   ./trading-cli --endpoint http://localhost:50051
   ```

### Health Checks

Before starting trading, verify:

- [ ] MongoDB is running and accessible (if using persistence)
- [ ] Redis is running and accessible (only if using web UI, not required for TUI CLI)
- [ ] Exchange API credentials are valid
- [ ] Network connectivity to exchange
- [ ] Trading system gRPC server is listening
- [ ] All services are healthy (use health endpoints)
- [ ] Configuration is valid (use validation)
- [ ] Risk limits are configured
- [ ] At least one level is configured (if using level-based trading)

### Production Checklist

- [ ] MongoDB instance running with backups enabled (if using persistence)
- [ ] Redis instance running (only if using web UI, not required for TUI CLI)
- [ ] Exchange API credentials configured (production or testnet)
- [ ] Trading system compiled and tested
- [ ] Configuration files validated
- [ ] Risk limits configured appropriately
- [ ] Levels configured (if using level-based trading)
- [ ] Monitoring/alerting set up
- [ ] Logging configured
- [ ] Error handling tested
- [ ] Circuit breakers configured
- [ ] Graceful shutdown tested
- [ ] Network security configured
- [ ] API key security verified
- [ ] TUI CLI or Web UI accessible
- [ ] Health endpoints responding

### Minimal Setup (Development/Testing)

For development or testing with TUI CLI, minimal setup:

1. **Exchange API** (testnet credentials)
2. **Trading System** (can run without MongoDB initially)
3. **TUI CLI** (for interaction)

**Optional (only for web UI)**:
- **Redis** (only needed for web UI, not TUI CLI)
- **MongoDB** (can be added later when persistence is needed)
**Note**: TUI CLI connects directly to Trading System via gRPC (gRPC server in runner), so Redis is not required for TUI. TUI IS the monitoring interface.

### Quick Start Command Reference

```bash
# Start MongoDB (Docker example)
docker run -d -p 27017:27017 --name mongodb mongo:latest

# Start Redis (Docker example) - Only if using web UI, not required for TUI CLI
docker run -d -p 6379:6379 --name redis redis:latest

# Start Trading System
cargo run --release --bin trading_system -- --config config/trading_system.json

# Start TUI CLI (in another terminal)
cargo run --release --bin trading-cli -- --endpoint http://localhost:50051
```

## System Architecture

### High-Level Architecture (Monolith)

```
┌─────────────────────────────────────────────────────────┐
│              Trading System (Monolith)                  │
│                                                          │
│  ┌──────────────────┐      ┌──────────────────┐        │
│  │ Exchange Engine │      │ Market Data      │        │
│  │ (State Manager) │      │ Engine           │        │
│  │                 │      │ (State Manager) │        │
│  └────────┬────────┘      └────────┬─────────┘        │
│           │                        │                   │
│           │ Channel                │ Channel           │
│           │ (< 1μs latency)        │ (< 1μs latency)   │
│           │                        │                   │
│  ┌────────▼────────────────────────▼─────────┐        │
│  │      Trading Engine                       │        │
│  │  - Level Manager                          │        │
│  │  - Level Executor                         │        │
│  │  - Risk Manager (cached)                 │        │
│  │  - Indicator Subscriber (channel cache)  │        │
│  └────────┬──────────────────────────────────┘        │
│           │                                            │
│           │ Channel (in-process) OR gRPC (external)   │
│           │                                            │
└───────────┼────────────────────────────────────────────┘
            │
            │ gRPC Server (in runner)
            │
┌───────────▼──────────┐
│   TUI CLI            │ (External Process)
│   (gRPC Client)      │
│   - Monitoring       │
│   - Level Management │
│   - System Health    │
└──────────────────────┘
```

## Communication Patterns

### In-Process Channels (Monolith Internal Communication)

**Used For** (Ultra-low latency < 1μs):
1. **Exchange Engine → Trading System**
   - Orders, balances, positions, trades
   - High-frequency updates
   - Direct memory communication

2. **Market Data Engine → Trading System**
   - Orderbook updates
   - Public trades
   - High-frequency updates
   - Direct memory communication

3. **Indicator Service → Trading System** (in-process, same workspace)
   - Indicator values
   - High-frequency
   - Cached in trading system
   - Uses channels (not gRPC)

**Benefits**:
- Ultra-low latency (< 1μs vs 0.5-2ms for gRPC)
- No serialization overhead
- Direct memory access
- Simple and efficient
- Perfect for monolith architecture

### gRPC (External Services Only)

**Used For** (External processes):
1. **Trading System (gRPC Server in runner) → TUI CLI**
   - Level management
   - Monitoring data streaming
   - System health
   - External process (must use gRPC)

**Benefits**:
- Required for external processes
- Binary protocol (protobuf): 3-10x smaller than JSON
- HTTP/2 multiplexing: Lower latency
- Built-in backpressure: Flow control
- Connection pooling: Reuse connections
- Type-safe: Compile-time checks

### Redis Pub/Sub (Currently Used, To Be Removed)

**Current Usage**:
1. **Market Data Engine → Redis → Trading Engine** (to be migrated to channels per Proposal 04)

**Future Usage** (Optional, Feature Flag):
- Only if Web UI is implemented in the future
- Would be used for Web Server communication (if needed)
- Currently not planned - TUI CLI is the primary interface

**Note**: **Redis will be removed** from internal communication after Proposal 04 migration. Redis code kept behind feature flag for potential future Web UI use.

## Monolith Architecture

### Why Monolith?

**For Trading Systems**:
- **Ultra-low latency**: Channels provide < 1μs latency vs 0.5-2ms for gRPC
- **No serialization overhead**: Direct memory access
- **Single instance**: Trading bots are typically single-instance per account
- **Simpler deployment**: One binary, one process
- **Better performance**: Critical for high-frequency trading

**Communication Strategy**:
- **Internal (monolith)**: Channels for ultra-low latency
- **External (TUI CLI)**: gRPC for external processes
- **Future separation**: Can add gRPC layer if needed later

## Performance Optimizations Summary

### 1. Communication Layer

**Channels (In-Process)**:
- Ultra-low latency: < 1μs per update
- No serialization: Direct memory access
- No batching needed: Efficient enough without batching
- Simple: Direct communication

**gRPC (External)**:
- Connection pooling: Reuse connections
- Streaming: Real-time with backpressure
- Binary protocol: Efficient serialization
- Batching: Optional (for external services)

**Redis** (Only for web UI):
- Debouncing: Publish snapshots only every N seconds
- Batching: Batch multiple updates
- Incremental: Publish only changed data
- Connection pooling: Reuse connections

### 2. State Management

**Caching**:
- Risk metrics: Cache with TTL (e.g., 1 second)
- Indicators: In-memory cache (channel/gRPC updates)
- Market data: TTL-based cache (5-10 minutes)
- Check results: Cache identical requests (100ms TTL)

**Incremental Updates**:
- Risk metrics: Incremental updates (post-trade)
- State changes: Only recalculate if state changed
- Delta-based: Update only changed values

### 3. Data Persistence

**MongoDB**:
- Lazy initialization: Initialize client only when enabled
- Early return: Skip persistence if disabled (no overhead)
- Batch writes: Batch multiple updates
- Conditional compilation: Feature flags for compile-time optimization

### 4. Risk System

**Optimizations**:
- Metrics caching: Cache with TTL, state change detection
- Incremental updates: Post-trade delta updates
- Check result caching: Cache identical requests
- Async processing: Non-blocking checks

### 5. Market Data

**Optimizations**:
- TTL-based caching: Cache for 5-10 minutes
- Background refresh: Refresh before expiration
- Stale data fallback: Use stale cache if fresh fetch fails
- Connection pooling: Reuse HTTP connections

## Component Responsibilities

### Exchange Engine
- Fetches orders, balances, positions, trades
- Manages exchange state (in-memory, own state)
- Publishes via channels (trading system, ultra-low latency)
- Publishes via channels/gRPC (monitoring, if in-process/external)
- MongoDB persistence (disabled, lazy, own persistence)

### Market Data Engine
- Fetches orderbooks, public trades
- Manages market data state (in-memory, own state)
- Publishes via channels (trading system, ultra-low latency)
- Publishes via channels/gRPC (monitoring, if in-process/external)
- MongoDB persistence (disabled, lazy, own persistence)

### Trading Engine
- Level management and execution
- Risk checks (cached, incremental)
- Indicator subscription (channels, cached)
- Order placement
- Receives updates via channels (exchange, market data)
- Monitoring data aggregation

### Runner (gRPC Server)
- Orchestrates all services
- Creates channel pairs and wires services together
- Exposes gRPC server for TUI CLI
- Provides level management, monitoring data, system health endpoints

### TUI CLI (Terminal User Interface and Monitoring)
- Connects to Trading System via gRPC (external process)
- Receives monitoring data via gRPC streaming
- Displays real-time trading data, level management, system health
- **IS the monitoring interface** - no separate monitoring service needed

### Level Manager
- Level lifecycle management
- MongoDB persistence (config only)
- Level configuration

## Data Flow Examples

### Order Update Flow

```
Exchange WebSocket → ExchangeStateManager
    ├── Update in-memory state
    ├── Publish to Channel → Trading System (ultra-low latency)
    └── Persist to MongoDB (if enabled, lazy, batched)
```

### Orderbook Update Flow

```
Exchange WebSocket → MarketDataStateManager
    ├── Update in-memory state
    ├── Publish to Channel → Trading System (ultra-low latency)
    └── Persist to MongoDB (if enabled, lazy, batched)
```

### Level Execution Flow

```
Orderbook Update (Channel) → Trading Engine
    ├── Get indicators (from cache, channels)
    ├── Resolve level (exchange data priority, cached)
    ├── Risk check (cached metrics, incremental)
    ├── Place order (if allowed)
    ├── Update state
    └── Aggregate monitoring data (for gRPC server)
```

## Performance Characteristics

### Latency
- **In-process channels**: < 1μs (ultra-low latency)
- **External gRPC**: < 10ms (for TUI CLI)
- **Risk checks**: < 1ms (cached)
- **Indicator lookup**: < 0.1ms (in-memory cache)

### Throughput
- **Order updates**: 1000+ updates/second (channels, no batching needed)
- **Orderbook updates**: 100+ updates/second (channels, no batching needed)
- **Risk checks**: 1000+ checks/second (cached)
- **Level execution**: 100+ levels/second

### Memory
- **State managers**: In-memory only (efficient data structures)
- **Caches**: TTL-based, limited size
- **Batching buffers**: Configurable size limits

## Scalability Considerations

### Horizontal Scaling
- **Trading system**: Single instance (stateful)
- **Monitoring service**: Can scale (stateless)
- **Web server**: Can scale (subscribes to Redis)
- **Indicator service**: Can scale (separate service)

### Vertical Scaling
- **Connection pooling**: Efficient resource usage
- **Batching**: Reduce network overhead
- **Caching**: Reduce computation
- **Async processing**: Efficient CPU usage

## Error Handling & Resilience

### Communication Failures
- **Channels**: No failures (in-process, reliable)
- **gRPC**: Automatic reconnection with exponential backoff (external only)
- **Redis**: Retry with exponential backoff (only for web UI)
- **MongoDB**: Continue with in-memory state, retry connection

### State Synchronization
- **Exchange state is SOT**: Always prefer exchange data
- **MongoDB backup**: Restore config only if exchange unavailable
- **Reconciliation**: Periodic checks, log discrepancies

### Circuit Breakers
- **Risk system**: Circuit breakers for critical breaches
- **Graceful degradation**: Reduce activity before full stop
- **Recovery**: Auto-recovery when conditions improve

## Configuration

### Communication Configuration

```json
{
  "communication": {
    "channels": {
      "buffer_size": 1000,
      "backpressure_threshold": 500
    },
    "grpc": {
      "endpoint": "http://localhost:50051",
      "connection_pool_size": 10,
      "batch_size": 20,
      "batch_interval_ms": 50
    },
    "redis": {
      "enabled": false,
      "snapshot_interval_secs": 5,
      "batch_size": 10,
      "batch_interval_ms": 100
    }
  }
}
```

### Performance Configuration

```json
{
  "performance": {
    "caching": {
      "risk_metrics_ttl_secs": 1,
      "indicator_cache_ttl_secs": 60,
      "market_data_cache_ttl_secs": 300,
      "check_result_cache_ttl_ms": 100
    },
    "batching": {
      "grpc_batch_size": 20,
      "grpc_batch_interval_ms": 50,
      "redis_batch_size": 10,
      "redis_batch_interval_ms": 100
    }
  }
}
```

## Testing Strategy

### Performance Testing
- Load testing: High-frequency updates
- Latency testing: Measure end-to-end latency
- Throughput testing: Maximum updates per second
- Memory testing: Memory usage under load

### Integration Testing
- gRPC streaming: Test batching and backpressure
- Redis pub/sub: Test debouncing and batching
- Caching: Test cache hit/miss scenarios
- Error handling: Test failure scenarios

## Migration Path

### Phase 1: Foundation
1. ✅ Library separation (Proposal 01) - **COMPLETE**
2. Indicator Service (Proposal 02) - **Prerequisite for Proposal 03**
3. Basic state management

### Phase 2: Core Features
1. Level-based trading (Proposal 03) - **Requires Proposal 02 (Indicator Service)**
2. Redis to Channels migration (Proposal 04) - **After Proposal 03**
3. Data handling and persistence (Proposal 05)
4. LiveConfig Service (Proposal 06) - **After Proposal 05**
5. Risk system (Proposal 07)
6. Market data integration (Proposal 08)

### Phase 3: User Interface & Quality
1. TUI CLI and Monitoring (Proposal 09)
2. Testing Strategy (Proposal 10) - Comprehensive test coverage
3. Code Optimization and Performance Review (Proposal 11) - Final optimization pass

## Success Metrics

### Performance
- **Latency**: < 10ms for order updates
- **Throughput**: 1000+ updates/second
- **Memory**: < 500MB for state management
- **CPU**: < 50% under normal load

### Reliability
- **Uptime**: 99.9%+
- **Error rate**: < 0.1%
- **Recovery time**: < 30 seconds

### Scalability
- **Horizontal scaling**: Monitoring and web server
- **Vertical scaling**: Efficient resource usage
- **Load handling**: 10x normal load

## Notes

- **Monolith architecture**: Single process for ultra-low latency
- **Channels for internal**: Ultra-low latency (< 1μs) in-process communication
- **gRPC for external**: Only for external services (TUI CLI)
- **Performance optimized**: Caching, no serialization overhead for internal
- **Resilient**: Error handling, circuit breakers, graceful degradation
- **Maintainable**: Clear separation of concerns, modular design
- **Independent services**: Exchange Engine and Market Data Engine are self-contained

