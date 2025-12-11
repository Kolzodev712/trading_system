# Architecture Summary: Complete System Overview

**Version**: 0.1  
**Last Updated**: December 2024

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

### ✅ Channel Configuration Centralization (COMPLETE)
- **ChannelConfig**: Centralized buffer size configuration in `trading_core::config::ChannelConfig`
- **Exchange-Agnostic**: Generic channel names (e.g., `websocket_inbound_capacity` not `binance_inbound_capacity`)
- **Centralized Management**: All channel buffer sizes defined in one place, configurable via `bot.json`
- **Architecture**: Only drivers and `run.rs` know about `ChannelConfig`; underlying clients receive primitive values
- **No Circular Dependencies**: `ChannelConfig` lives in `trading_core` (shared library), preventing dependency cycles
- **Multi-Exchange Support**: Design supports multiple exchanges without exchange-specific naming
- **Default Values**: Sensible defaults for all channels (market data: 1000, WebSocket: 1024, etc.)

### ✅ Risk System (Proposal 07) - COMPLETE
- **Comprehensive Risk Management**: Top-level risk system in `trading_engine::risk` (applies to both levels and strategies)
- **Pre-Trade Checks**: Circuit breaker, daily loss limits, balance, position limits, order count - all checked before every order
- **Post-Trade Checks**: Integrated after order execution to monitor for limit breaches
- **Periodic Checks**: Background monitoring every second to catch issues even when no trades occur
- **Circuit Breaker**: Three-state system (Closed/Open/HalfOpen) with automatic recovery after timeout (5 minutes default)
- **Daily P&L Tracking**: Automatic tracking with midnight reset, per-pair and global limits
- **Risk Metrics**: Real-time calculation of unrealized P&L, realized P&L, daily P&L, exposure, position size
- **SymbolRegistry Integration**: Uses SymbolRegistry for proper symbol-to-pair conversion in balance checks
- **Risk Limits**: Configurable limits for position size, order count, daily loss (global and per-pair)
- **Integration**: Fully integrated into level-based event loop with price tracking for periodic checks

### ✅ Asset Manager Engine (Proposal 08) - COMPLETE
- **Asset Engine Service**: New service dedicated to fetching public exchange market data via unsigned REST calls
- **Bootstrap Integration**: Market data fetched at application startup for configured pairs
- **TTL-Based Caching**: 5-minute default TTL with automatic refresh on stale data
- **Priority System**: Exchange values (trading rules, limits) take priority over config values, with graceful config fallback
- **Order Validation**: Prices rounded to tick size, quantities rounded to step size using exchange data
- **Min/Max Validations**: Order size, price, and notional limits enforced from exchange data when available
- **Integration Points**: Used in both level resolver (`IndicatorResolver`) and strategy executor for order validation
- **Binance Implementation**: `BinancePublicClient` for fetching exchange info, trading rules, and exchange limits
- **Centralized Access**: `AssetService` included in `SharedServices` for easy access across all trading components
- **Graceful Degradation**: System works with or without Asset Engine data (falls back to config values)

## Prerequisites & Infrastructure Requirements

To run the trading system in production, the following infrastructure and services must be set up and running:

### Required Services

#### 1. MongoDB Instance (Optional - Configuration Persistence)
- **Purpose**: Configuration persistence (levels, risk limits, indicators, strategies)
- **Version**: MongoDB 3.0+ (or compatible)
- **Connection**: Connection string configured in `bot.json` (defaults to disabled)
- **Collections**: 
  - `bot_configs` - Bot configurations with levels and risk limits
  - `indicators` - Indicator definitions (simple and composite)
  - `strategies` - Strategy configurations
- **Persistence**: Can be enabled/disabled via `bot.json` (`mongo.enabled`)
- **Fallback**: System falls back to file-based config if MongoDB unavailable
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

#### 6. TUI CLI (Terminal User Interface and Monitoring) - ✅ COMPLETE
- **Purpose**: Terminal-based interface for trading operations and monitoring
- **Protocol**: gRPC client (connects to Trading System gRPC server in runner)
- **Dependencies**: 
  - Terminal with color support
  - Keyboard input support
- **Status**: ✅ **COMPLETE** - Full implementation with real-time monitoring, level management, and trading control
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
   - Buffer size configured via `ChannelConfig.exchange_update_capacity`

2. **Market Data Engine → Trading System & Indicator Engine**
   - Orderbook updates
   - Public trades
   - High-frequency updates
   - **Broadcast channel** for fan-out to multiple consumers
   - Direct memory communication
   - Each consumer tracks its own lag
   - Buffer size configured via `ChannelConfig.market_data_capacity`

3. **Indicator Engine → Trading System** (in-process, same workspace)
   - ✅ **COMPLETE**: Real-time indicator values (SMA, EMA, composite)
   - High-frequency updates
   - **Broadcast channel** for fan-out
   - Cached in trading system (< 0.1ms lookup)
   - Uses channels (not gRPC)
   - Buffer size configured via `ChannelConfig.indicator_update_capacity`

**Channel Configuration**:
- ✅ **COMPLETE**: All channel buffer sizes centralized in `ChannelConfig` (`trading_core::config::ChannelConfig`)
- Exchange-agnostic naming (e.g., `websocket_inbound_capacity` not `binance_inbound_capacity`)
- Configurable via `bot.json` with sensible defaults
- Only drivers and `run.rs` know about `ChannelConfig`; underlying clients receive primitive values
- Prevents circular dependencies (config in `trading_core`, not `runner`)

