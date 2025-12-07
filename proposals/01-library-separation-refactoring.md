# Proposal 01: Library Separation Refactoring

## Overview

This proposal outlines the refactoring of the trading system into a modular, monolith architecture with clear separation of concerns. The goal is to create a foundation library (`libs/core`) that provides shared types, traits, and infrastructure (REST/WebSocket clients), while keeping Exchange Engine and Market Data Engine as completely independent, self-contained services that manage their own state and communicate via in-process channels.

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
3. **Unclear Boundaries**: Exchange and market data types are mixed together
4. **State Management**: No clear ownership of state management
5. **Communication**: Uses Redis for everything, not optimized for monolith

## Target Architecture

### Proposed Structure

```
trading_system/
├── src/
│   ├── libs/
│   │   └── core/                          # Shared foundation
│   │       ├── Cargo.toml
│   │       ├── src/
│   │       │   ├── lib.rs
│   │       │   ├── types/                 # ALL data types (organized by domain)
│   │       │   │   ├── mod.rs
│   │       │   │   ├── common.rs          # Pair, Side, OrderType, OrderStatus, etc.
│   │       │   │   ├── exchange.rs        # Order, Balance, Position, Trade
│   │       │   │   └── market_data.rs     # Orderbook, PublicTrade
│   │       │   ├── rest/                  # REST client infrastructure
│   │       │   │   ├── mod.rs
│   │       │   │   ├── client.rs          # Generic REST client
│   │       │   │   ├── authenticated.rs   # Auth support hooks
│   │       │   │   └── unauthenticated.rs # Non-auth support
│   │       │   ├── websocket/             # WebSocket client infrastructure
│   │       │   │   ├── mod.rs
│   │       │   │   ├── client.rs          # Generic WS client
│   │       │   │   ├── authenticated.rs   # Auth streams hooks
│   │       │   │   └── unauthenticated.rs # Public streams hooks
│   │       │   ├── traits/                # Abstractions only
│   │       │   │   ├── mod.rs
│   │       │   │   ├── trading.rs         # TradingClient trait
│   │       │   │   ├── market_data.rs     # MarketDataClient trait
│   │       │   │   └── private_data.rs    # PrivateDataClient trait
│   │       │   ├── channels/              # In-process channel types
│   │       │   │   ├── mod.rs
│   │       │   │   ├── exchange.rs        # Exchange update channels
│   │       │   │   └── market_data.rs     # Market data update channels
│   │       │   ├── grpc/                  # gRPC utilities (NOT proto definitions)
│   │       │   │   ├── mod.rs
│   │       │   │   ├── client.rs          # Generic gRPC client utilities
│   │       │   │   ├── server.rs          # Generic gRPC server utilities
│   │       │   │   ├── connection_pool.rs # Connection pooling
│   │       │   │   └── retry.rs           # Retry logic for gRPC
│   │       │   ├── error.rs               # Common error types
│   │       │   ├── config.rs               # Common config types
│   │       │   └── utils.rs               # Shared utilities
│   │       └── proto/                     # Optional: shared common proto (minimal)
│   │           └── common.proto          # Only truly common types (Pair, Side, etc.)
│   │
│   ├── exchange_engine/                   # Self-contained exchange service
│   │   ├── Cargo.toml
│   │   ├── proto/                         # Own proto definitions
│   │   │   ├── exchange.proto
│   │   │   └── build.rs
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── lib.rs
│   │   │   ├── state/                     # Own state management
│   │   │   │   ├── mod.rs
│   │   │   │   ├── manager.rs             # ExchangeStateManager
│   │   │   │   └── types.rs
│   │   │   ├── exchanges/                 # Exchange implementations
│   │   │   │   ├── mod.rs
│   │   │   │   └── binance/
│   │   │   │       ├── mod.rs
│   │   │   │       ├── client.rs          # Implements TradingClient
│   │   │   │       ├── rest.rs
│   │   │   │       └── websocket.rs
│   │   │   ├── channels/                  # Channel publishers
│   │   │   │   ├── mod.rs
│   │   │   │   └── publisher.rs           # Publishes to channels
│   │   │   └── persistence/              # Own persistence (optional)
│   │   │       └── mongo.rs
│   │   └── proto/                         # Generated proto code
│   │       └── mod.rs
│   │
│   ├── market_data_engine/                # Self-contained market data service
│   │   ├── Cargo.toml
│   │   ├── proto/                         # Own proto definitions
│   │   │   ├── market_data.proto
│   │   │   └── build.rs
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── lib.rs
│   │   │   ├── state/                     # Own state management
│   │   │   │   ├── mod.rs
│   │   │   │   ├── manager.rs             # MarketDataStateManager
│   │   │   │   └── types.rs
│   │   │   ├── exchanges/                 # Market data implementations
│   │   │   │   ├── mod.rs
│   │   │   │   └── binance/
│   │   │   │       ├── mod.rs
│   │   │   │       ├── client.rs          # Implements MarketDataClient
│   │   │   │       ├── rest.rs
│   │   │   │       └── websocket.rs
│   │   │   ├── channels/                  # Channel publishers
│   │   │   │   ├── mod.rs
│   │   │   │   └── publisher.rs           # Publishes to channels
│   │   │   └── persistence/               # Own persistence (optional)
│   │   │       └── mongo.rs
│   │   └── proto/                         # Generated proto code
│   │       └── mod.rs
│   │
│   ├── trading_engine/                    # Core trading system
│   │   ├── Cargo.toml
│   │   ├── proto/                         # Own proto definitions (for TUI CLI)
│   │   │   ├── trading.proto
│   │   │   └── build.rs
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── lib.rs
│   │   │   ├── channels/                  # Channel subscribers
│   │   │   │   ├── mod.rs
│   │   │   │   ├── exchange_subscriber.rs # Subscribes to exchange channels
│   │   │   │   └── market_data_subscriber.rs # Subscribes to market data channels
│   │   │   ├── grpc/                     # gRPC server (for TUI CLI)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── server.rs
│   │   │   │   └── service.rs
│   │   │   └── ... (existing trading logic)
│   │   └── proto/                         # Generated proto code
│   │       └── mod.rs
│   │
│   └── runner/                            # Orchestration (monolith entry point)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           └── lib.rs
```

