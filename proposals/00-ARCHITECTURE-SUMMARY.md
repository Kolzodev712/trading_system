# Architecture Summary: Complete System Overview

## Overview

This document provides a comprehensive overview of the entire trading system architecture, incorporating all proposals with performance optimizations and efficient communication patterns.

## Recent Completions

### ✅ Proposal 03: Level-Based Trading System (COMPLETE)
- Full level-based trading implementation with JSON configuration
- Level executor, risk manager, cancellation manager
- Event loop for orderbook processing and level execution
- Indicator-based price/quantity resolution
- Level lifecycle management (Created, Posted, Filled, Cancelled, Paused, Removed)
- Both strategy and levels modes fully functional and tested

### ✅ Major Refactoring (COMPLETE)
- **Module Separation**: `strategies/` and `levels/` are independent modules with no cross-dependencies
- **State Management**: Renamed `strategy.rs` → `state.rs` to reflect `TradingState` as trading engine's state
- **Code Cleanup**: Removed unused `data/` module (legacy `OrderbookProcessor`)
- **Runner Organization**: Clean `strategy_runner()` and `level_runner()` functions, removed redundant health wrapper
- **Execution Report Handler**: Shared handler function eliminates code duplication
- **Configuration Separation**: Trading-specific configs separated from general bot config
- **Security**: `SecretString` type for API keys/secrets prevents accidental exposure in logs
- **Dependencies**: Added `rust_decimal_macros` for cleaner decimal literals

### ✅ Redis to Channels Migration (COMPLETE)
- **Channel Infrastructure**: Broadcast channels implemented in `trading_core` for ultra-low latency (< 1μs)
- **Market Data Engine**: Migrated from Redis to broadcast channels for publishing market data
- **Trading Engine**: Both `StrategyExecutor` and `LevelBasedEventLoop` now consume from channels instead of Redis
- **Indicator Engine**: Updated to use broadcast channels for market data updates
- **Fan-out Support**: Multiple consumers (Trading Engine, Indicator Engine) can subscribe to same channel
- **Redis Optional**: Redis dependency removed from Trading Engine, optional in Runner (behind feature flag for future Web UI)
- **Performance**: Eliminated Redis serialization overhead, direct memory access via channels

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

#### 2. Redis Instance (Optional - Feature Flag Only)
- **Purpose**: Not required for internal communication (channels are used instead)
- **Version**: Redis 6.0+ (or compatible) - only if Web UI feature is enabled
- **Connection**: Connection string optional in configuration (defaults to disabled)
- **Note**: **Redis is no longer used for internal communication**. All internal communication uses broadcast channels. Redis code is kept behind feature flag (`--features redis`) for potential future Web UI use only.

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

#### 5. Indicator Engine (In-Process)
- **Purpose**: Calculate and provide indicator values
- **Protocol**: Channels (in-process, same workspace)
- **Note**: Separate crate but same monolith, uses channels for communication
- **Status**: ✅ **COMPLETE** - Fully implemented with SMA, EMA, and composite indicators
- **Features**: Real-time calculation, caching, extensible framework, performance optimized

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
- ✅ **SecretString Type**: All API keys and secrets use `SecretString` wrapper
  - Prevents accidental exposure in logs, Debug, or Display output
  - Shows `***REDACTED***` instead of actual values
  - Requires explicit `.expose()` to access sensitive data
  - Still serializes actual values for config files (needed for loading)
- **Storage**: Encrypted storage or environment variables
- **Permissions**: Minimum required permissions (read, trade)
- **Rotation**: Regular key rotation recommended
- **Testnet**: Use testnet for development/testing

#### Network Security
- **TLS/SSL**: All external connections use TLS
- **Authentication**: gRPC authentication (if needed)
- **Authorization**: API key-based authentication with exchange

#### Data Security
- ✅ **Sensitive Data**: API keys, secrets use `SecretString` type (prevents accidental logging)
- **Logging**: `SecretString` automatically redacts in Debug/Display
- **Backup**: Encrypted backups for MongoDB (config only)
- **Access Control**: Sensitive data requires explicit `.expose()` call

### Startup Sequence

1. **Start MongoDB** (if using persistence)
   ```bash
   mongod --dbpath /path/to/data
   ```