**Benefits**:
- Ultra-low latency (< 1μs vs 0.5-2ms for gRPC)
- No serialization overhead
- Direct memory access
- Simple and efficient
- Perfect for monolith architecture
- Centralized configuration for easy tuning

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

### Communication Layer

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

### State Management

**Caching**:
- Risk metrics: Cache with TTL (e.g., 1 second)
- Indicators: In-memory cache (channel/gRPC updates)
- Market data: TTL-based cache (5-10 minutes)
- Check results: Cache identical requests (100ms TTL)

**Incremental Updates**:
- Risk metrics: Incremental updates (post-trade)
- State changes: Only recalculate if state changed
- Delta-based: Update only changed values

### Code Quality

**Optimizations Completed**:
- Reduced unnecessary clones across all services
- Improved lock contention by minimizing lock hold times
- Optimized memory usage and data structures
- Module splitting for better maintainability (TUI screens, gRPC service)

## State Management Architecture

### Overview

The system uses a clear separation between **State Managers** (full state management with publishing) and **Caching Only** (fast lookup, no state management). This architecture ensures proper data flow, separation of concerns, and security.

**Key Components**:
- **SymbolRegistry**: Centralized bidirectional mapping between `Symbol` (e.g., "BTCUSDT") and `Pair` (e.g., BTC/USDT) for reliable conversions across all services
- **State Managers**: Full state management with in-memory state, reconciliation, and channel publishing
- **Caching Services**: Fast lookup caches for indicators, risk metrics, and asset metadata

### State Managers (Full State Management)

#### 1. ExchangeStateManager (Exchange Engine)

**Purpose**: Manages raw exchange data (balances, orders, trades from WebSocket). **Single point of contact with exchange for account data.**

**Location**: `src/exchange_engine/src/state/`

**Structure**:
- `state.rs`: `ExchangeState` - in-memory state structure with `SymbolRegistry` for symbol↔pair conversions
- `manager.rs`: `ExchangeStateManager` - orchestrates state updates, reconciliation, and publishing
- `publisher.rs`: `ExchangeStatePublisher` - publishes updates via broadcast channels
- `reconciler.rs`: `StateReconciler` - performs WS vs REST reconciliation

**Responsibilities**:
- Manages in-memory state: balances, orders, trades from WebSocket
- Uses `SymbolRegistry` for reliable `Symbol` ↔ `Pair` conversions
- Performs WS vs REST reconciliation (exchange-level reconciliation)
  - **Bootstrap reconciliation**: Initial REST fetch on startup to initialize state
  - **Periodic reconciliation**: Configurable interval (default: 180000ms = 3 minutes)
  - REST is authoritative - WS state is updated to match REST if discrepancies found
- Publishes all updates via broadcast channels to Trading Engine, Risk Engine, TUI
- Processes WebSocket updates:
  - `BalanceUpdate` messages update balances (delta-based)
  - `ExecutionReport` messages update orders and create trades (for filled/partially filled orders)
  - `OutboundAccountPosition` messages update all balances
- MongoDB persistence capability (disabled initially, lazy evaluation)

**Key Principle**: Exchange Engine is the **only** component that calls exchange for account data. All other components rely on ExchangeStateManager.

**Data Flow**:
```
Exchange WebSocket → ExchangeStateManager
    ├── Update in-memory state (using SymbolRegistry for conversions)
    ├── WS vs REST Reconciliation (bootstrap + periodic)
    │   ├── Bootstrap: Initial REST fetch on startup
    │   └── Periodic: Configurable interval (default: 180000ms)
    ├── Publish via Broadcast Channel
    │   ├──→ Trading Engine (subscribes, updates TradingState)
    │   ├──→ Risk Engine (subscribes, updates risk cache)
    │   └──→ TUI (via gRPC aggregation)
    └── Persist to MongoDB* (if enabled)
```

#### 2. MarketDataStateManager (Market Data Engine)

**Purpose**: Manages market data state (orderbooks, public trades).

**Location**: `src/market_data_engine/src/state/`

**Structure**:
- `state.rs`: `MarketDataState` - in-memory state structure
- `manager.rs`: `MarketDataStateManager` - orchestrates state updates and publishing
- `publisher.rs`: `MarketDataStatePublisher` - publishes updates via broadcast channels

**Responsibilities**:
- Manages in-memory state: orderbooks, public trades
- Uses `SymbolRegistry` for reliable `Symbol` ↔ `Pair` conversions
- Publishes all updates via broadcast channels to Indicator Engine, Trading Engine, TUI
  - Updates include `Pair` information for proper routing
- MongoDB persistence capability (disabled initially, lazy evaluation)

**Data Flow**:
```
Exchange WebSocket → MarketDataStateManager
    ├── Update in-memory state (using SymbolRegistry for conversions)
    ├── Publish via Broadcast Channel (with Pair information)
    │   ├──→ Indicator Engine (subscribes, updates indicator cache)
    │   ├──→ Trading Engine (for level execution)
    │   └──→ TUI (via gRPC aggregation)
    └── Persist to MongoDB* (if enabled)
```

#### 3. TradingStateManager (Trading Engine)

