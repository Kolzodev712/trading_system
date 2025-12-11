# Risk Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Risk Engine provides comprehensive risk management for all trading operations. It is a **caching-only** service (not a state manager) that can also call the exchange directly for independent verification, acting as a second layer of security.

## Key Principle

Risk Engine acts as an **independent checker** that can verify state correctness by calling the exchange directly, independent of `ExchangeStateManager`.

## Architecture

### Structure

```
src/trading_engine/src/risk/
├── limits.rs              # RiskLimits - risk limit definitions
├── metrics.rs             # RiskMetrics - risk metrics calculation
└── circuit_breaker.rs     # CircuitBreaker - circuit breaker pattern
```

### Components

#### 1. RiskLimits (`limits.rs`)

**Purpose**: Defines risk limits for trading operations.

**Data Structures**:
- `total_max_position: Option<Decimal>` - Maximum total position size
- `max_total_orders: Option<usize>` - Maximum number of active orders
- `daily_loss_limit_all_pairs: Option<Decimal>` - Global daily loss limit
- `daily_loss_limit_per_pair: HashMap<SmolStr, Decimal>` - Per-pair daily loss limits

**Methods**:
- `new()` - Create new RiskLimits
- `validate()` - Validate limits configuration

#### 2. RiskMetrics (`metrics.rs`)

**Purpose**: Calculates and tracks risk metrics.

**Components**:
- **RiskMetricsCalculator**: Calculates risk metrics
  - Unrealized P&L
  - Realized P&L
  - Daily P&L
  - Total exposure
  - Total position size

- **DailyPnLTracker**: Tracks daily P&L
  - Automatic midnight reset
  - Incremental updates
  - Per-pair and global tracking

**Methods**:
- `calculate(trading_state: &TradingState, prices: &HashMap<Pair, Decimal>) -> RiskMetrics` - Calculate metrics
- `update(realized_pnl: Decimal, unrealized_pnl: Decimal)` - Update daily P&L

#### 3. CircuitBreaker (`circuit_breaker.rs`)

**Purpose**: Implements circuit breaker pattern for risk management.

**States**:
- **Closed**: Normal operation, all checks pass
- **Open**: Circuit open, trading blocked
- **HalfOpen**: Testing recovery, limited operations

**Behavior**:
- **Failure Tracking**: Tracks consecutive failures
- **Auto-Recovery**: Automatically transitions to HalfOpen after timeout (default: 5 minutes)
- **Success Reset**: Resets failure count on success

**Methods**:
- `new()` - Create new CircuitBreaker
- `is_allowed() -> bool` - Check if trading is allowed
- `record_failure()` - Record failure
- `record_success()` - Record success
- `open(reason: &str)` - Manually open circuit
- `close()` - Manually close circuit
- `get_state() -> CircuitBreakerState` - Get current state
- `get_open_reason() -> Option<String>` - Get reason if open

#### 4. RiskManager

**Purpose**: Orchestrates all risk checks.

**Key Responsibilities**:
- Caches risk metrics (P&L, exposure, risk limits)
- Receives updates from ExchangeStateManager via channels
- **Can call exchange directly** for independent verification (second layer of security)
- Alarms if discrepancies detected between ExchangeStateManager and direct exchange calls

**Risk Checks**:

1. **Pre-Trade Checks** (before every order):
   - Circuit breaker status
   - Daily loss limits (global and per-pair)
   - Balance checks
   - Position limits
   - Order count limits

2. **Post-Trade Checks** (after order execution):
   - Monitor for limit breaches
   - Update risk metrics

3. **Periodic Checks** (background monitoring):
   - Every second
   - Catches issues even when no trades occur

**Methods**:
- `check_total_position_limits(trading_state: &TradingState) -> RiskCheckResult` - Check position limits
- `check_order_count(trading_state: &TradingState) -> RiskCheckResult` - Check order count
- `check_daily_loss_limits(metrics: &RiskMetrics, pair: &Pair) -> RiskCheckResult` - Check per-pair loss limits
- `check_daily_loss_limits_global(metrics: &RiskMetrics) -> RiskCheckResult` - Check global loss limits
- `check_balance(order_request: &OrderRequest, balances: &[Balance]) -> RiskCheckResult` - Check balance
- `get_limits() -> &RiskLimits` - Get current limits

## Data Flow

```
ExchangeStateManager (via broadcast channel) → Risk Engine
    ├── Update risk cache
    ├── Calculate risk metrics
    ├── Perform risk checks
    └── Independent verification (optional, direct exchange call)
        └── Alarm if discrepancies detected
```

## Communication

### Input
- **ExchangeStateManager** (via broadcast channel): Balances, orders, trades
- **TradingStateManager**: Trading state for risk calculations
- **Direct Exchange Calls** (optional): Independent verification

### Output
- **Risk Check Results**: Pass/fail with reasons
- **Alarms**: If discrepancies detected

## Risk Check Process

### Pre-Trade Check Flow

```
Order Request → Risk Manager
    ├── Check circuit breaker
    ├── Check daily loss limits (global + per-pair)
    ├── Check balance
    ├── Check position limits
    ├── Check order count
    └── Return RiskCheckResult
        ├── allowed: bool
        └── reason: Option<String>
```

### Post-Trade Check Flow

```
Order Execution → Risk Manager
    ├── Update risk metrics
    ├── Check for limit breaches
    └── Alarm if breach detected
```

### Periodic Check Flow

```
Background Task (every 1 second) → Risk Manager
    ├── Calculate current risk metrics
    ├── Check all limits
    └── Alarm if breach detected
```

## Independent Verification

**Purpose**: Second layer of security by verifying state correctness.

**Process**:
1. Risk Engine receives data from ExchangeStateManager via channels
2. Risk Engine can also call exchange directly for verification
3. Compare ExchangeStateManager state with direct exchange calls
4. Alarm if discrepancies detected

**Key Point**: Risk Engine can call exchange directly for verification (independent of `ExchangeStateManager`).

## Performance Characteristics

- **Check Latency**: < 1ms (cached metrics)
- **Cache TTL**: 1 second for risk metrics
- **Throughput**: 1000+ checks/second

## Dependencies

- `trading_core`: Types, channels, TradingClient trait
- `exchange_engine`: ExchangeStateManager (via channels)
- `trading_engine`: TradingState (for risk calculations)

## Testing

- Unit tests for risk limits
- Unit tests for risk metrics
- Unit tests for circuit breaker
- Integration tests for risk checks

## Configuration

### Risk Limits Configuration

```json
{
  "risk": {
    "total_max_position": 10.0,
    "max_total_orders": 100,
    "daily_loss_limit_all_pairs": 1000.0,
    "daily_loss_limit_per_pair": {
      "BTCUSDT": 500.0
    }
  }
}
```

### Circuit Breaker Configuration

```json
{
  "circuit_breaker": {
    "max_failures": 5,
    "timeout_secs": 300
  }
}
```

## Error Handling

- **Check Failures**: Order placement blocked
- **Verification Discrepancies**: Alarm raised
- **Calculation Errors**: Logged, fallback to safe defaults

## Security

- **Multiple Layers**: ExchangeStateManager + direct verification
- **Circuit Breaker**: Prevents catastrophic losses
- **Daily Limits**: Prevents excessive daily losses
- **Position Limits**: Prevents over-exposure

