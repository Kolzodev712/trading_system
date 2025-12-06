# Proposal 01: Library Separation Refactoring

## Overview

This proposal outlines the refactoring of the trading system into a modular, plug-and-play architecture. The goal is to separate key functionalities into independent libraries that serve single purposes, making the system more maintainable, testable, and extensible.

## Current State Analysis

### Current Structure
```
trading_system/
├── src/
│   ├── core_logic/          # Mixed: abstractions + utilities + data types
│   ├── exchange_engine/     # Binance-specific + some abstractions
│   ├── market_data_engine/  # Binance-specific + some abstractions
│   ├── trading_engine/      # Trading logic (core system)
│   └── runner/              # Orchestration
```

### Current Issues

1. **Tight Coupling**: `core_logic` mixes generic abstractions with exchange-specific code
2. **Monolithic Engines**: `exchange_engine` and `market_data_engine` are tightly coupled to Binance
3. **Hard to Extend**: Adding a new exchange requires changes across multiple modules
4. **Unclear Boundaries**: Shared utilities and exchange-specific code are intermingled
5. **Testing Difficulty**: Hard to test components in isolation

## Target Architecture

### Proposed Structure
```
trading_system/
├── src/
│   ├── libs/
│   │   ├── exchange/                    # Exchange integration library
│   │   │   ├── core/                     # Core exchange abstractions
│   │   │   │   ├── lib.rs
│   │   │   │   ├── traits.rs             # TradingClient, PrivateDataClient
│   │   │   │   ├── types.rs              # Order, OrderRequest, Balance, AccountInfo
│   │   │   │   ├── error.rs              # Exchange-specific errors
│   │   │   │   └── config.rs            # Exchange config abstractions
│   │   │   └── binance/                  # Binance implementation
│   │   │       ├── lib.rs
│   │   │       ├── ws_client.rs
│   │   │       ├── rest_client.rs
│   │   │       ├── websocket/
│   │   │       ├── config.rs
│   │   │       └── authenticator.rs
│   │   │
│   │   ├── market_data/                  # Market data library
│   │   │   ├── core/                     # Core market data abstractions
│   │   │   │   ├── lib.rs
│   │   │   │   ├── traits.rs            # MarketDataClient
│   │   │   │   ├── types.rs             # Orderbook, Trade
│   │   │   │   ├── error.rs
│   │   │   │   └── config.rs
│   │   │   └── binance/                  # Binance implementation
│   │   │       ├── lib.rs
│   │   │       ├── client.rs
│   │   │       ├── websocket/
│   │   │       └── config.rs
│   │   │
│   │   └── shared/                       # Common types for exchange & market_data
│   │       ├── lib.rs
│   │       ├── data_types.rs             # Pair, Side, OrderType, OrderStatus, etc.
│   │       ├── redis.rs                  # Redis messaging utilities
│   │       ├── circuit_breaker.rs
│   │       ├── retry.rs
│   │       ├── error.rs                  # Common error types
│   │       └── utils.rs
│   │
│   ├── trading_engine/                    # Core trading system (NOT a library)
│   │   ├── lib.rs
│   │   ├── strategy.rs                   # Strategy trait
│   │   ├── strategies.rs                 # Strategy implementations
│   │   ├── executor.rs                   # Strategy execution
│   │   ├── state.rs                      # TradingState, Position
│   │   ├── reconciliation.rs
│   │   ├── backtest.rs
│   │   └── strategy_manager.rs
│   │
│   └── runner/                            # Orchestration
│       ├── lib.rs
│       ├── main.rs
│       ├── health.rs
│       └── shutdown.rs
```

### Key Principles

1. **Single Responsibility**: Each library serves one clear purpose
2. **Trait-Based Design**: Core libraries define traits; implementations live in exchange-specific modules
3. **Shared Common Types**: `shared` library contains types used by both `exchange` and `market_data`
4. **Trading Engine is Application Code**: Not a library, but the core trading system that uses the libraries
5. **Plug-and-Play**: New exchanges can be added by implementing traits in new modules