2. **Start Redis** (only if using web UI, not required for TUI CLI)
   ```bash
   redis-server
   ```

3. **Start Trading System**
   ```bash
   ./trading_system --config config/trading_system.json
   ```

4. **Start TUI CLI** (for interaction and monitoring)
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
│           │                        ▼                   │
│           │              ┌──────────────────┐          │
│           │              │ Indicator Engine │ ✅ COMPLETE
│           │              │ (SMA, EMA, etc.) │          │
│           │              └────────┬─────────┘          │
│           │                       │ Channel             │
│           │                       │ (< 1μs latency)     │
│           │                       │                     │
│  ┌────────▼───────────────────────▼─────────┐        │
│  │      Trading Engine                       │        │
│  │  ✅ Level-based Trading (Proposal 03)     │        │
│  │  ✅ Strategy-based Trading                 │        │
│  │  - TradingState (shared state)            │        │
│  │  - Level Manager / Strategy Executor      │        │
│  │  - Risk Manager (cached)                 │        │
│  │  - Indicator Subscriber (channel cache)  │        │
│  │  - Position Reconciliation                │        │
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
1. **Exchange Engine → Trading System & Risk Engine**
   - Orders, balances, positions, trades
   - High-frequency updates
   - **Broadcast channel** for fan-out to multiple consumers
   - Direct memory communication
   - Each consumer tracks its own lag

2. **Market Data Engine → Trading System & Indicator Engine**
   - Orderbook updates
   - Public trades
   - High-frequency updates
   - **Broadcast channel** for fan-out to multiple consumers
   - Direct memory communication
   - Each consumer tracks its own lag

3. **Indicator Engine → Trading System** (in-process, same workspace)
   - ✅ **COMPLETE**: Real-time indicator values (SMA, EMA, composite)
   - High-frequency updates
   - **Broadcast channel** for fan-out
   - Cached in trading system (< 0.1ms lookup)
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

### Redis (Optional, Feature Flag Only)

**Status**: **Not used for internal communication** - All internal communication uses broadcast channels.

**Future Usage** (Optional, Feature Flag):
- Only if Web UI is implemented in the future
- Would be used for Web Server communication (if needed)
- Currently not planned - TUI CLI is the primary interface
- Can be enabled with `--features redis` if needed

**Note**: **Redis has been removed** from internal communication. All internal communication uses broadcast channels (< 1μs latency). Redis code kept behind feature flag for potential future Web UI use only.

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

## State Management Architecture

### Overview

The system uses a clear separation between **State Managers** (full state management with publishing) and **Caching Only** (fast lookup, no state management). This architecture ensures proper data flow, separation of concerns, and security.

### State Managers (Full State Management)

#### 1. ExchangeStateManager (Exchange Engine)

**Purpose**: Manages raw exchange data (balances, orders, trades from WebSocket). **Single point of contact with exchange for account data.**

**Responsibilities**:
- Manages in-memory state: balances, orders, trades from WebSocket
- Performs WS vs REST reconciliation (exchange-level reconciliation)
- Publishes all updates via broadcast channels to Trading Engine, Risk Engine, TUI
- MongoDB persistence capability (disabled initially, lazy evaluation)

**Key Principle**: Exchange Engine is the **only** component that calls exchange for account data. All other components rely on ExchangeStateManager.

**Data Flow**:
```
Exchange WebSocket → ExchangeStateManager
    ├── Update in-memory state
    ├── WS vs REST Reconciliation (periodic)
    ├── Publish via Broadcast Channel
    │   ├──→ Trading Engine (subscribes, updates TradingState)
    │   ├──→ Risk Engine (subscribes, updates risk cache)
    │   └──→ TUI (via gRPC aggregation)
    └── Persist to MongoDB* (if enabled)
```

#### 2. MarketDataStateManager (Market Data Engine)

**Purpose**: Manages market data state (orderbooks, public trades).

**Responsibilities**:
- Manages in-memory state: orderbooks, public trades
- Publishes all updates via broadcast channels to Indicator Engine, Trading Engine, TUI
- MongoDB persistence capability (disabled initially, lazy evaluation)

**Data Flow**:
```
Exchange WebSocket → MarketDataStateManager
    ├── Update in-memory state
    ├── Publish via Broadcast Channel
    │   ├──→ Indicator Engine (subscribes, updates indicator cache)
    │   ├──→ Trading Engine (for level execution)
    │   └──→ TUI (via gRPC aggregation)
    └── Persist to MongoDB* (if enabled)
```