**Purpose**: Manages trading-specific state (positions, active orders). **Derived from ExchangeStateManager via channels.**

**Location**: `src/trading_engine/src/state/`

**Structure**:
- `state.rs`: `TradingState` - in-memory state structure (positions, active orders, pending orders)
- `manager.rs`: `TradingStateManager` - orchestrates state updates, subscription, and reconciliation
- `subscriber.rs`: `ExchangeStateSubscriber` - subscribes to `ExchangeStateManager` updates via channels
- `reconciler.rs`: `PositionReconciler` - reconciles `TradingState` with `ExchangeStateManager` (not with exchange directly)

**Responsibilities**:
- Manages in-memory state: positions (derived from balances), active orders (only orders we placed), pending orders
- **Derived from ExchangeStateManager** via channel subscriptions
  - `ExchangeStateSubscriber` listens to `ExchangeUpdate` messages (Order, Balance, Trade)
  - Updates `TradingState` based on received updates
  - Only tracks orders we placed (filters out external orders)
- **Does NOT call exchange directly** - relies on ExchangeStateManager
- Reconciles with ExchangeStateManager (not with exchange directly)
  - **Periodic reconciliation**: Configurable interval (default: 180000ms = 3 minutes)
  - Compares positions and active orders with `ExchangeStateManager`
  - Updates `TradingState` if discrepancies found
  - Handles channel lag scenarios (if messages were dropped)
- Used by strategies and levels for trading decisions
- Provides methods: `add_order()`, `update_order()`, `remove_order()`, `reconcile()`

**Key Principle**: Trading Engine **does NOT** call exchange directly. It relies entirely on ExchangeStateManager for account data.

**Data Flow**:
```
ExchangeStateManager (via broadcast channel) → Trading Engine
    ├── ExchangeStateSubscriber receives updates
    │   ├── Order updates → Update TradingState.active_orders
    │   ├── Balance updates → Derive positions (future: balance-based position derivation)
    │   └── Trade updates → Update positions and order fills
    ├── TradingStateManager orchestrates updates
    ├── Periodic Reconciliation with ExchangeStateManager
    │   ├── Compare positions (TradingState vs ExchangeStateManager balances)
    │   ├── Compare active orders (TradingState vs ExchangeStateManager orders)
    │   └── Update TradingState if discrepancies found
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

### SymbolRegistry

**Purpose**: Centralized bidirectional mapping between `Symbol` (e.g., "BTCUSDT") and `Pair` (e.g., BTC/USDT).

**Location**: `libs/trading_core/src/registry/symbol.rs`

**Why Needed**:
- Exchanges provide symbols as strings (e.g., "BTCUSDT")
- System uses `Pair` types for clearer base/quote separation
- Avoids unreliable parsing logic (`Pair::from_symbol` was removed)
- Ensures consistency across all services

**Structure**:
- `symbol_to_pair: HashMap<Symbol, Pair>` - forward lookup
- `pair_to_symbol: HashMap<Pair, Symbol>` - reverse lookup

**Usage**:
- Each service creates its own `SymbolRegistry` from configuration
- `ExchangeStateManager` uses registry for symbol↔pair conversions
- `MarketDataStateManager` uses registry for symbol↔pair conversions
- Services are decoupled - each manages its own registry instance

**Methods**:
- `new()` - Create empty registry
- `from_pairs(pairs: Vec<Pair>)` - Create from pair list (auto-generates symbols)
- `register(symbol, pair)` - Add bidirectional mapping
- `get_pair(symbol)` - Get Pair from Symbol
- `get_symbol(pair)` - Get Symbol from Pair

### Reconciliation Architecture

#### Exchange-Level Reconciliation (ExchangeStateManager)

**What**: WebSocket state vs REST state

**Purpose**: Ensure `ExchangeStateManager` matches exchange authoritative state (REST is authoritative)

**Implementation**: `src/exchange_engine/src/state/reconciler.rs` - `StateReconciler`

**Process**:
1. **Bootstrap Reconciliation** (on startup):
   - Fetches balances, orders, trades from REST API
   - Initializes `ExchangeState` with REST data
   - Logs fetched counts and any discrepancies
   - Runs before WebSocket connection is established

2. **Periodic Reconciliation** (ongoing):
   - Configurable interval (default: 180000ms = 3 minutes)
   - Fetches balances, orders, trades from REST API
   - Compares with current WebSocket state
   - Updates `ExchangeState` to match REST if discrepancies found
   - Logs all reconciliation activities (info level)

**Configuration**:
- `reconciliation.exchange_level_interval_ms` - Interval in milliseconds (default: 180000)
- `reconciliation.enabled` - Enable/disable reconciliation (default: true)

**Logging**:
- Info logs for bootstrap start, fetch operations, comparisons, and results
- Warning logs for discrepancies found
- Info logs for periodic reconciliation runs

#### Trading-Level Reconciliation (TradingStateManager)

**What**: `TradingState` vs `ExchangeStateManager`

**Purpose**: Ensure `TradingState` matches `ExchangeStateManager` state (catches channel lag, subscriber bugs, calculation errors)

**Implementation**: `src/trading_engine/src/state/reconciler.rs` - `PositionReconciler`

**Process**:
1. **Periodic Reconciliation** (ongoing):
   - Configurable interval (default: 180000ms = 3 minutes)
   - Compares `TradingState` positions with `ExchangeStateManager` balances
   - Compares `TradingState` active orders with `ExchangeStateManager` orders
   - Updates `TradingState` to match `ExchangeStateManager` if discrepancies found
   - Logs all reconciliation activities (info level)

**Key Points**:
- `TradingState` reconciles with `ExchangeStateManager` (NOT with exchange directly)
- Trading Engine does NOT call exchange directly
- Handles channel lag scenarios (if messages were dropped)
- Safety net for subscriber bugs or calculation errors

**Configuration**:
- `reconciliation.trading_level_interval_ms` - Interval in milliseconds (default: 180000)
- `reconciliation.enabled` - Enable/disable reconciliation (default: true)

**Logging**:
- Info logs for reconciliation start, comparisons, and results
- Warning logs for discrepancies found
- Info logs for successful reconciliation (no discrepancies)

#### Risk Engine Verification (Independent Checker)

**What**: `ExchangeStateManager` state vs direct exchange calls

**Purpose**: Independent verification of state correctness (second layer of security)

**Frequency**: Periodic (optional, configurable) - **Not yet implemented**

**Action**: Alarm if discrepancies detected

**Key Point**: Risk Engine can call exchange directly for verification (independent of `ExchangeStateManager`)

## Component Responsibilities

### Exchange Engine

**Structure**:
- **Top-Level Runner** (`src/exchange_engine/src/run.rs`):
  - Exchange-agnostic entry point
  - Creates channels, `SymbolRegistry`, `TradingClient`, `ExchangeStateManager`
  - Configures reconciliation (bootstrap + periodic)
  - Creates exchange-specific driver and spawns its event loop
  - Returns state manager and channels for external use

- **Exchange-Specific Drivers** (`src/exchange_engine/src/exchanges/{exchange}/driver.rs`):
  - Implements `ExchangeDriver` trait (defined in `libs/trading_core/src/traits/exchange_driver.rs`)
  - Handles protocol-specific communication and WebSocket management
  - Integrates with `ExchangeStateManager` to update state on WebSocket events
  - Each exchange has its own driver (e.g., `BinanceDriver`)

- **State Management** (`src/exchange_engine/src/state/`):
  - `ExchangeStateManager`: Orchestrates state updates, reconciliation, and publishing
  - `ExchangeState`: In-memory state with `SymbolRegistry` for conversions
  - `ExchangeStatePublisher`: Publishes updates via broadcast channels
  - `StateReconciler`: Performs WS vs REST reconciliation

**Key Responsibilities**:
- **ExchangeStateManager**: Manages raw exchange data (balances, orders, trades from WebSocket)
- **Single point of contact** with exchange for account data
- Performs WS vs REST reconciliation (exchange-level):
  - Bootstrap reconciliation on startup (REST fetch)
  - Periodic reconciliation at configurable interval (default: 180000ms)
- Publishes all updates via broadcast channels (Trading Engine, Risk Engine, TUI)
- Processes WebSocket updates:
  - `BalanceUpdate` messages (delta-based balance updates)
  - `ExecutionReport` messages (order updates and trade creation)
  - `OutboundAccountPosition` messages (full balance snapshot)
- MongoDB persistence (disabled, lazy, own persistence)
- **Key Principle**: Only Exchange Engine calls exchange for account data

**Architecture**:
```
run.rs (exchange-agnostic)
    ├── Creates channels, SymbolRegistry, TradingClient
    ├── Creates ExchangeStateManager
    ├── Bootstrap reconciliation (REST fetch)
    ├── Starts periodic reconciliation
    └── Creates exchange-specific driver
        └── driver.run() (exchange-specific)
            ├── Sets state manager on client
            └── Starts WebSocket event loop
                └── Updates ExchangeStateManager on WebSocket events