## Detailed Component Breakdown

### 1. `libs/shared/` - Common Types Library

**Purpose**: Shared utilities and data types used by both exchange and market_data libraries.

**Contents**:
- `data_types.rs`: `Pair`, `Side`, `OrderType`, `OrderStatus`, `OrderId`, `InstrumentType`, `AccountType`, `DataType`
- `redis.rs`: `RedisClient`, `RedisMessage`, channel constants (for broadcast scenarios)
- `grpc/`: gRPC communication layer
  - `client.rs`: gRPC client utilities and connection pooling
  - `server.rs`: gRPC server utilities
  - `streaming.rs`: Streaming helpers for unidirectional/bidirectional streams
- `circuit_breaker.rs`: `CircuitBreaker` implementation
- `retry.rs`: Retry logic with exponential backoff
- `error.rs`: Common error types (`EngineError` base)
- `utils.rs`: Serialization utilities, helpers

**Dependencies**: 
- `redis`, `tokio`, `serde`, `thiserror`, `smol_str`, `rust_decimal`, `chrono`
- `tonic` (gRPC framework), `prost` (protobuf)

**Exports**: Public API for common types, utilities, and gRPC helpers

**Communication Strategy**:
- **gRPC**: For high-frequency, direct service-to-service communication (trading→monitoring, market data→trading, indicators→trading)
- **Redis Pub/Sub**: For broadcast scenarios and multi-subscriber patterns (monitoring→web server)

### 2. `libs/exchange/core/` - Exchange Core Abstractions

**Purpose**: Define the exchange integration interface and core types.

**Contents**:
- `traits.rs`: 
  - `TradingClient<OrderRequest>` trait
  - `PrivateDataClient` trait
- `types.rs`: 
  - `Order`, `OrderRequest`, `Balance`, `AccountInfo`
- `error.rs`: Exchange-specific error variants
- `config.rs`: Exchange configuration abstractions

**Dependencies**: 
- `shared` (for common types)
- `async-trait`, `tokio`

**Exports**: Public traits and types that exchange implementations must use

### 3. `libs/exchange/binance/` - Binance Exchange Implementation

**Purpose**: Binance-specific implementation of exchange traits.

**Contents**:
- `ws_client.rs`: `BinanceClient` implementing `TradingClient` and `PrivateDataClient`
- `rest_client.rs`: REST API client for Binance
- `websocket/`: WebSocket message types and handlers
- `config.rs`: Binance-specific configuration
- `authenticator.rs`: Binance authentication logic

**Dependencies**: 
- `exchange/core` (for traits)
- `shared` (for common types)
- Binance-specific dependencies

**Exports**: `BinanceClient`, `BinanceConfig`, and related types

### 4. `libs/market_data/core/` - Market Data Core Abstractions

**Purpose**: Define the market data interface and core types.

**Contents**:
- `traits.rs`: `MarketDataClient` trait
- `types.rs`: `Orderbook`, `Trade`
- `error.rs`: Market data specific errors
- `config.rs`: Market data configuration abstractions

**Dependencies**: 
- `shared` (for common types)
- `async-trait`, `tokio`

**Exports**: Public traits and types for market data implementations

### 5. `libs/market_data/binance/` - Binance Market Data Implementation

**Purpose**: Binance-specific implementation of market data traits.

**Contents**:
- `client.rs`: `BinanceClient` implementing `MarketDataClient`
- `websocket/`: WebSocket message types and handlers
- `config.rs`: Binance-specific market data configuration

**Dependencies**: 
- `market_data/core` (for traits)
- `shared` (for common types, Redis)
- Binance-specific dependencies

**Exports**: `BinanceClient`, `BinanceConfig`, and related types

### 6. `trading_engine/` - Core Trading System

**Purpose**: Trading logic, strategy execution, and order management.

**Contents**: (No changes to structure, only dependency updates)
- `strategy.rs`: Strategy trait (depends on `exchange/core` traits)
- `strategies.rs`: Strategy implementations
- `executor.rs`: Strategy executor
- `state.rs`: Trading state management
- `reconciliation.rs`: Position reconciliation