#### 3. TradingState (Trading Engine)

**Purpose**: Manages trading-specific state (positions, active orders). **Derived from ExchangeStateManager via channels.**

**Responsibilities**:
- Manages in-memory state: positions (derived from balances), active orders (only orders we placed), pending orders
- **Derived from ExchangeStateManager** via channel subscriptions
- **Does NOT call exchange directly** - relies on ExchangeStateManager
- Reconciles with ExchangeStateManager (not with exchange directly)
- Used by strategies and levels for trading decisions

**Key Principle**: Trading Engine **does NOT** call exchange directly. It relies entirely on ExchangeStateManager for account data.

**Data Flow**:
```
ExchangeStateManager (via broadcast channel) → Trading Engine
    ├── ExchangeStateSubscriber receives updates
    ├── Updates TradingState
    │   ├── Update positions (derived from balances)
    │   ├── Update active orders (only orders we placed)
    │   └── Update pending orders
    ├── Periodic Reconciliation with ExchangeStateManager
    └── Used by strategies/levels for trading decisions
```

### Caching Only (No State Management)

#### 4. Indicator Engine Cache

**Purpose**: Caches calculated indicator values for fast lookup (< 0.1ms).

**Responsibilities**:
- Caches calculated indicator values (SMA, EMA, composite indicators)
- Receives updates from MarketDataStateManager via channels
- Fast lookup for trading decisions

**Not a State Manager**: No state management, no publishing, just caching.

#### 5. Risk Engine Cache

**Purpose**: Caches risk metrics for fast lookup. Can also call exchange directly for independent verification.

**Responsibilities**:
- Caches risk metrics (P&L, exposure, risk limits)
- Receives updates from ExchangeStateManager via channels
- **Can call exchange directly** for independent verification (second layer of security)
- Alarms if discrepancies detected between ExchangeStateManager and direct exchange calls

**Not a State Manager**: No state management, no publishing, just caching with verification capability.

**Key Principle**: Risk Engine acts as an **independent checker** that can verify state correctness by calling exchange directly.

#### 6. Asset Engine Cache

**Purpose**: Caches static asset metadata (symbols, trading rules) with TTL-based refresh.

**Responsibilities**:
- Caches static asset metadata (symbols, trading rules, exchange limits)
- TTL-based refresh from REST API (public endpoints, no authentication)
- No state management (static data)

**Not a State Manager**: No state management, no publishing, just caching static data.

### Key Architectural Principles

1. **Exchange Engine is the single point of contact** with exchange for account data
   - Only ExchangeStateManager calls exchange for balances, orders, trades
   - All other components rely on ExchangeStateManager

2. **Trading Engine relies on ExchangeStateManager**
   - Trading Engine does NOT call exchange directly
   - TradingState is derived from ExchangeStateManager via channels
   - TradingState reconciles with ExchangeStateManager (not with exchange)

3. **Risk Engine is an independent checker**
   - Receives data from ExchangeStateManager via channels
   - Can also call exchange directly for verification
   - Alarms if discrepancies detected

4. **State Managers** (Exchange, Market Data, Trading) manage full state and publish
5. **Caching Only** (Indicator, Risk, Asset) - fast lookup, no state management

### Reconciliation Architecture

#### Exchange-Level Reconciliation (ExchangeStateManager)
- **What**: WebSocket state vs REST state
- **Purpose**: Ensure ExchangeStateManager matches exchange authoritative state
- **Frequency**: Periodic (configurable interval)
- **Action**: Update ExchangeStateManager to match REST (REST is authoritative)

#### Trading-Level Reconciliation (TradingState)
- **What**: TradingState vs ExchangeStateManager
- **Purpose**: Ensure TradingState matches ExchangeStateManager state
- **Frequency**: Periodic (configurable interval)
- **Action**: Update TradingState to match ExchangeStateManager
- **Key Point**: TradingState reconciles with ExchangeStateManager (NOT with exchange directly)

#### Risk Engine Verification (Independent Checker)
- **What**: ExchangeStateManager state vs direct exchange calls
- **Purpose**: Independent verification of state correctness
- **Frequency**: Periodic (optional, configurable)
- **Action**: Alarm if discrepancies detected
- **Key Point**: Risk Engine can call exchange directly for verification

