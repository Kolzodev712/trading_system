# Exchange Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Exchange Engine is the **single point of contact** with the exchange for all account-related data. It manages raw exchange data (balances, orders, trades) received via WebSocket and provides this data to other components through broadcast channels.

## Key Principle

**Exchange Engine is the only component that calls the exchange for account data.** All other components rely on `ExchangeStateManager` for account information.

## Architecture

### Structure

```
src/exchange_engine/
├── src/
│   ├── run.rs                    # Exchange-agnostic entry point
│   ├── state/
│   │   ├── state.rs              # ExchangeState - in-memory state
│   │   ├── manager.rs            # ExchangeStateManager - orchestrates state
│   │   ├── publisher.rs          # ExchangeStatePublisher - publishes updates
│   │   └── reconciler.rs         # StateReconciler - WS vs REST reconciliation
│   └── exchanges/
│       └── binance/
│           ├── driver.rs          # BinanceDriver - exchange-specific driver
│           └── ws_client.rs      # BinanceClient - WebSocket client
```

### Components

#### 1. Top-Level Runner (`run.rs`)

**Purpose**: Exchange-agnostic entry point that initializes the exchange engine.

**Responsibilities**:
- Creates channels for communication
- Creates `SymbolRegistry` for symbol↔pair conversions
- Creates `TradingClient` for exchange communication
- Creates `ExchangeStateManager`
- Configures reconciliation (bootstrap + periodic)
- Creates exchange-specific driver and spawns its event loop
- Returns state manager and channels for external use

**Flow**:
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

#### 2. Exchange-Specific Drivers (`exchanges/{exchange}/driver.rs`)

**Purpose**: Handles protocol-specific communication and WebSocket management for each exchange.

**Responsibilities**:
- Implements `ExchangeDriver` trait (defined in `libs/trading_core/src/traits/exchange_driver.rs`)
- Handles protocol-specific communication
- Manages WebSocket connections
- Integrates with `ExchangeStateManager` to update state on WebSocket events
- Each exchange has its own driver (e.g., `BinanceDriver`)

**Trait Definition**:
- `ExchangeDriver` trait provides a common interface for all exchange drivers
- Allows exchange-agnostic initialization in `run.rs`

#### 3. State Management (`state/`)

##### ExchangeState (`state.rs`)

**Purpose**: In-memory state structure holding all exchange data.

**Data Structures**:
- `balances: HashMap<Symbol, Balance>` - Account balances
- `orders: HashMap<OrderId, Order>` - All orders (active and filled)
- `orders_by_pair: HashMap<Pair, Vec<OrderId>>` - Orders indexed by pair
- `trades: Vec<Trade>` - Trade history
- `symbol_registry: SymbolRegistry` - For symbol↔pair conversions

**Key Features**:
- Uses `SymbolRegistry` for reliable `Symbol` ↔ `Pair` conversions
- Efficient lookups via multiple indexes
- Thread-safe access via `Arc<RwLock<ExchangeState>>`

##### ExchangeStateManager (`manager.rs`)

**Purpose**: Orchestrates state updates, reconciliation, and publishing.

**Key Responsibilities**:
- Manages in-memory state: balances, orders, trades from WebSocket
- Uses `SymbolRegistry` for reliable `Symbol` ↔ `Pair` conversions
- Performs WS vs REST reconciliation (exchange-level reconciliation)
- Publishes all updates via broadcast channels
- Processes WebSocket updates:
  - `BalanceUpdate` messages (delta-based balance updates)
  - `ExecutionReport` messages (order updates and trade creation)
  - `OutboundAccountPosition` messages (full balance snapshot)
- MongoDB persistence capability (disabled initially, lazy evaluation)

**Methods**:
- `update_order(order: Order)` - Update order state
- `update_balance(balance: Balance)` - Update balance
- `add_trade(trade: Trade)` - Add trade to history
- `get_state() -> Arc<RwLock<ExchangeState>>` - Get state for reading
- `reconcile()` - Trigger reconciliation

