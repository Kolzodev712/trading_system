# Exchange Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024  
**Status:** In-process CEX adapter (Binance, Bybit, k-paper). Current system status including Demo/Paper scope: [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md).

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
│   ├── config.rs                  # Exchange configuration structures
│   ├── state/
│   │   ├── state.rs              # ExchangeState - in-memory state
│   │   ├── manager.rs            # ExchangeStateManager - orchestrates state
│   │   ├── publisher.rs          # ExchangeStatePublisher - publishes updates
│   │   └── reconciler.rs         # StateReconciler - WS vs REST reconciliation
│   └── exchanges/
│       ├── binance/
│       │   ├── client.rs          # BinanceClient - main client
│       │   ├── driver.rs          # BinanceDriver - exchange-specific driver
│       │   ├── authenticator.rs   # BinanceAuthenticator - ED25519 signing
│       │   ├── config.rs          # BinanceConfig
│       │   ├── common_types.rs    # Binance common types
│       │   ├── error.rs           # Binance error types
│       │   ├── rest/
│       │   │   └── client.rs      # BinanceHttpClient - REST client
│       │   └── websocket/
│       │       ├── client.rs      # BinanceWebSocketClient
│       │       ├── rpc_client.rs   # BinanceRpcClient - RPC client
│       │       ├── streaming_client.rs # BinanceStreamingClient
│       │       ├── mapper.rs       # Message mapping
│       │       └── subscriber.rs  # Subscription handling
│       └── bybit/
│           ├── client.rs          # BybitClient - main client
│           ├── driver.rs          # BybitDriver - exchange-specific driver
│           ├── authenticator.rs   # BybitAuthenticator - HMAC signing
│           ├── config.rs          # BybitConfig
│           ├── common_types.rs    # Bybit common types
│           ├── error.rs           # Bybit error types
│           ├── rest/
│           │   └── client.rs      # BybitHttpClient - REST client
│           └── websocket/
│               ├── rpc_client.rs   # BybitRpcClient - RPC client
│               ├── streaming_client.rs # BybitStreamingClient
│               ├── mapper.rs       # Message mapping
│               └── subscriber.rs  # Subscription handling
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

#### 3. Rate Limiting (`exchanges/{exchange}/rate_limit.rs`)

**Purpose**: Enforces exchange API rate limits to prevent IP bans and ensure compliance.

**Components**:
- **Generic Rate Limiters** (`trading_core::rate_limiter`):
  - `FixedWindowRateLimiter`: Fixed window algorithm (used for Binance)
  - `TokenBucketRateLimiter`: Token bucket algorithm (for future use)
  
- **Exchange-Specific Rate Limiter** (`exchanges/binance/rate_limit.rs`):
  - `BinanceRateLimiter`: Manages multiple rate limit types and intervals
  - Handles REQUEST_WEIGHT and ORDERS limits
  - Supports multiple intervals (SECOND, MINUTE, HOUR, DAY)
  - Updates limits dynamically from API responses

**Integration**:
- **REST Client**: Checks REQUEST_WEIGHT limits before each REST API call
- **WebSocket RPC Client**: Checks REQUEST_WEIGHT or ORDERS limits before each RPC request
- **429 Handling**: Extracts `retryAfter` from 429 responses and waits appropriately
- **Dynamic Updates**: Updates rate limits from `rateLimits` field in API responses

#### 4. State Management (`state/`)

##### ExchangeState (`state.rs`)

**Purpose**: In-memory state structure holding all exchange data.

**Data Structures**:
- `balances: HashMap<Symbol, Balance>` - Account balances
- `orders: HashMap<OrderId, Order>` - All orders (active and filled)
- `orders_by_pair: HashMap<Pair, Vec<OrderId>>` - Orders indexed by pair
- `positions: HashMap<Pair, Position>` - Exchange-level positions (for futures)
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
- `update_position(position: Position)` - Update position (for futures)
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
- Publishes order, balance, position, and trade updates
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

### Position Messages (Futures)
- Position updates from exchange WebSocket (for futures trading)
- Updates `ExchangeState.positions`
- Publishes `ExchangeUpdate::Position` via channel
- Includes leverage, margin, unrealized P&L, liquidation price

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

### Exchange Configuration Structure

Exchange configuration follows a unified structure:

```json
{
  "exchange_engine": {
    "exchange": "Binance",
    "config": {
      "pairs": ["ETH/USDC", "BTC/USDT"],
      "apiKey": "...",
      "secret": "..."
    }
  }
}
```

For Bybit, includes instrument type:

```json
{
  "exchange_engine": {
    "exchange": "Bybit",
    "config": {
      "pairs": ["ETH/USDC", "BTC/USDT"],
      "instrumentType": "Futures",
      "credentials": {
        "apiKey": "...",
        "apiSecret": "..."
      }
    }
  }
}
```

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

## Trading Operations API

### TradingClient Trait

The `TradingClient` trait provides exchange-agnostic trading operations:

**Core Methods**:
- `open_order(req: OrderRequest) -> Result<Order>` - Place a new order
- `cancel_order_by_id(order_id: OrderId) -> Result<()>` - Cancel an order by ID
- `cancel_all_orders(pair: &Pair) -> Result<()>` - Cancel all orders for a pair
- `amend_order(order_id: OrderId, new_request: OrderRequest) -> Result<Order>` - Amend an existing order

### Order Amendment

**Purpose**: Modify existing orders (price/quantity) without canceling and placing new orders.