## Component Responsibilities

### Exchange Engine
- **ExchangeStateManager**: Manages raw exchange data (balances, orders, trades from WebSocket)
- **Single point of contact** with exchange for account data
- Performs WS vs REST reconciliation (exchange-level)
- Publishes all updates via broadcast channels (Trading Engine, Risk Engine, TUI)
- MongoDB persistence (disabled, lazy, own persistence)
- **Key Principle**: Only Exchange Engine calls exchange for account data

### Market Data Engine
- **MarketDataStateManager**: Manages market data state (orderbooks, public trades)
- Publishes all updates via broadcast channels (Indicator Engine, Trading Engine, TUI)
- MongoDB persistence (disabled, lazy, own persistence)
- No reconciliation needed (market data is public)

### Trading Engine
- ✅ **COMPLETE**: Level-based trading system (Proposal 03)
  - Level management and execution (`levels/manager.rs`, `levels/executor.rs`)
  - Level lifecycle (Created, Posted, Filled, Cancelled, Paused, Removed)
  - Indicator-based price/quantity resolution (`levels/resolver.rs`)
  - Risk checks per level (`levels/risk/manager.rs`, cached, incremental)
  - Cancellation manager (`levels/cancellation/manager.rs`)
  - Event loop (`levels/event_loop.rs`) for orderbook processing and level execution
  - Indicator subscriber (`levels/indicator_subscriber.rs`) for real-time indicator values
- ✅ **COMPLETE**: Strategy-based trading system
  - Strategy trait (`state.rs`) and implementations (`strategies/strategies.rs`)
  - Strategy executor (`strategies/executor.rs`) with Redis orderbook subscription
  - Strategy manager (`strategies/strategy_manager.rs`) for multiple strategies
  - Backtesting framework (`strategies/backtest.rs`)
- ✅ **State Management**: `TradingState` in `state.rs` (renamed from `strategy.rs`)
  - Positions tracking (`HashMap<Pair, Position>`) - derived from ExchangeStateManager balances
  - Active orders tracking (`HashMap<OrderId, Order>`) - only orders we placed
  - Pending orders tracking with timestamps
  - Shared by both strategies and levels modes
  - **Derived from ExchangeStateManager** via channel subscriptions
  - **Does NOT call exchange directly** - relies on ExchangeStateManager
- ✅ **Position Reconciliation**: `PositionReconciler` in `reconciliation.rs`
  - Reconciles TradingState with ExchangeStateManager (NOT with exchange directly)
  - Compares TradingState positions with ExchangeStateManager balances
  - Updates TradingState when discrepancies detected
  - Used by both modes
  - **Key Principle**: Trading Engine does NOT call exchange directly
- ✅ **Health Server**: Shared health monitoring (`health.rs`)
  - REST API for positions, orders, state
  - Used by both strategies and levels modes
- **Module Structure**:
  - `state.rs`: Shared `TradingState`, `Position`, `Strategy` trait
  - `strategies/`: Strategy implementations (independent module)
    - `strategies.rs`: BuyLowSellHigh implementation
    - `executor.rs`: Strategy execution loop
    - `strategy_manager.rs`: Multi-strategy management
    - `backtest.rs`: Backtesting framework
  - `levels/`: Level-based trading (independent module)
    - `manager.rs`: Level lifecycle management
    - `executor.rs`: Level execution
    - `event_loop.rs`: Main event loop
    - `resolver.rs`: Indicator resolution
    - `indicator_subscriber.rs`: Indicator value subscription
    - `risk/`: Risk management submodule
    - `cancellation/`: Cancellation logic submodule
  - `reconciliation.rs`: Position reconciliation (shared)
  - `health.rs`: Health server (shared)
- **Independence**: `strategies` and `levels` modules do NOT depend on each other
- **Common Logic**: Shared components extracted to `state`, `reconciliation`, `health`

### Indicator Engine
- ✅ **COMPLETE**: Real-time indicator calculation (SMA, EMA, composite indicators)
- **Caching Only** (not a state manager)
- Subscribes to MarketDataStateManager via channels
- Publishes indicator values via channels
- In-memory caching for fast lookup (< 0.1ms)
- Extensible framework for new indicators
- Performance optimized (SmolStr, efficient data structures)
- Comprehensive unit tests (39 tests)

