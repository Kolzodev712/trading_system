# Trading Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Trading Engine is the core component that executes trading strategies and level-based trading. It manages trading-specific state (positions, active orders) derived from `ExchangeStateManager` and makes trading decisions based on market data and indicators.

## Key Principle

**Trading Engine does NOT call the exchange directly.** It relies entirely on `ExchangeStateManager` for account data and subscribes to market data and indicators via channels.

## Architecture

### Structure

```
src/trading_engine/
├── src/
│   ├── state/
│   │   ├── state.rs              # TradingState, Position, Strategy trait
│   │   ├── manager.rs             # TradingStateManager - orchestrates state
│   │   ├── subscriber.rs         # ExchangeStateSubscriber - subscribes to ExchangeStateManager
│   │   └── reconciler.rs          # PositionReconciler - reconciles with ExchangeStateManager
│   ├── strategies/
│   │   ├── strategies.rs         # BuyLowSellHigh implementation
│   │   ├── executor.rs            # Strategy execution loop
│   │   ├── strategy_manager.rs    # Multi-strategy management
│   │   └── backtest.rs            # Backtesting framework
│   ├── levels/
│   │   ├── manager.rs             # Level lifecycle management
│   │   ├── executor.rs            # Level execution
│   │   ├── event_loop.rs          # Main event loop
│   │   ├── resolver.rs            # Indicator resolution
│   │   ├── indicator_subscriber.rs # Indicator value subscription
│   │   ├── risk/                  # Risk management submodule
│   │   └── cancellation/          # Cancellation logic submodule
│   └── risk/
│       ├── limits.rs              # Risk limits definitions
│       ├── metrics.rs             # Risk metrics calculation
│       └── circuit_breaker.rs     # Circuit breaker pattern
```

### Components

#### 1. State Management (`state/`)

##### TradingState (`state.rs`)

**Purpose**: In-memory state structure for trading-specific data.

**Data Structures**:
- `positions: HashMap<Pair, Position>` - Positions tracking (derived from ExchangeStateManager balances)
- `active_orders: HashMap<OrderId, Order>` - Active orders (only orders we placed)
- `pending_orders: HashMap<OrderId, u64>` - Pending orders with timestamps

**Key Features**:
- Shared by both strategies and levels modes
- Derived from ExchangeStateManager via channel subscriptions
- Does NOT call exchange directly - relies on ExchangeStateManager

**Methods**:
- `add_order(order: Order)` - Add order to active orders
- `update_order_status(order_id: &OrderId, status: OrderStatus, filled_amount: Option<Decimal>)` - Update order status
- `remove_order(order_id: &OrderId)` - Remove order
- `update_position(pair: Pair, quantity: Decimal, price: Decimal)` - Update position
- `get_position(pair: &Pair) -> Option<&Position>` - Get position for pair
- `get_stale_orders(timeout_secs: u64) -> Vec<OrderId>` - Get stale orders

##### TradingStateManager (`manager.rs`)

**Purpose**: Orchestrates state updates, subscription, and reconciliation.

**Key Responsibilities**:
- Manages `TradingState` (in-memory state)
- Runs `ExchangeStateSubscriber` to receive updates from `ExchangeStateManager`
- Provides `reconcile()` method for periodic reconciliation
- Provides methods: `add_order()`, `update_order()`, `remove_order()`

**Methods**:
- `new()` - Create new TradingStateManager
- `get_state() -> Arc<RwLock<TradingState>>` - Get state for reading
- `reconcile(exchange_state: &ExchangeState)` - Reconcile with ExchangeStateManager
- `add_order(order: Order)` - Add order
- `update_order(order: Order)` - Update order
- `remove_order(order_id: &OrderId)` - Remove order

##### ExchangeStateSubscriber (`subscriber.rs`)

**Purpose**: Subscribes to `ExchangeStateManager` updates via broadcast channels.

**Responsibilities**:
- Subscribes to `ExchangeStateManager` updates via broadcast channels
- Handles `ExchangeUpdate::Order`, `ExchangeUpdate::Balance`, `ExchangeUpdate::Trade`
- Updates `TradingState` based on received updates
- Only tracks orders we placed (filters out external orders)
- Handles channel lag (`RecvError::Lagged`) with warnings

**Data Flow**:
```
ExchangeStateManager (via broadcast channel) → Trading Engine
    ├── ExchangeStateSubscriber receives updates
    │   ├── Order updates → Update TradingState.active_orders
    │   ├── Balance updates → Derive positions
    │   └── Trade updates → Update positions and order fills
    └── TradingStateManager orchestrates updates
```

##### PositionReconciler (`reconciler.rs`)

**Purpose**: Reconciles `TradingState` with `ExchangeStateManager` (NOT with exchange directly).

**Reconciliation Process**:
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

#### 2. Trading Modes

##### Level-Based Trading (`levels/`)

**Purpose**: Execute trading based on predefined levels with indicator-based price/quantity resolution.

**Components**:
- **LevelManager** (`manager.rs`): Level lifecycle management
  - CRUD operations for levels
  - Level state transitions (Created, Posted, Filled, Cancelled, Paused, Removed)
  - Enable/disable levels
  - MongoDB persistence

- **LevelExecutor** (`executor.rs`): Level execution
  - Resolves price and quantity using indicators
  - Places orders via TradingClient
  - Handles order fills and state transitions

- **EventLoop** (`event_loop.rs`): Main event loop
  - Processes orderbook updates
  - Executes levels ready to post
  - Handles order fills and cancellations

- **IndicatorResolver** (`resolver.rs`): Indicator resolution
  - Resolves price from PriceConfig (Fixed, Indicator, Orderbook)
  - Resolves quantity from QuantityConfig (Fixed, Indicator, Balance)
  - Uses AssetService for order validation