```

### Market Data Engine

**Structure**:
- **State Management** (`src/market_data_engine/src/state/`):
  - `MarketDataStateManager`: Orchestrates state updates and publishing
  - `MarketDataState`: In-memory state structure (orderbooks, public trades)
  - `MarketDataStatePublisher`: Publishes updates via broadcast channels
  - Uses `SymbolRegistry` for reliable `Symbol` ↔ `Pair` conversions

**Key Responsibilities**:
- **MarketDataStateManager**: Manages market data state (orderbooks, public trades)
- Uses `SymbolRegistry` for symbol↔pair conversions
- Publishes all updates via broadcast channels (Indicator Engine, Trading Engine, TUI)
  - Updates include `Pair` information for proper routing
- MongoDB persistence (disabled, lazy, own persistence)
- No reconciliation needed (market data is public)

### Trading Engine

**State Management** (`src/trading_engine/src/state/`):
- ✅ **TradingStateManager**: Orchestrates state updates, subscription, and reconciliation
  - Manages `TradingState` (in-memory state)
  - Runs `ExchangeStateSubscriber` to receive updates from `ExchangeStateManager`
  - Provides `reconcile()` method for periodic reconciliation
  - Provides methods: `add_order()`, `update_order()`, `remove_order()`
- ✅ **TradingState** (`state/state.rs`):
  - Positions tracking (`HashMap<Pair, Position>`) - derived from ExchangeStateManager balances
  - Active orders tracking (`HashMap<OrderId, Order>`) - only orders we placed
  - Pending orders tracking with timestamps
  - Shared by both strategies and levels modes
  - **Derived from ExchangeStateManager** via channel subscriptions
  - **Does NOT call exchange directly** - relies on ExchangeStateManager
- ✅ **ExchangeStateSubscriber** (`state/subscriber.rs`):
  - Subscribes to `ExchangeStateManager` updates via broadcast channels
  - Handles `ExchangeUpdate::Order`, `ExchangeUpdate::Balance`, `ExchangeUpdate::Trade`
  - Updates `TradingState` based on received updates
  - Handles channel lag (`RecvError::Lagged`) with warnings
- ✅ **PositionReconciler** (`state/reconciler.rs`):
  - Reconciles `TradingState` with `ExchangeStateManager` (NOT with exchange directly)
  - Compares positions and active orders
  - Updates `TradingState` when discrepancies detected
  - Periodic reconciliation at configurable interval (default: 180000ms)
  - Used by both strategies and levels modes
  - **Key Principle**: Trading Engine does NOT call exchange directly

**Trading Modes**:
- ✅ **COMPLETE**: Level-based trading system (Proposal 03)
  - Level management and execution (`levels/manager.rs`, `levels/executor.rs`)
  - Level lifecycle (Created, Posted, Filled, Cancelled, Paused, Removed)
  - Indicator-based price/quantity resolution (`levels/resolver.rs`)
  - Risk checks per level (`levels/risk/manager.rs`, cached, incremental)
  - Cancellation manager (`levels/cancellation/manager.rs`)
  - Event loop (`levels/event_loop.rs`) for orderbook processing and level execution
  - Indicator subscriber (`levels/indicator_subscriber.rs`) for real-time indicator values
  - Uses `TradingStateManager` for all state interactions
- ✅ **COMPLETE**: Strategy-based trading system
  - Strategy trait (`state/state.rs`) and implementations (`strategies/strategies.rs`)
  - Strategy executor (`strategies/executor.rs`) with channel-based orderbook subscription
  - Strategy manager (`strategies/strategy_manager.rs`) for multiple strategies
  - Backtesting framework (`strategies/backtest.rs`)
  - Uses `TradingStateManager` for all state interactions

**Module Structure**:
- `state/`: State management module
  - `state.rs`: `TradingState`, `Position`, `Strategy` trait
  - `manager.rs`: `TradingStateManager` - orchestrates state management
  - `subscriber.rs`: `ExchangeStateSubscriber` - subscribes to ExchangeStateManager
  - `reconciler.rs`: `PositionReconciler` - reconciles TradingState with ExchangeStateManager
- `strategies/`: Strategy implementations (independent module)
  - `strategies.rs`: BuyLowSellHigh implementation
  - `executor.rs`: Strategy execution loop (uses `TradingStateManager`)
  - `strategy_manager.rs`: Multi-strategy management
  - `backtest.rs`: Backtesting framework
- `levels/`: Level-based trading (independent module)
  - `manager.rs`: Level lifecycle management
  - `executor.rs`: Level execution
  - `event_loop.rs`: Main event loop (uses `TradingStateManager`)
  - `resolver.rs`: Indicator resolution
  - `indicator_subscriber.rs`: Indicator value subscription
  - `risk/`: Risk management submodule
  - `cancellation/`: Cancellation logic submodule
- `health.rs`: Health server (shared)
- **Independence**: `strategies` and `levels` modules do NOT depend on each other
- **Common Logic**: Shared components extracted to `state/` module

### Indicator Engine
- ✅ **COMPLETE**: Real-time indicator calculation (SMA, EMA, composite indicators)
- **Caching Only** (not a state manager)
- Subscribes to `MarketDataStateManager` via channels
- Publishes indicator values via channels
- In-memory caching for fast lookup (< 0.1ms)
  - Stores historical indicator values (up to 100 per indicator)
  - Efficient pair-based and name-based lookups
  - `pair_index` and `name_index` for fast queries
- Extensible framework for new indicators
- Performance optimized (SmolStr, efficient data structures)
- Comprehensive unit tests (39 tests)
- Methods: `get_value()`, `get_history()`, `get_values_for_pair()`, `get_values_by_name()`, `get_value_by_pair_and_name()`

### Risk Engine
- **Caching Only** (not a state manager)
- Caches risk metrics (P&L, exposure, risk limits)
- Receives updates from ExchangeStateManager via channels
- **Can call exchange directly** for independent verification (second layer of security)
- Alarms if discrepancies detected between ExchangeStateManager and direct exchange calls
- **Key Principle**: Risk Engine acts as independent checker

### Asset Engine
- ✅ **COMPLETE**: Asset Manager Engine (Proposal 08)
- **Caching Only** (not a state manager)
- Caches static asset metadata (symbols, trading rules, exchange limits)
- TTL-based refresh from REST API (public endpoints, no authentication)
- Fetches market data at bootstrap via `AssetService`
- Priority system: Exchange values override config values, with config fallback
- Used in level resolver and strategy executor for order validation and rounding
- Binance implementation with unsigned REST client
- No state management (static data)

### Runner (Application Orchestrator)
- ✅ **Dual Mode Support**: Both strategy and levels modes
  - `strategy_runner()`: Complete strategy mode setup
  - `level_runner()`: Complete levels mode setup
  - Mode selection via command-line argument (`--mode strategy|levels`)
- ✅ **Shared Services**: `SharedServices` struct encapsulates common services
  - Trading client, execution report channel, shutdown manager
  - Market data handle, trading WebSocket handle, indicator handle
  - Indicator receiver (`indicator_rx`) for shared indicator channel
- ✅ **Execution Report Handler**: Shared handler function eliminates duplication
  - Common logic for receiving and processing execution reports
  - Mode-specific callbacks for strategy vs levels handling
- ✅ **Configuration Separation**:
  - `config/bot.json`: General bot config (system, exchange, server, redis, features, channels)
  - `config/strategies.json`: Strategy-specific config
  - `config/levels.json`: Levels-specific config
- ✅ **Channel Configuration**: Centralized channel buffer sizes
  - Loads `ChannelConfig` from `bot.json`
  - Passes `ChannelConfig` to `exchange_engine::run()` and `MarketDataDriver::new()`
  - All channel creations use values from `ChannelConfig`
  - Exchange-agnostic design supports multiple exchanges
- ✅ **Exchange-Agnostic Initialization**: All initialization functions handle exchange config internally
  - `initialize_market_data_engine()`: Extracts exchange config from `BotConfig` internally
  - `initialize_exchange_engine()`: Extracts exchange config from `BotConfig` internally
  - `initialize_indicator_service()`: Extracts pair from `BotConfig` internally
  - `collect_configured_pairs()`: Takes `BotConfig` instead of exchange-specific pair
  - Scalable: Adding new exchanges only requires adding branches in initialization functions
- ✅ **Shared Indicator Channel**: Single channel for indicator updates
  - Channel created in `main.rs` before service initialization
  - Sender passed to indicator service, receiver passed to runners via `SharedServices`
  - Prevents channel closure errors by ensuring receiver stays alive
- Orchestrates all services
- Creates channel pairs and wires services together
- Exposes gRPC server for TUI CLI (future)
- Provides level management, monitoring data, system health endpoints

### TUI CLI (Terminal User Interface and Monitoring) - ✅ COMPLETE
- ✅ **COMPLETE**: Full TUI implementation with `ratatui` framework
- **Architecture**: External process connecting to Trading System via gRPC
- **Communication**: gRPC streaming for real-time monitoring data updates
- **Primary Interface**: TUI IS the monitoring interface - no separate monitoring service needed

**Features Implemented**:
- ✅ **Main Menu**: Trading status display and navigation
- ✅ **Trading Monitor Screen**: Real-time display of:
  - Open orders (with level mapping and order types)
  - Account balances
  - Recent trades
  - Indicator values (real-time from Indicator Engine)
  - Active levels (with status, enabled state, active orders)
- ✅ **Level Management**:
  - Level selection and navigation (↑/↓ keys)
  - Level toggle (T key) - enable/disable levels with automatic order cancellation
  - Level details view (L key) - popup overlay with full level information
  - Level editing (E key) - edit level configuration (price, quantity, risk limits, lifecycle settings)
  - Level creation (C key) - create new levels with symbol selection from existing levels
- ✅ **Real-time Updates**: gRPC streaming for live data updates
- ✅ **gRPC Service**: Complete implementation in `runner` with:
  - Trading control (enable/disable trading)
  - Level management (CRUD operations)
  - Monitoring data aggregation and streaming
  - Direct access to ExchangeStateManager, IndicatorService, TradingStateManager

**Technical Implementation**:
- **Framework**: `ratatui` for TUI rendering
- **gRPC Client**: `tonic` for communication
- **Protocol**: Protocol Buffers (`.proto` files in `libs/trading_core/proto/`)
- **State Management**: Local state in CLI, real-time updates via streaming
- **Error Handling**: Comprehensive error handling with graceful degradation
- **Logging**: File-based logging to `cli_debug.log` (doesn't clutter terminal)

**gRPC Service Methods**:
- `SetTradingEnabled` / `GetTradingStatus` - Trading control
- `GetLevels` / `GetLevel` - Level retrieval
- `CreateLevel` / `UpdateLevel` / `ToggleLevel` - Level management
- `GetMonitoringData` / `StreamMonitoringData` - Real-time monitoring

**Data Sources**:
- All data fetched from existing execution layer (no separate REST calls)
- Balances, orders, trades from `ExchangeStateManager` (WebSocket updates)
- Indicator values from `IndicatorService` (direct calculator access)
- Levels from `LevelManager` (MongoDB persistence)
- Positions removed from TUI (internal calculation, not exchange data)

**Scripts Added**:
- ✅ `scripts/check_dependencies.sh` - Pre-flight check for dependencies and ports
  - Checks MongoDB connection
  - Checks Redis (if enabled)
  - Verifies ports are free (gRPC: 50051, Health: 8080)
  - Auto-starts missing dependencies in new terminal windows
  - Supports systemd, Docker, and direct execution
- ✅ `scripts/kill_grpc_port.sh` - Utility to free gRPC port if occupied

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
  - **Architecture**: Channel created in `main.rs`, sender passed to indicator service, receiver passed to runners via `SharedServices`
  - **Implementation**: Prevents channel closure by ensuring receiver stays alive throughout application lifetime
  - **Architecture**: Channel created in `main.rs`, sender passed to indicator service, receiver passed to runners via `SharedServices`
  - **Implementation**: Prevents channel closure by ensuring receiver stays alive throughout application lifetime

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

### Channel Configuration (✅ Complete)

**Location**: `libs/trading_core/src/config.rs` - `ChannelConfig` struct

**Purpose**: Centralizes buffer sizes for all internal communication channels in an exchange-agnostic way.

**Structure**:
- **Market Data Channel**: Broadcast channel for orderbooks and trades (default: 1000)
- **Exchange Update Channel**: Broadcast channel for orders, balances, positions, trades (default: 1000)
- **Indicator Update Channel**: Broadcast channel for indicator value updates (default: 1000)
- **WebSocket Channels**: Exchange-agnostic WebSocket buffer sizes
  - Inbound channel capacity (default: 1024)
  - Ping channel capacity (default: 16)
  - Outbound RPC channel capacity (default: 1024)
- **LiveConfig Request Channel**: mpsc channel for configuration update requests (default: 100)
- **Shutdown Channel**: Broadcast channel for shutdown signals (default: 1)
- **Market Data Engine Buffer**: Internal buffer for exchange-specific market data clients (default: 1000)

**Key Design Principles**:
- **Exchange-Agnostic**: All channel names are generic (e.g., `websocket_inbound_capacity` not `binance_inbound_capacity`)
- **Centralized**: All buffer sizes defined in one place (`ChannelConfig`)
- **Configurable**: Can be customized via `bot.json` configuration file
- **Default Values**: Sensible defaults for all channels
- **No Circular Dependencies**: `ChannelConfig` lives in `trading_core` (shared library), not in `runner`

**Usage**:
- `runner` loads `ChannelConfig` from `bot.json` and passes it to services
- `exchange_engine::run()` accepts `ChannelConfig` and extracts values for exchange-specific clients
- `BinanceDriver` receives `ChannelConfig` and extracts WebSocket capacities
- `MarketDataDriver` receives `ChannelConfig` and extracts market data buffer size
- Underlying clients (e.g., `BinanceClient`) receive primitive `usize` values (not the config struct)

**Configuration Example**:
```json
{
  "channels": {
    "market_data_capacity": 1000,
    "exchange_update_capacity": 1000,
    "indicator_update_capacity": 1000,
    "websocket_inbound_capacity": 1024,
    "websocket_ping_capacity": 16,
    "websocket_outbound_rpc_capacity": 1024,
    "live_config_request_capacity": 100,
    "shutdown_capacity": 1,
    "market_data_engine_buffer": 1000
  }
}
```

**Architecture**:
- Only `run.rs` (exchange engine) and drivers know about `ChannelConfig`
- Underlying clients remain agnostic and receive primitive values
- Prevents circular dependencies (config in `trading_core`, not `runner`)
- Supports multiple exchanges without exchange-specific naming

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

**Note**: The above `communication.channels` section is legacy. Channel buffer sizes are now configured via the `channels` section in `bot.json` (see Channel Configuration above).

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

## Testing Strategy (✅ Complete)

### Unit Testing
- ✅ Comprehensive unit test coverage across all services
- ✅ Tests for core components: Asset Engine, Market Data Engine, Indicator Engine, Live Config, Trading Engine
- ✅ Mock-based testing for isolated component verification
- ✅ Test coverage for state management, risk management, circuit breakers, and metrics

### Integration Testing
- ✅ Modular integration test suite organized by functionality area
- ✅ Test modules: config, levels, risk, trading_state, circuit_breaker, market_data, indicator, asset, exchange, live_config, level_execution, workflows, risk_metrics, channels
- ✅ Shared test helpers for common setup and data creation
- ✅ External API call tests marked with `#[ignore]` for optional execution
- ✅ Test binary (`test_runner`) for full system testing in no-trade mode
- ✅ Tests verify end-to-end workflows and service interactions