### Risk Engine
- **Caching Only** (not a state manager)
- Caches risk metrics (P&L, exposure, risk limits)
- Receives updates from ExchangeStateManager via channels
- **Can call exchange directly** for independent verification (second layer of security)
- Alarms if discrepancies detected between ExchangeStateManager and direct exchange calls
- **Key Principle**: Risk Engine acts as independent checker

### Asset Engine
- **Caching Only** (not a state manager)
- Caches static asset metadata (symbols, trading rules, exchange limits)
- TTL-based refresh from REST API (public endpoints, no authentication)
- No state management (static data)

### Runner (Application Orchestrator)
- ✅ **Dual Mode Support**: Both strategy and levels modes
  - `strategy_runner()`: Complete strategy mode setup
  - `level_runner()`: Complete levels mode setup
  - Mode selection via command-line argument (`--mode strategy|levels`)
- ✅ **Shared Services**: `SharedServices` struct encapsulates common services
  - Trading client, execution report channel, shutdown manager
  - Market data handle, trading WebSocket handle, indicator handle
- ✅ **Execution Report Handler**: Shared handler function eliminates duplication
  - Common logic for receiving and processing execution reports
  - Mode-specific callbacks for strategy vs levels handling
- ✅ **Configuration Separation**:
  - `config/bot.json`: General bot config (system, exchange, server, redis, features)
  - `config/strategies.json`: Strategy-specific config
  - `config/levels.json`: Levels-specific config
- Orchestrates all services
- Creates channel pairs and wires services together
- Exposes gRPC server for TUI CLI (future)
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

## Data Distribution Architecture

### Exchange Engine WebSocket Stream Data

The Exchange Engine receives real-time data from the exchange via WebSocket (Binance User Data Stream):

1. **Balances** - Account balances (free, used, locked per asset)
2. **Open Orders** - Active orders (pending, partially filled, status updates)
3. **Trades** - Execution reports (fills, order status changes, trade history)

### System Data Requirements

#### 1. TUI (Terminal User Interface)
**Needs: ALL DATA** (comprehensive monitoring interface)

- **Orderbooks** (from Market Data Engine via channels)
  - Real-time price levels, depth, best bid/ask
  - Purpose: Display market conditions
- **Balances** (from Exchange Engine WS via channels → Trading Engine → gRPC)
  - Account balances, available funds, locked funds
  - Purpose: Display account status
- **Open Orders** (from Exchange Engine WS via channels → Trading Engine → gRPC)
  - Active orders, status, fills, pending orders
  - Purpose: Display order state
- **Trades** (from Exchange Engine WS via channels → Trading Engine → gRPC)
  - Execution history, fills, order updates
  - Purpose: Display trade history
- **Indicators** (from Indicator Engine via channels → Trading Engine → gRPC)
  - Technical indicators, calculated values
  - Purpose: Display technical analysis
- **Risk Statistics** (from Risk Engine → Trading Engine → gRPC)
  - P&L, exposure, risk metrics, limit status
  - Purpose: Display risk monitoring

**Why**: TUI is the primary monitoring interface - it needs all data for comprehensive display.

---

#### 2. Trading Engine
**Needs: Balances, Open Orders, Trades**

- **Balances** (from Exchange Engine WS via broadcast channel)
  - Purpose: Position sizing, available funds calculation, reconciliation
  - Usage: Level quantity resolution, position management
- **Open Orders** (from Exchange Engine WS via broadcast channel)
  - Purpose: Order state tracking, prevent duplicate orders, track active orders
  - Usage: Level execution, order lifecycle management
- **Trades** (from Exchange Engine WS via broadcast channel)
  - Purpose: Execution reports, position updates, trigger strategy logic
  - Usage: Update `TradingState`, trigger level fills, strategy callbacks

**Why**: Core trading logic requires account state and order execution data to make trading decisions.

---

#### 3. Risk Engine
**Needs: Balances, Open Orders, Trades**

- **Balances** (from Exchange Engine WS via broadcast channel)
  - Purpose: Calculate exposure, check balance thresholds, monitor available funds
  - Usage: Risk checks, exposure calculations, balance-based limits