**Implementation**:
- **Binance**: Uses native `order.modify` WebSocket API
- **Automatic Fallback**: Falls back to cancel+place if:
  - Order is filled or cancelled (cannot be amended)
  - Side changed (must cancel and place new)
  - Amendment API call fails

**Amendment Logic**:
1. **Check Order State**: Verifies order is `New` or `PartiallyFilled` (can be amended)
2. **Check Changes**: Only amends if price or quantity changed (side unchanged)
3. **Native Amend**: Uses exchange's native amend API (e.g., Binance `order.modify`)
4. **Fallback**: If amend fails or not applicable, cancels and places new order

**Benefits**:
- **Reduced Latency**: Amendments are faster than cancel+place
- **Better Execution**: Maintains order priority and reduces market impact
- **Automatic**: Trading engine automatically decides when to use amend vs cancel+place

**Usage in Trading Engine**:
- **Levels**: Automatically amends orders when resolved price/quantity changes
- **Strategies**: Automatically amends orders when strategy requests order with changed price/quantity

## Rate Limiting

### Overview

The Exchange Engine implements comprehensive rate limiting to comply with exchange API limits and prevent IP bans. Rate limiting is implemented at both the generic level (in `trading_core`) and exchange-specific level (in `exchange_engine`).

### Generic Rate Limiter (`trading_core::rate_limiter`)

The generic rate limiter module provides reusable rate limiting algorithms:

#### Fixed Window Rate Limiter

- **Algorithm**: Divides time into fixed windows (e.g., 1 minute, 10 seconds)
- **Behavior**: All requests within a window count towards the limit, count resets at window start
- **Use Case**: Binance's rate limits (which reset at fixed intervals)

#### Token Bucket Rate Limiter

- **Algorithm**: Maintains a bucket of tokens that refill at a constant rate
- **Behavior**: Allows burst traffic up to bucket capacity, then steady-state at refill rate
- **Use Case**: More flexible rate limiting with burst capacity

### Binance Rate Limiter (`exchanges/binance/rate_limit.rs`)

The Binance-specific rate limiter manages multiple rate limit types and intervals:

#### Rate Limit Types

- **REQUEST_WEIGHT**: Weight-based limits (IP-based, shared across connections)
  - Default: 6000 per minute
  - Used for: All API requests (REST and WebSocket)
  
- **ORDERS**: Order count limits (account-based, shared across API keys)
  - Default: 50 per 10 seconds, 160000 per day
  - Used for: Order placement operations

#### Rate Limit Intervals

Binance supports multiple intervals per rate limit type:
- **SECOND**: Per-second limits (e.g., 50 orders per 10 seconds)
- **MINUTE**: Per-minute limits (e.g., 6000 weight per minute)
- **HOUR**: Per-hour limits
- **DAY**: Per-day limits (e.g., 160000 orders per day)

#### Implementation Details

**Initialization**:
- Rate limiters are initialized with default Binance limits on client creation
- Limits are updated dynamically from API responses (`rateLimits` field)

**Rate Limit Checking**:
- Before each request, the rate limiter checks if the request is allowed
- If not allowed, the client waits for the required duration (`retryAfter`)
- The most restrictive limit (across all intervals) is enforced

**Rate Limit Updates**:
- Rate limits are extracted from API responses (`rateLimits` field in WebSocket responses)
- Limits are updated dynamically as responses are received
- REST API responses include rate limit headers (`x-mbx-used-weight-1m`)

**429 Error Handling**:
- When a 429 (Too Many Requests) error is received:
  - The `retryAfter` field is extracted from the error response
  - The client waits for the specified duration before retrying
  - Error is logged with retry information

### Integration

#### REST Client (`rest/client.rs`)

- **Rate Limiting**: Checks `REQUEST_WEIGHT` limits before each REST API call
- **Request Weight**: Different endpoints have different weights (e.g., `exchangeInfo` = 10, most endpoints = 1)
- **429 Handling**: Extracts `retryAfter` from 429 responses and logs appropriately
- **Header Monitoring**: Tracks `x-mbx-used-weight-1m` header for usage monitoring

#### WebSocket RPC Client (`rpc_client.rs`)

- **Rate Limiting**: Checks appropriate limits (`REQUEST_WEIGHT` or `ORDERS`) before each RPC request
- **Request Weight**: Most RPC requests cost 1 weight (connection costs 2)
- **Rate Limit Type**: Order operations use `ORDERS` limit, others use `REQUEST_WEIGHT`
- **Response Updates**: Extracts `rateLimits` from responses and updates limiters dynamically
- **429 Handling**: Handles 429 errors with `retryAfter` extraction

### Rate Limit Flow

```
Request → Rate Limiter Check
    ├── Allowed? → Send Request → Record Usage → Update Limits from Response
    └── Blocked? → Wait (retryAfter) → Retry Check
```

### Configuration

Rate limiters are initialized with default Binance limits:
- **REQUEST_WEIGHT**: 6000 per minute
- **ORDERS**: 50 per 10 seconds, 160000 per day

These defaults are updated automatically from API responses.

### Best Practices

1. **Monitor Rate Limits**: Check logs for rate limit usage and 429 errors
2. **Respect Limits**: The system automatically enforces limits, but monitor for issues
3. **Connection Limits**: Binance allows 300 connections per 5 minutes per IP
4. **Weight Management**: Heavier requests (e.g., `exchangeInfo`) consume more weight

## Security

- **API Keys**: Use `SecretString` wrapper to prevent accidental exposure
- **TLS/SSL**: All connections use TLS
- **Rate Limiting**: Comprehensive rate limiting prevents IP bans and ensures API compliance