### Test Structure
- **Location**: `tests/integration/` directory with modular organization
- **Helpers**: `tests/integration/helpers.rs` - shared utilities for test setup
- **Coverage**: All major services and workflows have integration tests
- **Execution**: Run with `cargo test --test integration` (or `-- --ignored` for external API tests)

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
3. ✅ State Management Architecture (Proposal 05) - **COMPLETE**
   - ExchangeStateManager with WS vs REST reconciliation
   - MarketDataStateManager with symbol registry
   - TradingStateManager with channel-based subscription
   - SymbolRegistry for reliable symbol↔pair conversions
   - Exchange-level and trading-level reconciliation
   - All state management logic implemented and documented
4. ✅ MongoDB Persistence and Configuration Storage (Proposal 06) - **COMPLETE**
   - MongoDB client wrapper with connection management
   - Document models for all configurations (levels, indicators, strategies, risk limits)
   - Full CRUD persistence modules (LevelPersistence, IndicatorPersistence, StrategyPersistence)
   - Bidirectional conversion between documents and configs
   - Integration into LevelManager, IndicatorService, and Strategy persistence
   - Runner integration with MongoDB client initialization
   - Graceful fallback to file-based config if MongoDB unavailable
5. ✅ LiveConfig Service (Proposal 06b) - **COMPLETE**
   - Channel-based API for runtime configuration updates (no HTTP dependencies)
   - Type definitions for level fields, config paths, and update requests
   - Validation layer for all configuration changes
   - Update logic for levels, risk limits, and bot configuration
   - Integration with existing MongoDB persistence modules
   - Audit logging for all configuration changes
   - Service orchestrator coordinating validation, updates, and persistence
   - Runner integration with background API handler