**Dependencies**: 
- `exchange/core` (for `TradingClient` trait)
- `market_data/core` (for `MarketDataClient` trait)
- `shared` (for common types, Redis, utilities)

**Note**: This is NOT a library - it's the core application logic

### 7. `runner/` - Orchestration

**Purpose**: Wire everything together and run the system.

**Dependencies**: 
- `trading_engine`
- `exchange/binance` (or other exchange implementations)
- `market_data/binance` (or other market data implementations)
- `shared`

## Migration Plan

### Phase 1: Create New Library Structure

1. **Create `libs/shared/`**
   - Create `Cargo.toml` for shared library
   - Move from `core_logic/src/`:
     - `data/base_data.rs` → `libs/shared/src/data_types.rs`
     - `redis.rs` → `libs/shared/src/redis.rs`
     - `circuit_breaker.rs` → `libs/shared/src/circuit_breaker.rs`
     - `retry.rs` → `libs/shared/src/retry.rs`
     - `error.rs` → `libs/shared/src/error.rs` (base error types)
     - `utils.rs` → `libs/shared/src/utils.rs`
   - Update imports and exports

2. **Create `libs/exchange/core/`**
   - Create `Cargo.toml` (depends on `shared`)
   - Move from `core_logic/src/`:
     - `traits.rs` (TradingClient, PrivateDataClient parts) → `libs/exchange/core/src/traits.rs`
     - `data/private_data.rs` → `libs/exchange/core/src/types.rs`
   - Create `libs/exchange/core/src/error.rs` (exchange-specific error variants)
   - Create `libs/exchange/core/src/config.rs` (exchange config abstractions)

3. **Create `libs/exchange/binance/`**
   - Create `Cargo.toml` (depends on `exchange/core`, `shared`)
   - Move from `exchange_engine/src/exchanges/binance/`:
     - All Binance-specific code
   - Update to implement `exchange/core` traits

4. **Create `libs/market_data/core/`**
   - Create `Cargo.toml` (depends on `shared`)
   - Move from `core_logic/src/`:
     - `traits.rs` (MarketDataClient part) → `libs/market_data/core/src/traits.rs`
     - `data/public_data.rs` → `libs/market_data/core/src/types.rs`
   - Create `libs/market_data/core/src/error.rs`
   - Create `libs/market_data/core/src/config.rs`

5. **Create `libs/market_data/binance/`**
   - Create `Cargo.toml` (depends on `market_data/core`, `shared`)
   - Move from `market_data_engine/src/exchanges/binance/`:
     - All Binance-specific code
   - Update to implement `market_data/core` traits

### Phase 2: Update Existing Modules

1. **Update `trading_engine/`**
   - Update `Cargo.toml` dependencies:
     - Remove: `core_logic`, `exchange_engine`, `market_data_engine`
     - Add: `exchange/core`, `market_data/core`, `shared`
   - Update imports throughout:
     - `core_logic::data::...` → `shared::data_types::...` or `exchange::core::types::...`
     - `core_logic::traits::TradingClient` → `exchange::core::traits::TradingClient`
     - `core_logic::redis::...` → `shared::redis::...`
     - `core_logic::circuit_breaker::...` → `shared::circuit_breaker::...`
     - `core_logic::retry::...` → `shared::retry::...`

2. **Update `runner/`**
   - Update `Cargo.toml` dependencies:
     - Remove: `core_logic`, `exchange_engine`, `market_data_engine`
     - Add: `exchange/binance`, `market_data/binance`, `shared`
   - Update imports:
     - `exchange_engine::exchanges::binance::...` → `exchange::binance::...`
     - `market_data_engine::exchanges::binance::...` → `market_data::binance::...`
     - `core_logic::...` → `shared::...` or appropriate library

### Phase 3: Cleanup

1. **Remove Old Modules**
   - Delete `src/core_logic/`
   - Delete `src/exchange_engine/`
   - Delete `src/market_data_engine/`