## Key Principles

1. **Core Library**: Shared foundation with types, traits, REST/WS clients, channels, gRPC utilities
2. **Independent Services**: Exchange Engine and Market Data Engine are completely self-contained
3. **State Ownership**: Each service manages its own state (no shared state management)
4. **In-Process Communication**: Channels for internal monolith communication (ultra-low latency)
5. **External Communication**: gRPC only for external services (TUI CLI)
6. **Per-Service Proto**: Each service defines its own proto (for external APIs)
7. **Data Types by Domain**: Types organized by domain (exchange, market_data, common), not by service

## Communication Architecture

### Monolith Internal Communication (Channels)

**In-Process Channels** (< 1μs latency):
```
Exchange Engine → Channel → Trading Engine
Market Data Engine → Channel → Trading Engine
```

**Benefits**:
- Ultra-low latency (< 1μs vs 0.5-2ms for gRPC)
- No serialization overhead
- Direct memory access
- Simple and efficient

### External Communication (gRPC)

**gRPC for External Services**:
```
Trading Engine → gRPC → TUI CLI (external process)
```

**Benefits**:
- Required for external processes (can't use channels)
- Type-safe with proto definitions
- Efficient binary protocol

## Detailed Component Breakdown

### 1. `libs/core/` - Shared Foundation Library

**Purpose**: Provides shared types, traits, REST/WebSocket clients, channels, and gRPC utilities.

**Contents**:

#### Types (`types/`)
- `common.rs`: `Pair`, `Side`, `OrderType`, `OrderStatus`, `OrderId`, `InstrumentType`, `AccountType`, `DataType`
- `exchange.rs`: `Order`, `OrderRequest`, `Balance`, `AccountInfo`, `Position`, `Trade`
- `market_data.rs`: `Orderbook`, `PublicTrade`

**Organized by domain, not by service**.

#### REST Client (`rest/`)
- `client.rs`: Generic REST client with hooks for authentication
- `authenticated.rs`: Hooks for authenticated requests
- `unauthenticated.rs`: Hooks for non-authenticated requests

**Provides infrastructure, not implementations**.

#### WebSocket Client (`websocket/`)
- `client.rs`: Generic WebSocket client with hooks
- `authenticated.rs`: Hooks for authenticated streams
- `unauthenticated.rs`: Hooks for public streams

**Provides infrastructure, not implementations**.

#### Traits (`traits/`)
- `trading.rs`: `TradingClient` trait (what every exchange engine should implement)
- `market_data.rs`: `MarketDataClient` trait (what every market data engine should implement)
- `private_data.rs`: `PrivateDataClient` trait (for authenticated streams)

**Abstractions only, no implementations**.

#### Channels (`channels/`)
- `exchange.rs`: Exchange update channel types
  ```rust
  pub type ExchangeUpdateSender = mpsc::UnboundedSender<ExchangeUpdate>;
  pub type ExchangeUpdateReceiver = mpsc::UnboundedReceiver<ExchangeUpdate>;
  
  pub struct ExchangeUpdate {
      pub orders: Vec<Order>,
      pub balances: Vec<Balance>,
      pub positions: Vec<Position>,
      pub trades: Vec<Trade>,
  }
  ```
- `market_data.rs`: Market data update channel types
  ```rust
  pub type MarketDataUpdateSender = mpsc::UnboundedSender<MarketDataUpdate>;
  pub type MarketDataUpdateReceiver = mpsc::UnboundedReceiver<MarketDataUpdate>;
  
  pub struct MarketDataUpdate {
      pub orderbooks: Vec<Orderbook>,
      pub trades: Vec<PublicTrade>,
  }
  ```

#### gRPC Utilities (`grpc/`)
- `client.rs`: Generic gRPC client utilities (connection pooling, retry)
- `server.rs`: Generic gRPC server utilities
- `connection_pool.rs`: Connection pooling
- `retry.rs`: Retry logic for gRPC

**Utilities only, NOT proto definitions**.

#### Error Types (`error.rs`)
- Common error types that are integration-agnostic
- Services can extend with service-specific errors
- Service-specific errors can be mapped to common errors

**Dependencies**: 
- `tokio`, `serde`, `thiserror`, `smol_str`, `rust_decimal`, `chrono`
- `tonic` (gRPC framework), `prost` (protobuf)

**Does NOT contain**:
- State management
- Business logic
- Exchange-specific implementations
- Service-specific code
- Proto definitions (services define their own)

### 2. `exchange_engine/` - Self-Contained Exchange Service

**Purpose**: Independent exchange trading service with own state management.

**Contains**:

#### State Management (`state/`)
- `manager.rs`: `ExchangeStateManager` (in-memory state)
- Own state, no shared state
- Manages: orders, balances, positions, trades

#### Exchange Implementations (`exchanges/`)
- `binance/`: Binance exchange implementation
- Implements `TradingClient`, `PrivateDataClient` traits from core
- Uses core REST/WebSocket clients

#### Channel Publishers (`channels/`)
- `publisher.rs`: Publishes state updates to channels
- Sends `ExchangeUpdate` to Trading Engine via channel

#### Proto Definitions (`proto/`)
- `exchange.proto`: Own proto definition (for future external APIs)
- Generated Rust code in `src/proto/`

#### Persistence (`persistence/`)
- `mongo.rs`: MongoDB integration (optional)
- Own persistence logic

**Dependencies**:
- `libs/core` (traits, types, REST/WS clients, channels)
- Own crates (can be modular internally)

**Completely independent** - no dependencies on other services.

### 3. `market_data_engine/` - Self-Contained Market Data Service

**Purpose**: Independent market data service with own state management.

**Contains**:

#### State Management (`state/`)
- `manager.rs`: `MarketDataStateManager` (in-memory state)
- Own state, no shared state
- Manages: orderbooks, public trades

#### Market Data Implementations (`exchanges/`)
- `binance/`: Binance market data implementation
- Implements `MarketDataClient` trait from core
- Uses core REST/WebSocket clients

#### Channel Publishers (`channels/`)
- `publisher.rs`: Publishes market data updates to channels
- Sends `MarketDataUpdate` to Trading Engine via channel

#### Proto Definitions (`proto/`)
- `market_data.proto`: Own proto definition (for future external APIs)
- Generated Rust code in `src/proto/`

#### Persistence (`persistence/`)
- `mongo.rs`: MongoDB integration (optional)
- Own persistence logic

**Dependencies**:
- `libs/core` (traits, types, REST/WS clients, channels)
- Own crates (can be modular internally)

**Completely independent** - no dependencies on other services.

### 4. `trading_engine/` - Core Trading System

**Purpose**: Trading logic, strategy execution, level management, risk system.

**Contains**:

#### Channel Subscribers (`channels/`)
- `exchange_subscriber.rs`: Subscribes to exchange update channels
- `market_data_subscriber.rs`: Subscribes to market data update channels
- Receives updates from Exchange Engine and Market Data Engine

#### gRPC Server (`grpc/`)
- `server.rs`: gRPC server for TUI CLI
- `service.rs`: Implements trading proto service
- Only for external communication (TUI CLI)

#### Trading Logic
- Level management, risk system, strategy execution
- Uses data from channel subscribers

**Dependencies**:
- `libs/core` (traits, types, channels, gRPC utilities)
- Own proto definitions (for TUI CLI)

### 5. `runner/` - Monolith Orchestration

**Purpose**: Wire everything together and run the monolith.

**Responsibilities**:
- Create channel pairs (exchange, market_data)
- Start Exchange Engine with channel sender
- Start Market Data Engine with channel sender
- Start Trading Engine with channel receivers
- Start gRPC server (for TUI CLI)
- Orchestrate shutdown

**Dependencies**:
- `exchange_engine`
- `market_data_engine`
- `trading_engine`
- `libs/core`

## Communication Flow

### Internal (Monolith) - Channels

```
Exchange Engine
    ├── Fetches from Exchange (REST/WebSocket)
    ├── Updates ExchangeStateManager (in-memory)
    └── Publishes to Channel → Trading Engine

Market Data Engine
    ├── Fetches from Exchange (REST/WebSocket)
    ├── Updates MarketDataStateManager (in-memory)
    └── Publishes to Channel → Trading Engine

Trading Engine
    ├── Subscribes to Exchange Channel
    ├── Subscribes to Market Data Channel
    ├── Processes updates
    └── Makes trading decisions
```

### External - gRPC

```
Trading Engine
    ├── gRPC Server (listening)
    └── Serves TUI CLI requests

TUI CLI
    ├── gRPC Client
    └── Connects to Trading Engine gRPC server
```

## Migration Plan

### Phase 1: Create Core Library

1. **Create `libs/core/`**
   - Create `Cargo.toml`
   - Move types from `core_logic/src/data/` → `libs/core/src/types/`
   - Organize by domain (common, exchange, market_data)
   - Create REST/WebSocket client infrastructure
   - Create trait definitions
   - Create channel types
   - Create gRPC utilities (NOT proto definitions)
   - Create error types

### Phase 2: Refactor Exchange Engine

1. **Update `exchange_engine/`**
   - Add state management (own state)
   - Add channel publishers
   - Create own proto definitions
   - Update to use core traits, types, clients
   - Remove dependencies on other services

### Phase 3: Refactor Market Data Engine

1. **Update `market_data_engine/`**
   - Add state management (own state)
   - Add channel publishers
   - Create own proto definitions
   - Update to use core traits, types, clients
   - Remove dependencies on other services

### Phase 4: Update Trading Engine

1. **Update `trading_engine/`**
   - Add channel subscribers
   - Add gRPC server (for TUI CLI)
   - Create own proto definitions
   - Update to use core types, channels
   - Remove dependencies on exchange_engine, market_data_engine

### Phase 5: Update Runner

1. **Update `runner/`**
   - Create channel pairs
   - Wire services together
   - Start all services in monolith
   - Handle shutdown

## Dependency Structure

### `libs/core/Cargo.toml`
```toml
[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
smol_str = { workspace = true }
rust_decimal = { workspace = true }
chrono = { workspace = true }
reqwest = { workspace = true }
tungstenite = { workspace = true }
# gRPC utilities (NOT proto)
tonic = "0.11"
prost = "0.12"
prost-types = "0.12"
```

### `exchange_engine/Cargo.toml`
```toml
[dependencies]
core = { path = "../libs/core" }
# Proto generation
prost = "0.12"
prost-build = "0.12"
tonic = "0.11"
# Exchange-specific
mongodb = { workspace = true } # Optional
```

### `market_data_engine/Cargo.toml`
```toml
[dependencies]
core = { path = "../libs/core" }
# Proto generation
prost = "0.12"
prost-build = "0.12"
tonic = "0.11"
# Market data-specific
mongodb = { workspace = true } # Optional
```

### `trading_engine/Cargo.toml`
```toml
[dependencies]
core = { path = "../libs/core" }
# Proto generation
prost = "0.12"
prost-build = "0.12"
tonic = "0.11"
# Trading-specific
mongodb = { workspace = true } # For config persistence
```

### `runner/Cargo.toml`
```toml
[dependencies]
exchange_engine = { path = "../exchange_engine" }
market_data_engine = { path = "../market_data_engine" }
trading_engine = { path = "../trading_engine" }
core = { path = "../libs/core" }
```

## Benefits

1. **True Independence**: Exchange Engine and Market Data Engine are completely independent
2. **Ultra-Low Latency**: Channels for internal communication (< 1μs)
3. **Clear Separation**: Core = foundation, Services = implementations
4. **Better Organization**: Data types by domain, not by service
5. **State Ownership**: Each service manages its own state
6. **Easy Testing**: Services can be tested independently
7. **Deployment Flexibility**: Can separate later if needed (add gRPC layer)
8. **Maintainability**: Changes are localized to specific services

## Notes

- **Monolith Architecture**: All services run in same process
- **Channels for Internal**: Ultra-low latency in-process communication
- **gRPC for External**: Only for external services (TUI CLI)
- **Per-Service Proto**: Each service defines its own proto (for external APIs)
- **Core Provides Utilities**: gRPC utilities in core, NOT proto definitions
- **State Management**: Each service manages its own state
- **Data Types by Domain**: Organized by domain (exchange, market_data, common)