6. ✅ Risk System (Proposal 07) - **COMPLETE**
   - Comprehensive risk management at trading_engine level (applies to both levels and strategies)
   - Pre-trade, post-trade, and periodic risk checks fully integrated
   - Circuit breaker with Closed/Open/HalfOpen states and automatic recovery
   - Daily P&L tracking with automatic midnight reset
   - Risk metrics: unrealized P&L, realized P&L, daily P&L, exposure, position size
   - Risk limits: position size limits, order count limits, daily loss limits (global and per-pair)
   - Balance checks before every order
   - SymbolRegistry integration for symbol-to-pair conversion
   - All checks integrated into level-based event loop
7. ✅ Asset Manager Engine (Proposal 08) - **COMPLETE**
   - Asset Engine service for fetching public exchange market data via unsigned REST calls
   - Binance implementation with `BinancePublicClient` for exchange info, trading rules, and limits
   - TTL-based caching (5 minutes default) with automatic refresh on stale data
   - Bootstrap integration: Market data fetched at application startup
   - Priority system: Exchange values take priority over config values, with config fallback
   - Order validation: Prices rounded to tick size, quantities rounded to step size
   - Min/max validations: Order size, price, and notional limits from exchange data
   - Integration: Used in both level resolver and strategy executor for order validation
   - Fallback strategy: System works with or without Asset Engine data (graceful degradation)
   - Centralized in `SharedServices` for easy access across all trading components