- **Open Orders** (from Exchange Engine WS via broadcast channel)
  - Purpose: Calculate pending exposure, check position limits, monitor order count
  - Usage: Pre-trade risk checks, exposure limits, order count limits
- **Trades** (from Exchange Engine WS via broadcast channel)
  - Purpose: Calculate realized P&L, update risk metrics, track trade history
  - Usage: P&L calculations, daily loss limits, trade history analysis

**Why**: Risk calculations depend on account state, positions, and order exposure to prevent wipeouts.

---

#### 4. Indicator Engine
**Needs: NONE from Exchange Engine WS**

- **Orderbooks** (from Market Data Engine via channels)
  - Purpose: Price data for indicator calculations
  - Usage: SMA, EMA, composite indicators
- **Trades** (from Market Data Engine - public trades via channels)
  - Purpose: Trade data for volume-based indicators
  - Usage: Volume indicators, trade-based calculations

**Why**: Indicators use market data (public), not account-specific data (private). Indicator Engine is market-data focused.

---

#### 5. Level Manager/Executor
**Needs: Balances (indirectly)**

- **Balances** (from Exchange Engine WS → Trading Engine → Level Executor)
  - Purpose: Position sizing, quantity resolution, balance-based level creation
  - Usage: Level quantity calculation, balance percentage calculations

**Why**: Levels need balance info for sizing, but typically get it via Trading Engine state (not direct subscription).

---

### Data Flow Architecture

```
Exchange Engine WebSocket Stream
    ↓ (broadcast channel - single source)
    ├─→ Trading Engine
    │   ├─ Balances (for position sizing, reconciliation)
    │   ├─ Open Orders (for order state tracking)
    │   └─ Trades (for execution reports, position updates)
    │
    ├─→ Risk Engine
    │   ├─ Balances (for exposure calculation)
    │   ├─ Open Orders (for pending exposure)
    │   └─ Trades (for P&L calculation)
    │
    └─→ TUI (via gRPC from Trading Engine aggregation)
        ├─ Balances (display)
        ├─ Open Orders (display)
        └─ Trades (display)

Market Data Engine WebSocket Stream
    ↓ (broadcast channel - single source)
    ├─→ Indicator Engine
    │   └─ Orderbooks/Trades (for indicator calculations)
    │
    ├─→ Trading Engine
    │   └─ Orderbooks (for level execution, strategy decisions)
    │
    └─→ TUI (via gRPC from Trading Engine aggregation)
        └─ Orderbooks (display)

Indicator Engine
    ↓ (broadcast channel - calculated values)
    ├─→ Trading Engine
    │   └─ Indicator Values (for level resolution, strategy decisions)
    │
    └─→ TUI (via gRPC from Trading Engine aggregation)
        └─ Indicator Values (display)
```

### Channel Distribution Strategy

**For Exchange Engine WS Data:**
- **Broadcast Channel** (recommended)
  - Single broadcast from Exchange Engine
  - Trading Engine, Risk Engine subscribe independently
  - Each tracks its own lag
  - TUI receives aggregated data via gRPC (from Trading Engine)
  - **Pros**: Single source, efficient fan-out, no duplication
  - **Cons**: Need lag monitoring, slow consumers can drop messages (acceptable for TUI)

**For Market Data Engine:**
- **Broadcast Channel** (recommended)
  - Single broadcast from Market Data Engine
  - Indicator Engine, Trading Engine subscribe independently
  - Each tracks its own lag
  - **Pros**: Single source, efficient fan-out
  - **Cons**: Need lag monitoring

**For Indicator Engine:**
- **Broadcast Channel** (recommended)
  - Single broadcast from Indicator Engine
  - Trading Engine subscribes, caches values
  - TUI receives via gRPC aggregation
  - **Pros**: Single source, efficient fan-out

### Data Requirements Summary

| System | Balances | Open Orders | Trades | Orderbooks | Indicators | Risk Stats |
|--------|----------|-------------|--------|------------|-------------|------------|
| **TUI** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Trading Engine** | ✅ | ✅ | ✅ | ✅ (via Market Data) | ✅ (via Indicator Engine) | ❌ |
| **Risk Engine** | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ (calculates) |
| **Indicator Engine** | ❌ | ❌ | ❌ | ✅ | ❌ (calculates) | ❌ |
| **Level Manager** | ✅ (indirect) | ❌ | ❌ | ✅ (via Market Data) | ✅ (via Indicator Engine) | ❌ |