##### ExchangeStatePublisher (`publisher.rs`)

**Purpose**: Publishes state updates via broadcast channels.

**Responsibilities**:
- Publishes all updates via broadcast channels to:
  - Trading Engine (subscribes, updates TradingState)
  - Risk Engine (subscribes, updates risk cache)
  - TUI (via gRPC aggregation)
- Uses broadcast channels for efficient fan-out
- Each subscriber tracks its own lag

##### StateReconciler (`reconciler.rs`)

**Purpose**: Performs WebSocket vs REST reconciliation.

**Reconciliation Process**:

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

**Key Principle**: REST is authoritative - WS state is updated to match REST if discrepancies found.

**Configuration**:
- `reconciliation.exchange_level_interval_ms` - Interval in milliseconds (default: 180000)
- `reconciliation.enabled` - Enable/disable reconciliation (default: true)

## Data Flow

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

## Communication

### Input
- **WebSocket**: Real-time updates from exchange (balances, orders, trades)
- **REST API**: Periodic reconciliation fetches

### Output
- **Broadcast Channels**: State updates published to subscribers
  - Trading Engine subscribes for trading decisions
  - Risk Engine subscribes for risk calculations
  - TUI receives via gRPC aggregation

## WebSocket Message Processing

### BalanceUpdate Messages
- Delta-based balance updates
- Updates `ExchangeState.balances`
- Publishes `ExchangeUpdate::Balance` via channel

### ExecutionReport Messages
- Order status updates (NEW, PARTIALLY_FILLED, FILLED, CANCELED, etc.)
- Updates `ExchangeState.orders`
- Creates trades for filled/partially filled orders
- Publishes `ExchangeUpdate::Order` and `ExchangeUpdate::Trade` via channel

### OutboundAccountPosition Messages
- Full balance snapshot
- Updates all balances in `ExchangeState`
- Publishes `ExchangeUpdate::Balance` for each balance

## Reconciliation

### Exchange-Level Reconciliation

**What**: WebSocket state vs REST state

**Purpose**: Ensure `ExchangeStateManager` matches exchange authoritative state (REST is authoritative)

**Process**:
1. Fetch balances, orders, trades from REST API
2. Compare with current WebSocket state
3. Update `ExchangeState` to match REST if discrepancies found
4. Log all reconciliation activities

**Frequency**:
- Bootstrap: Once on startup (before WebSocket connection)
- Periodic: Configurable interval (default: 180000ms = 3 minutes)

## Persistence

### MongoDB (Optional)

**Purpose**: Configuration persistence (not state persistence)

**Status**: Disabled initially, lazy evaluation

**Note**: State is in-memory only. MongoDB is for configuration backup, not state backup.

## Performance Characteristics

- **Latency**: < 1μs for channel publishing
- **Throughput**: 1000+ updates/second
- **Memory**: Efficient in-memory data structures
- **Reconciliation**: Non-blocking, runs in background

## Dependencies

- `trading_core`: Types, traits, channels, SymbolRegistry
- `tokio`: Async runtime
- Exchange-specific clients (e.g., `BinanceClient`)

## Testing

- Unit tests for state management
- Integration tests for reconciliation
- WebSocket message processing tests
- Channel publishing tests

## Configuration

### Channel Configuration

Channel buffer sizes configured via `ChannelConfig`:
- `exchange_update_capacity`: Broadcast channel capacity (default: 1000)

### Reconciliation Configuration

```json
{
  "reconciliation": {
    "enabled": true,
    "exchange_level_interval_ms": 180000
  }
}
```

## Error Handling

- **WebSocket Disconnections**: Automatic reconnection with exponential backoff
- **REST API Failures**: Logged, reconciliation retries on next interval
- **Channel Lag**: Subscribers track lag independently
- **State Inconsistencies**: Detected and corrected via reconciliation

## Security

- **API Keys**: Use `SecretString` wrapper to prevent accidental exposure
- **TLS/SSL**: All connections use TLS
- **Rate Limiting**: Handled automatically by exchange clients