### Phase 3: User Interface & Quality
1. ✅ **TUI CLI and Monitoring (Proposal 09) - COMPLETE**
   - Full TUI implementation with real-time monitoring
   - Level management (CRUD, toggle, edit, create)
   - gRPC service for external communication
   - Dependency checking and auto-start scripts
2. ✅ **Testing Strategy - COMPLETE**
   - Comprehensive unit test coverage across all services
   - Modular integration test suite with organized test structure
   - Test binary (`test_runner`) for full system testing in no-trade mode
   - Tests organized by service/functionality area (config, levels, risk, trading state, etc.)
   - External API call tests marked with `#[ignore]` for optional execution
   - Helper utilities for test setup and data creation
3. ✅ **Code Optimization - COMPLETE**
   - Performance optimizations completed across all services
   - Reduced unnecessary clones, improved lock contention, optimized memory usage
   - Module splitting for better maintainability (TUI screens, gRPC service)

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

### Runner Refactoring & Exchange-Agnostic Design (✅ Complete)
- **Exchange Config Handling**: Exchange-agnostic initialization functions
  - Removed `extract_binance_config()` from `main.rs` (no longer needed)
  - Created `extract_bot_pair()` helper in `lib.rs` for exchange-agnostic pair extraction
  - All initialization functions (`initialize_market_data_engine()`, `initialize_exchange_engine()`, `initialize_indicator_service()`) now handle exchange-specific config extraction internally
  - Functions take only `BotConfig` instead of exchange-specific configs
  - Scalable design: Adding new exchanges only requires adding branches in initialization functions, not new extraction functions in `main.rs`