2. **Update Workspace**
   - Update root `Cargo.toml` workspace members
   - Remove old packages, add new ones

3. **Update Tests**
   - Move integration tests to appropriate locations
   - Update test imports

## File Mapping Reference

### From `core_logic/src/`:

| Current File | New Location |
|-------------|--------------|
| `data/base_data.rs` | `libs/shared/src/data_types.rs` |
| `data/private_data.rs` | `libs/exchange/core/src/types.rs` |
| `data/public_data.rs` | `libs/market_data/core/src/types.rs` |
| `traits.rs` (TradingClient parts) | `libs/exchange/core/src/traits.rs` |
| `traits.rs` (MarketDataClient parts) | `libs/market_data/core/src/traits.rs` |
| `redis.rs` | `libs/shared/src/redis.rs` |
| `circuit_breaker.rs` | `libs/shared/src/circuit_breaker.rs` |
| `retry.rs` | `libs/shared/src/retry.rs` |
| `error.rs` | `libs/shared/src/error.rs` (base) + split to exchange/market_data cores |
| `utils.rs` | `libs/shared/src/utils.rs` |
| `config.rs` | Split: exchange config → `libs/exchange/core/src/config.rs`, market data config → `libs/market_data/core/src/config.rs` |

### From `exchange_engine/src/`:

| Current File | New Location |
|-------------|--------------|
| `exchanges/binance/*` | `libs/exchange/binance/*` |
| `driver.rs` | Remove (functionality moves to runner) |
| `config.rs` | `libs/exchange/binance/src/config.rs` |

### From `market_data_engine/src/`:

| Current File | New Location |
|-------------|--------------|
| `exchanges/binance/*` | `libs/market_data/binance/*` |
| `driver.rs` | Remove (functionality moves to runner) |
| `config.rs` | `libs/market_data/binance/src/config.rs` |
| `messages.rs` | `libs/market_data/binance/src/messages.rs` (or keep in core if generic) |

## Dependency Updates

### `libs/shared/Cargo.toml`
```toml
[dependencies]
redis = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
smol_str = { workspace = true }
rust_decimal = { workspace = true }
chrono = { workspace = true }
# gRPC dependencies
tonic = "0.11"
prost = "0.12"
prost-types = "0.12"
```

### `libs/exchange/core/Cargo.toml`
```toml
[dependencies]
shared = { path = "../shared" }
async-trait = { workspace = true }
tokio = { workspace = true }
```

### `libs/exchange/binance/Cargo.toml`
```toml
[dependencies]
exchange-core = { path = "../core" }
shared = { path = "../../shared" }
# ... Binance-specific dependencies
```

### `libs/market_data/core/Cargo.toml`
```toml
[dependencies]
shared = { path = "../shared" }
async-trait = { workspace = true }
tokio = { workspace = true }
```

### `libs/market_data/binance/Cargo.toml`
```toml
[dependencies]
market-data-core = { path = "../core" }
shared = { path = "../../shared" }
# ... Binance-specific dependencies
```

### `trading_engine/Cargo.toml`
```toml
[dependencies]
exchange-core = { path = "../libs/exchange/core" }
market-data-core = { path = "../libs/market_data/core" }
shared = { path = "../libs/shared" }
# Remove: core_logic, exchange_engine, market_data_engine
```

### `runner/Cargo.toml`
```toml
[dependencies]
trading_engine = { path = "../trading_engine" }
exchange-binance = { path = "../libs/exchange/binance" }
market-data-binance = { path = "../libs/market_data/binance" }
shared = { path = "../libs/shared" }
# Remove: core_logic, exchange_engine, market_data_engine
```

## Import Path Changes

### Before:
```rust
use core_logic::data::base_data::Pair;
use core_logic::traits::TradingClient;
use core_logic::redis::RedisClient;
use exchange_engine::exchanges::binance::ws_client::BinanceClient;
```

### After:
```rust
use shared::data_types::Pair;
use exchange::core::traits::TradingClient;
use shared::redis::RedisClient;
use exchange::binance::BinanceClient;
```

## Testing Strategy