- **IndicatorSubscriber** (`indicator_subscriber.rs`): Indicator value subscription
  - Subscribes to indicator updates via channels
  - Caches indicator values for fast lookup

- **Risk Manager** (`risk/manager.rs`): Risk checks per level
  - Pre-trade risk checks
  - Cached risk metrics
  - Incremental updates

- **Cancellation Manager** (`cancellation/manager.rs`): Cancellation logic
  - Handles level cancellations
  - Cancels associated orders

**Level Lifecycle**:
1. **Created**: Level created, not yet posted
2. **Posted**: Order placed, waiting for fill
3. **Filled**: Order filled, level complete
4. **Cancelled**: Level cancelled, orders cancelled
5. **Paused**: Level paused, no new orders
6. **Removed**: Level removed from system

##### Strategy-Based Trading (`strategies/`)

**Purpose**: Execute trading based on algorithmic strategies.

**Components**:
- **Strategy Trait** (`state.rs`): Common interface for strategies
  - `execute()` - Execute strategy logic
  - `get_state()` - Get strategy state

- **BuyLowSellHigh** (`rust_strategies/low_high.rs`): Mean reversion strategy
  - Buy when price drops below average, sell when price rises above average
  - Volatility-adaptive thresholds
  - EMA smoothing and RSI confirmation
  - Trend filtering and liquidity analysis
  - Dynamic position sizing and smart order selection
  - Performance tracking and adaptation

- **StrategyExecutor** (`executor.rs`): Strategy execution loop
  - Subscribes to orderbook updates via channels
  - Executes strategies based on market conditions
  - Uses TradingStateManager for state interactions
  - Respects trading control flag (enable/disable trading)
  - Performs risk checks before order placement

- **StrategyManager** (`strategy_manager.rs`): Multi-strategy management
  - Manages multiple strategies
  - Coordinates strategy execution

- **Backtest** (`backtest.rs`): Backtesting framework
  - Run strategies on historical data
  - Evaluate strategy performance

#### 3. Risk Management (`risk/`)

**Purpose**: Comprehensive risk management for all trading operations.

**Components**:
- **RiskLimits** (`limits.rs`): Risk limit definitions
  - Position size limits
  - Order count limits
  - Daily loss limits (global and per-pair)

- **RiskMetrics** (`metrics.rs`): Risk metrics calculation
  - Unrealized P&L
  - Realized P&L
  - Daily P&L
  - Exposure
  - Position size

- **CircuitBreaker** (`circuit_breaker.rs`): Circuit breaker pattern
  - Three-state system (Closed/Open/HalfOpen)
  - Automatic recovery after timeout
  - Failure tracking

**Risk Checks**:
- **Pre-Trade**: Before every order placement
  - Circuit breaker
  - Daily loss limits
  - Balance checks
  - Position limits
  - Order count limits

- **Post-Trade**: After order execution
  - Monitor for limit breaches
  - Update risk metrics

- **Periodic**: Background monitoring
  - Every second
  - Catches issues even when no trades occur

## Data Flow

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

### Strategy Execution Flow

```
Orderbook Update (Channel) → Strategy Executor
    ├── Evaluate market conditions
    ├── Execute strategy logic
    ├── Risk check
    ├── Place order (if allowed)
    └── Update state
```

## Communication

### Input
- **ExchangeStateManager** (via broadcast channel): Balances, orders, trades
- **MarketDataStateManager** (via broadcast channel): Orderbooks, public trades
- **Indicator Engine** (via broadcast channel): Indicator values

### Output
- **TradingClient**: Order placement and cancellation
- **gRPC Server**: Monitoring data aggregation

## Module Independence

**Key Design**: `strategies/` and `levels/` modules are **independent** and do NOT depend on each other.

**Common Logic**: Shared components extracted to `state/` module:
- `TradingState` - shared state
- `TradingStateManager` - shared state management
- `ExchangeStateSubscriber` - shared subscription
- `PositionReconciler` - shared reconciliation

## Performance Characteristics

- **Latency**: < 1ms for risk checks (cached)
- **Throughput**: 100+ levels/second execution
- **Memory**: Efficient in-memory data structures
- **Indicator Lookup**: < 0.1ms (cached)

## Dependencies

- `trading_core`: Types, channels, TradingClient trait
- `exchange_engine`: ExchangeStateManager (via channels)
- `market_data_engine`: MarketDataStateManager (via channels)
- `indicator_engine`: IndicatorService (via channels)
- `asset_engine`: AssetService (for order validation)

## Testing

- Unit tests for state management
- Unit tests for level execution
- Unit tests for strategy execution
- Unit tests for risk management
- Integration tests for end-to-end workflows

## Configuration

### Level Configuration

```json
{
  "levels": [
    {
      "id": "level_1",
      "symbol": {"base": "BTC", "quote": "USDT"},
      "side": "Buy",
      "order_type": "Limit",
      "price": {"type": "Fixed", "value": 50000},
      "quantity": {"type": "Fixed", "value": 0.001},
      "risk": {...},
      "lifecycle": {...}
    }
  ]
}
```

### Strategy Configuration

```json
{
  "strategy_type": "BuyLowSellHigh",
  "parameters": {...}
}
```

## Error Handling

- **Channel Lag**: Handled gracefully with warnings
- **Risk Check Failures**: Order placement blocked
- **Order Placement Failures**: Retry with exponential backoff
- **State Inconsistencies**: Detected and corrected via reconciliation

## Security

- **Risk Limits**: Enforced before every order
- **Circuit Breaker**: Prevents catastrophic losses
- **State Validation**: All state updates validated