- **Indicator Channel Architecture**: Fixed channel closure issue with shared channel
  - Indicator channel created once in `main.rs` before service initialization
  - Channel sender passed to `initialize_indicator_service()` for publishing
  - Channel receiver added to `SharedServices` for use by runners
  - `level_runner()` now uses `services.indicator_rx` instead of creating separate channel
  - Prevents "channel closed" errors by ensuring receiver stays alive
- **Code Organization**: Improved initialization structure
  - All service initialization logic moved to helper functions in `lib.rs`
  - `main.rs` is now cleaner and focuses on orchestration
  - Better separation of concerns: initialization logic separated from main flow
  - Helper functions: `collect_configured_pairs()`, `initialize_mongodb()`, `initialize_asset_service()`, `fetch_asset_data()`, `initialize_market_data_engine()`, `initialize_indicator_service()`, `initialize_exchange_engine()`, etc.

### Configuration Architecture (✅ Complete)
- **Separated Configs**: Trading-specific configs separated from general bot config
  - `config/bot.json`: System-wide settings (exchange, server, redis, features)
  - `config/strategies.json`: Strategy-specific parameters
  - `config/levels.json`: Levels-specific parameters
- **Config Loading**: Logic in `runner` crate (where it belongs)
  - Not in `trading_core` (system-level concern, not library concern)

## Version 0.1 Status

### Completed Features
- ✅ Level-based trading system with full lifecycle management
- ✅ Strategy-based trading system
- ✅ Real-time indicator engine (SMA, EMA, composite indicators)
- ✅ Comprehensive risk management system
- ✅ Asset engine for exchange data fetching
- ✅ TUI CLI with real-time monitoring and level management
- ✅ gRPC service for external communication
- ✅ MongoDB persistence for configurations
- ✅ Live config service for runtime updates
- ✅ Channel-based communication for ultra-low latency
- ✅ Full unit and integration test coverage
- ✅ Code optimizations and performance improvements

### Production Readiness
- **Core Trading**: ✅ Ready for production use
- **Monitoring**: ✅ TUI CLI fully functional
- **Testing**: ✅ Comprehensive test coverage
- **Documentation**: ✅ Architecture documentation complete
- **Configuration**: ✅ Flexible configuration system
- **Security**: ✅ SecretString for API keys, secure defaults

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
- **Well tested**: Comprehensive unit and integration test coverage