1. **Unit Tests**: Each library should have its own test suite
   - `libs/shared/tests/` - Test utilities and common types
   - `libs/exchange/core/tests/` - Test trait contracts
   - `libs/exchange/binance/tests/` - Test Binance implementation
   - `libs/market_data/core/tests/` - Test market data traits
   - `libs/market_data/binance/tests/` - Test Binance market data

2. **Integration Tests**: Update existing integration tests
   - Update imports to use new library paths
   - Ensure all tests still pass

3. **Mock Implementations**: Create mock implementations for testing
   - `libs/exchange/core/tests/mocks.rs` - Mock TradingClient
   - `libs/market_data/core/tests/mocks.rs` - Mock MarketDataClient

## Rollback Plan

1. Keep old modules in a `_old/` directory during migration
2. Use feature flags to switch between old and new implementations
3. Maintain git branches for each phase
4. Test thoroughly before removing old code

## Benefits After Migration

1. **Clear Separation**: Each library has a single, well-defined purpose
2. **Easy Extension**: Add new exchanges by creating new implementation modules
3. **Better Testing**: Test libraries in isolation with mock implementations
4. **Reduced Coupling**: Trading engine depends on traits, not implementations
5. **Maintainability**: Changes are localized to specific libraries
6. **Reusability**: Libraries can be used independently in other projects
7. **Efficient Communication**: gRPC for high-frequency updates, Redis for broadcasts
8. **Performance Optimized**: Binary protocol (protobuf) reduces serialization overhead

## Potential Challenges

1. **Circular Dependencies**: Ensure `shared` doesn't depend on `exchange` or `market_data`
2. **Breaking Changes**: Some internal APIs may need adjustment
3. **Test Updates**: All tests need import path updates
4. **Documentation**: Update all documentation to reflect new structure

## Success Criteria

- [ ] All libraries compile independently
- [ ] All existing tests pass
- [ ] No circular dependencies
- [ ] Trading engine compiles with new dependencies
- [ ] Runner successfully orchestrates all components
- [ ] System runs end-to-end without errors
- [ ] Code is more maintainable and extensible

## Next Steps

1. Review and approve this proposal
2. Create feature branch for migration
3. Execute Phase 1 (create new structure)
4. Execute Phase 2 (update existing modules)
5. Execute Phase 3 (cleanup)
6. Update documentation
7. Merge to main

## Communication Architecture

### gRPC for High-Frequency Communication

**Use gRPC for**:
- Trading System → Monitoring Service (high-frequency updates)
- Market Data → Trading System (orderbook updates)
- Indicator Service → Trading System (indicator updates)

**Benefits**:
- Binary protocol (protobuf) - 3-10x smaller than JSON
- Lower latency with HTTP/2 multiplexing
- Built-in backpressure and flow control
- Type-safe with compile-time checks
- Streaming support for real-time updates

### Redis Pub/Sub for Broadcast Scenarios

**Use Redis for**:
- Monitoring Service → Web Server (multiple subscribers possible)
- Broadcast notifications
- Occasional full snapshots

**Benefits**:
- Decoupled services (no direct connections)
- Multiple subscribers
- Simple deployment
- Fire-and-forget pattern

### Implementation

The `shared` library provides:
- `grpc/client.rs`: Connection pooling, client utilities
- `grpc/server.rs`: Server setup, streaming helpers
- `grpc/streaming.rs`: Unidirectional/bidirectional stream helpers

## Performance Optimizations

1. **Connection Pooling**: Reuse gRPC connections
2. **Batching**: Batch updates where possible (configurable)
3. **Caching**: Cache frequently accessed data
4. **Async Processing**: Non-blocking operations
5. **Lazy Evaluation**: Calculate only when needed

## Notes

- Keep workspace structure as-is (no major reorganization)
- `trading_engine` remains application code, not a library
- `shared` contains only types/utilities used by both exchange and market_data
- All exchange-specific code lives in `exchange/binance/`
- All market data-specific code lives in `market_data/binance/`
- gRPC for direct service-to-service, Redis for broadcasts