**Key Insights:**
- **TUI needs everything** - it's the comprehensive monitoring interface
- **Trading Engine needs account data** - balances, orders, trades for trading decisions
- **Risk Engine needs account data** - balances, orders, trades for risk calculations
- **Indicator Engine needs market data only** - orderbooks, public trades (no account data)
- **Level Manager needs indirect access** - gets data via Trading Engine state

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
- **Indicator Engine**: ✅ **COMPLETE** - In-process service (scales with monolith)

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
2. ✅ Indicator Engine (Proposal 02) - **COMPLETE**
3. Basic state management

### Phase 2: Core Features
1. ✅ Level-based trading (Proposal 03) - **COMPLETE**
   - Level executor, risk manager, cancellation manager
   - Event loop for orderbook processing
   - Indicator-based price/quantity resolution
   - Level lifecycle management
   - Both strategy and levels modes fully functional
2. ✅ Redis to Channels migration - **COMPLETE**
   - Broadcast channels implemented for all internal communication
   - Market Data Engine, Trading Engine, Indicator Engine all use channels
   - Redis dependency removed from Trading Engine
   - Ultra-low latency (< 1μs) communication
3. Data handling and persistence (Proposal 05)
4. MongoDB Persistence and Configuration Storage (Proposal 06) - **After Proposal 05**
5. LiveConfig Service (Proposal 06b) - **After Proposal 06**
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

## Recent Refactoring & Improvements

### Module Organization (✅ Complete)
- **Trading Engine State**: `state.rs` contains `TradingState`, `Position`, and `Strategy` trait
  - Previously named `strategy.rs`, renamed to better reflect shared state purpose
  - Used by both strategies and levels modes
- **Module Separation**: `strategies/` and `levels/` are independent modules
  - No cross-dependencies between strategies and levels
  - Common logic extracted to shared modules (`state`, `reconciliation`, `health`)
- **Code Cleanup**: Removed unused `data/` module
  - `OrderbookProcessor` was legacy code from before Redis architecture
  - Trading engine now processes orderbooks directly from Redis (one at a time)

### Security Improvements (✅ Complete)
- **SecretString Type**: Secure wrapper for API keys and secrets (`libs/trading_core/src/secret.rs`)
  - Prevents accidental exposure in Debug/Display (shows `***REDACTED***`)
  - Still serializes actual values for config files
  - Requires explicit `.expose()` to access sensitive data
  - All API keys and secrets now use `SecretString` instead of plain `SmolStr`

### Code Quality Improvements (✅ Complete)
- **Execution Report Handler**: Shared function eliminates duplication
  - Common logic extracted to `execution_report_handler()` function
  - Both modes use same handler with mode-specific callbacks
- **Runner Organization**: Clean separation of concerns
  - `strategy_runner()` and `level_runner()` functions in `runner/lib.rs`
  - `main.rs` is clean, just routes to appropriate runner
  - Removed redundant `runner/src/health.rs` wrapper
- **Dependencies**: Added `rust_decimal_macros` for cleaner decimal literals

### Configuration Architecture (✅ Complete)
- **Separated Configs**: Trading-specific configs separated from general bot config
  - `config/bot.json`: System-wide settings (exchange, server, redis, features)
  - `config/strategies.json`: Strategy-specific parameters
  - `config/levels.json`: Levels-specific parameters
- **Config Loading**: Logic in `runner` crate (where it belongs)
  - Not in `trading_core` (system-level concern, not library concern)

## Notes

- **Monolith architecture**: Single process for ultra-low latency
- **Channels for internal**: Ultra-low latency (< 1μs) in-process communication
- **gRPC for external**: Only for external services (TUI CLI)
- **Performance optimized**: Caching, no serialization overhead for internal
- **Resilient**: Error handling, circuit breakers, graceful degradation
- **Maintainable**: Clear separation of concerns, modular design
- **Independent services**: Exchange Engine and Market Data Engine are self-contained
- **Independent trading modes**: Strategies and levels modules are independent
- **Shared state**: `TradingState` is the trading engine's state, shared by both modes
- **Secure by default**: API keys and secrets use `SecretString` wrapper

