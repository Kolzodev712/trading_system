# Trading System

A modular, exchange-agnostic trading system built in Rust with support for real-time market data, order execution, and strategy execution.

## Features

- **Exchange Integration**: Currently supports Binance (spot trading)
- **Real-time Market Data**: WebSocket-based orderbook streaming via Redis
- **Strategy Execution**: BuyLowSellHigh strategy with stop-loss and take-profit
- **Order Management**: Full order lifecycle tracking with status management
- **Position Reconciliation**: Automatic position sync with exchange
- **Circuit Breaker**: Error handling with automatic recovery
- **Health Monitoring**: REST API for system status and positions
- **Graceful Shutdown**: Clean shutdown with configurable timeout

## Architecture

The system is organized into several modules:

- **`core_logic`**: Core abstractions, data types, and traits
- **`exchange_engine`**: Exchange-specific implementations (Binance)
- **`market_data_engine`**: Market data collection and streaming
- **`trading_engine`**: Strategy execution and order management
- **`runner`**: Main application orchestrator

### Data Flow

1. **Market Data Engine** → Connects to Binance WebSocket → Receives orderbook updates → Publishes to Redis
2. **Strategy Executor** → Subscribes to Redis → Receives orderbook updates → Calls strategy logic
3. **Strategy** → Analyzes orderbook → Generates order requests → Returns to executor
4. **Strategy Executor** → Places orders via Trading Client → Tracks order status
5. **Exchange Engine** → Executes orders on Binance → Sends execution reports → Updates strategy state
6. **Health Server** → Exposes REST API → Provides real-time system status

### Component Interaction

```
┌─────────────────┐
│  Binance API    │
└────────┬────────┘
         │
         ├─────────────────┐
         │                 │
┌────────▼────────┐  ┌─────▼──────────┐
│ Market Data     │  │ Exchange       │
│ Engine          │  │ Engine         │
│ (WebSocket)     │  │ (WebSocket)    │
└────────┬────────┘  └─────┬──────────┘
         │                 │
         │                 │
┌────────▼─────────────────▼────────┐
│         Redis (Message Bus)        │
└────────┬──────────────────────────┘
         │
┌────────▼────────┐
│ Strategy        │
│ Executor        │
└────────┬────────┘
         │
┌────────▼────────┐
│ Strategy        │
│ (BuyLowSellHigh)│
└─────────────────┘
```

## Quick Start

### Prerequisites

1. **Redis** - Must be running and accessible
   ```bash
   # Check if Redis is running
   redis-cli ping
   # Should return: PONG
   
   # If not running, start Redis:
   # On Linux:
   sudo systemctl start redis
   # Or using Docker:
   docker run -d -p 6379:6379 redis:latest
   ```

2. **Binance API Keys** - Configured in `config/trading_system.json`

3. **Rust** - Ensure you have Rust installed (1.70+)

### Building

```bash
# Build in release mode (optimized)
cargo build --release

# Or build in debug mode (faster compilation, slower runtime)
cargo build
```

### Running

```bash
# Run from project root
cargo run --release --package runner

# Or in debug mode
cargo run --package runner
```

## Configuration

Edit `config/trading_system.json` to adjust:

- **Strategy parameters**: position size, thresholds, stop-loss, take-profit
- **Log level**: "trace", "debug", "info", "warn", "error"
- **Server port**: Default 8080
- **Redis URL**: Default "redis://127.0.0.1/"
- **Exchange settings**: API keys, symbol, rate limits

Example configuration:
```json
{
  "system": {
    "log_level": "info",
    "shutdown_timeout_secs": 30
  },
  "exchange": {
    "exchange": "Binance",
    "binance": {
      "symbol": { "base": "BTC", "quote": "USDT" },
      "ws": { "apiKey": "...", "secretKey": "..." },
      "rest": { "apiKey": "...", "secretKey": "..." }
    }
  },
  "strategy": {
    "strategy_type": "BuyLowSellHigh",
    "position_size": "0.001",
    "max_position": "0.01",
    "buy_threshold_percent": "2.0",
    "sell_threshold_percent": "2.0",
    "stop_loss_percent": "5.0",
    "take_profit_percent": "10.0"
  },
  "server": {
    "bind_address": "0.0.0.0",
    "port": 8080
  }
}
```

## Health Server & Monitoring

The system exposes a REST API on port 8080 for monitoring and debugging.

### Endpoints

#### `GET /health`
Basic health check to verify the system is running.

```bash
curl http://localhost:8080/health
```

Response:
```json
{
  "status": "healthy",
  "timestamp": "2024-12-05T13:34:11.084623Z"
}
```

#### `GET /positions`
Get all current trading positions.

```bash
curl http://localhost:8080/positions
```

Response:
```json
{
  "positions": [
    {
      "symbol": "BTCUSDT",
      "quantity": "0.001",
      "avg_price": "45000.00"
    }
  ]
}
```

#### `GET /state`
Get overall trading state statistics.

```bash
curl http://localhost:8080/state
```

Response:
```json
{
  "active_orders": 2,
  "pending_orders": 1,
  "positions_count": 1,
  "total_position_value": "45.00"
}
```

#### `GET /orders`
Get detailed information about all active orders.

```bash
curl http://localhost:8080/orders
```

Response:
```json
{
  "orders": [
    {
      "order_id": "12345678",
      "symbol": "BTCUSDT",
      "side": "Buy",
      "price": "45000.00",
      "amount": "0.001",
      "status": "New",
      "filled_amount": "0.000"
    }
  ]
}
```

### How to Know if It's Trading

#### 1. Watch the Console Logs

Look for these messages:

**Strategy Signals**:
```
INFO trading_engine::strategies: BUY signal: price=45000.0, avg=45900.0, diff=1.96%
INFO trading_engine::strategies: SELL signal: price=47000.0, avg=46000.0, diff=2.17%
```

**Order Placement**:
```
INFO trading_engine::executor: Order placed: Order { ... }
```

**Order Execution**:
```
INFO trading_engine::strategies: Order executed: Buy 0.001 BTCUSDT @ 45000.0
```

**Risk Management**:
```
WARN trading_engine::strategies: Stop-loss triggered: loss=5.2%, threshold=5.0%
INFO trading_engine::strategies: Take-profit triggered: profit=10.5%, threshold=10.0%
```

#### 2. Check the Health Endpoint

```bash
# Check current positions
curl http://localhost:8080/positions

# Check trading state
curl http://localhost:8080/state
```

#### 3. Monitor Logs in Real-Time

```bash
# Filter important messages
cargo run --package runner 2>&1 | grep -E "BUY|SELL|Order placed|Order executed|Stop-loss|Take-profit"

# Or watch positions
watch -n 2 'curl -s http://localhost:8080/positions | jq'
```

#### 4. What to Expect

**Initial Phase** (First ~20 seconds):
- "Building price history" messages
- No trades yet - the strategy is collecting data

**Normal Operation**:
- Periodic "BUY signal" or "SELL signal" messages
- "Order placed" when orders are sent to Binance
- "Order executed" when orders are filled
- Position updates in the `/positions` endpoint

**No Trading Activity?**
The strategy might not be trading if:
- Price hasn't moved enough (needs 2% deviation from average)
- Max position reached (0.01 BTC as configured)
- Max active orders reached (2 orders as configured)
- Still building price history (< 20 data points)

## Strategy: BuyLowSellHigh

The default strategy implements a simple mean reversion approach:

- **Buys** when price drops **2%** below the 20-period moving average
- **Sells** when price rises **2%** above the moving average (if you have a position)
- **Stop-loss**: Sells immediately if position loses **5%**
- **Take-profit**: Sells immediately if position gains **10%**

### Strategy Parameters

- `position_size`: Fixed position size per trade (default: 0.001 BTC)
- `max_position`: Maximum total position size (default: 0.01 BTC)
- `buy_threshold_percent`: Buy when price is X% below average (default: 2.0%)
- `sell_threshold_percent`: Sell when price is X% above average (default: 2.0%)
- `stop_loss_percent`: Stop loss percentage (default: 5.0%)
- `take_profit_percent`: Take profit percentage (default: 10.0%)
- `max_active_orders`: Maximum concurrent orders (default: 2)
- `price_history_size`: Size of price history buffer (default: 100)
- `min_price_history`: Minimum price points before trading (default: 20)

## System Features

### Order Status Tracking

The system tracks order lifecycle:
- **New**: Order placed, waiting for fill
- **PartiallyFilled**: Order partially executed
- **Filled**: Order completely executed
- **Canceled**: Order was cancelled
- **Rejected**: Order was rejected by exchange
- **Expired**: Order expired

### Position Reconciliation

The system automatically reconciles positions with the exchange every 5 minutes:
- Fetches balances from exchange
- Compares with internal state
- Updates internal state if discrepancies are found
- Logs warnings for significant differences

### Circuit Breaker

Automatic error handling:
- Opens circuit after 5 consecutive failures
- Blocks new orders when circuit is open
- Automatically attempts recovery after timeout
- Prevents cascading failures

### Stale Order Cleanup

Automatically cancels orders that have been pending too long:
- Default timeout: 5 minutes
- Prevents orders from getting stuck
- Updates order status to Expired

### Retry Logic with Exponential Backoff

Order placement uses exponential backoff retry:
- **Max attempts**: 3 retries
- **Initial delay**: 500ms
- **Max delay**: 2000ms
- **Backoff multiplier**: 2.0x

This ensures transient network errors don't cause order failures.

### Multiple Strategies Support

The system supports running multiple strategies simultaneously via `StrategyManager`:
- Each strategy receives the same orderbook updates
- Strategies can trade different pairs or use different logic
- All strategies share the same trading client
- Useful for portfolio diversification or strategy comparison

## Testing

### Integration Tests

Run the integration test suite:

```bash
cargo test --test integration_test
```

Or run all integration tests:

```bash
cargo test integration::
```

Tests cover:
- Configuration loading and validation
- Strategy creation and state management
- Order status tracking
- Circuit breaker functionality
- Position reconciliation logic
- Stale order detection
- Health server structure

**Note**: Integration tests don't require real API keys or external services - they test component interactions in isolation.

### Unit Tests

Run all tests:

```bash
cargo test
```

### Examples

The project includes example binaries that demonstrate **low-level component APIs** for debugging and development:

**Exchange Engine Examples** (Low-level API usage):
```bash
# Place an order via WebSocket
cargo run --example example_binance_open_order --package exchange_engine

# Fetch account balances
cargo run --example example_binance_balances --package exchange_engine

# Cancel an order by ID
cargo run --example example_binance_cancel_by_id --package exchange_engine

# Cancel all orders for a pair
cargo run --example example_binance_cancel_all --package exchange_engine

# Test unsigned REST API calls (no auth required)
cargo run --example example_unsigned_calls --package exchange_engine
```

**Note**: 
- These examples demonstrate **low-level** component APIs for debugging/development
- For **full system integration**, use the main `runner` application: `cargo run --package runner`
- The `runner` demonstrates how all components (market data, exchange, trading engine) work together
- Examples require valid Binance API keys in `config/binance_config.json` and network connectivity

### Backtesting Framework

The system includes a basic backtesting framework for testing strategies on historical data:

```rust
use trading_engine::backtest::BacktestEngine;
use trading_engine::strategies::BuyLowSellHigh;

// Create strategy
let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(...)));

// Create backtest engine
let mut backtest = BacktestEngine::new(strategy, initial_balance);

// Run backtest on historical orderbooks
let result = backtest.run(historical_orderbooks).await?;

// Analyze results
println!("Final balance: {}", result.final_balance);
println!("Total trades: {}", result.trades.len());
println!("PnL: {}", result.pnl);
```

**Note**: The backtesting framework is basic and doesn't account for:
- Slippage
- Order book depth
- Market impact
- Realistic order execution delays

For production backtesting, consider using more sophisticated tools.

## Troubleshooting

### Redis Connection Error
```
Error: Redis connection failed
```
**Solution**: Ensure Redis is running and accessible at the configured URL

### Binance API Error
```
Error: Authentication failed
```
**Solution**: Check API keys in `config/trading_system.json` are correct

### WebSocket Connection Error
```
Error: Trading WebSocket error
```
**Solution**: 
- Check internet connection
- Verify Binance API keys are valid
- Check if Binance API is accessible

### Strategy Not Trading
- Check logs for "Building price history" - strategy needs 20+ price points
- Verify orderbook data is flowing (check Redis)
- Check if max position/active orders limits are reached

### Health Server Not Responding
1. Check if server started: Look for "Health check server listening on..." in logs
2. Check port: `netstat -tuln | grep 8080`
3. Check firewall: `sudo ufw status`

### Empty Responses from Health Endpoints
- System just started (no positions/orders yet)
- Strategy hasn't placed any orders
- All orders have been filled/cancelled

## Security Considerations

⚠️ **Important**: The health server currently has **no authentication**. Anyone with network access can view:
- Your trading positions
- Order details
- Trading activity

**Recommendations**:
1. **Bind to localhost only** if you don't need external access:
   ```json
   "bind_address": "127.0.0.1"
   ```

2. **Use a reverse proxy** (nginx, Caddy) with authentication

3. **Firewall rules** to restrict access

4. **Future enhancement**: Add API key authentication

## Graceful Shutdown

Press `Ctrl+C` to initiate graceful shutdown:
- All components receive shutdown signal
- Active orders are handled
- Connections are closed cleanly
- Timeout: 30 seconds (configurable in config)

## Redis Usage

The system uses Redis as a message bus for market data:

```bash
# Start Redis
redis-server

# View data via redis-cli
redis-cli
subscribe orderbooks  # Subscribe to orderbook channel
subscribe trade        # Subscribe to trade channel
```

## Example Run

```bash
$ cargo run --release --package runner

2024-01-15T10:00:00.123 INFO Starting Trading System
2024-01-15T10:00:00.456 INFO Starting Market Data Engine...
2024-01-15T10:00:02.789 INFO Creating Trading Client...
2024-01-15T10:00:03.012 INFO Trading client ready
2024-01-15T10:00:03.234 INFO Creating Buy Low Sell High Strategy...
2024-01-15T10:00:03.456 INFO Creating Strategy Executor...
2024-01-15T10:00:03.678 INFO Health check server listening on 0.0.0.0:8080
2024-01-15T10:00:03.890 INFO All systems ready! Starting strategy execution...
2024-01-15T10:00:04.123 DEBUG Building price history: 5/20
2024-01-15T10:00:04.456 DEBUG Building price history: 15/20
2024-01-15T10:00:05.789 INFO BUY signal: price=45000.0, avg=45900.0, diff=1.96%
2024-01-15T10:00:05.890 INFO Order placed: Order { ... }
2024-01-15T10:00:06.123 INFO Order executed: Buy 0.001 BTCUSDT @ 45000.0
...
```

## Next Steps

1. **Start with small position sizes** (0.001 BTC as configured)
2. **Monitor the health endpoint** to track positions
3. **Watch logs** to understand strategy behavior
4. **Adjust strategy parameters** in config as needed
5. **Test stop-loss/take-profit** by monitoring positions
6. **Integrate with monitoring tools** (Prometheus, Grafana)

## Development

### Adding a New Strategy

1. Implement the `Strategy` trait in `src/trading_engine/src/strategies.rs`:

```rust
use trading_engine::strategy::{Strategy, TradingState};

pub struct MyStrategy {
    state: TradingState,
    // ... your strategy fields
}

#[async_trait::async_trait]
impl Strategy for MyStrategy {
    async fn on_orderbook(
        &mut self,
        orderbook: &Orderbook,
        state: &TradingState,
    ) -> Result<Option<OrderRequest>, EngineError> {
        // Your strategy logic here
        Ok(None) // or Some(order_request)
    }

    async fn on_execution_report(
        &mut self,
        order: &Order,
    ) -> Result<(), EngineError> {
        // Handle order execution
        Ok(())
    }

    fn get_state(&self) -> &TradingState { &self.state }
    fn get_state_mut(&mut self) -> &mut TradingState { &mut self.state }
}
```

2. Add strategy creation logic in `src/runner/src/main.rs`
3. Update configuration to support your strategy parameters

### Adding a New Exchange

1. Implement the `TradingClient` trait in `src/exchange_engine/src/exchanges/`
2. Implement the `MarketDataClient` trait in `src/market_data_engine/src/exchanges/`
3. Add exchange-specific configuration types
4. Update `Exchange` enum in `src/core_logic/src/lib.rs`
5. Add exchange-specific config parsing in `src/runner/src/main.rs`

### Core Traits

The system uses several key traits for abstraction:

- **`TradingClient<OrderRequest>`**: Exchange order execution
  - `open_order()`: Place an order
  - `cancel_order_by_id()`: Cancel an order
  - `cancel_all_orders()`: Cancel all orders for a pair
  - `fetch_balance()`: Get account balances
  - `test_open_order()`: Test order placement (paper trading)

- **`MarketDataClient`**: Market data streaming
  - `orderbooks()`: Stream orderbook updates
  - `trades()`: Stream trade updates

- **`Strategy`**: Trading strategy logic
  - `on_orderbook()`: Called on each orderbook update
  - `on_execution_report()`: Called when an order is executed
  - `get_state()` / `get_state_mut()`: Access trading state

- **`PrivateDataClient`**: Authenticated WebSocket streams
  - `stream()`: Start authenticated stream
  - `shutdown()`: Gracefully stop stream

## Project Structure

```
trading_system/
├── config/
│   └── trading_system.json    # Main configuration file
├── src/
│   ├── core_logic/            # Core abstractions and data types
│   │   ├── config.rs          # Unified configuration
│   │   ├── data/              # Data types (Order, Orderbook, etc.)
│   │   ├── traits.rs          # Core traits (TradingClient, etc.)
│   │   ├── error.rs           # Error types
│   │   ├── retry.rs           # Retry logic
│   │   ├── circuit_breaker.rs # Circuit breaker pattern
│   │   └── redis.rs           # Redis message bus
│   ├── exchange_engine/       # Exchange implementations
│   │   └── exchanges/
│   │       └── binance/       # Binance implementation
│   │           ├── ws_client.rs
│   │           ├── rest_client.rs
│   │           └── websocket/ # WebSocket types
│   ├── market_data_engine/    # Market data collection
│   │   └── exchanges/
│   │       └── binance/       # Binance market data
│   ├── trading_engine/        # Strategy execution
│   │   ├── strategy.rs        # Strategy trait
│   │   ├── strategies.rs      # Strategy implementations
│   │   ├── executor.rs        # Strategy executor
│   │   ├── reconciliation.rs  # Position reconciliation
│   │   ├── backtest.rs        # Backtesting framework
│   │   └── strategy_manager.rs # Multiple strategies
│   └── runner/                # Main application
│       ├── main.rs            # Application entry point
│       ├── health.rs          # Health server
│       └── shutdown.rs        # Graceful shutdown
├── tests/
│   └── integration_test.rs    # Integration tests
└── Cargo.toml                 # Workspace configuration
```

## Error Handling

The system uses a comprehensive error handling system:

### Error Types

- **`EngineError`**: Main error type with variants for:
  - WebSocket connection errors
  - HTTP request errors
  - Authentication errors
  - Order errors (not found, already filled, etc.)
  - Redis errors
  - Configuration errors
  - Custom errors

### Error Recovery

- **Retry Logic**: Automatic retry with exponential backoff for transient errors
- **Circuit Breaker**: Prevents cascading failures by opening circuit after repeated failures
- **Graceful Degradation**: System continues operating even if some components fail

### Error Context

All errors include context information:
- Error messages include relevant parameters
- Stack traces in debug mode
- Structured logging for error analysis

## Performance Considerations

- **Async/Await**: All I/O operations are asynchronous
- **Connection Pooling**: Reuses connections where possible
- **Efficient Serialization**: Uses `smol_str` for string interning
- **Minimal Allocations**: Reuses buffers and avoids unnecessary clones
- **Rate Limiting**: Respects exchange rate limits

## Limitations

- **Single Exchange**: Currently only supports Binance
- **Spot Trading Only**: No futures or margin trading
- **No Database**: Positions and orders are in-memory only
- **Basic Backtesting**: Backtesting framework is simplified
- **No Authentication**: Health server has no authentication
- **Limited Error Recovery**: Some errors may require manual intervention

## Future Enhancements

- [ ] Support for multiple exchanges (Coinbase, Kraken, etc.)
- [ ] Futures and margin trading support
- [ ] Database persistence for positions and orders
- [ ] Advanced backtesting with realistic execution simulation
- [ ] Authentication for health server
- [ ] Metrics and observability (Prometheus, OpenTelemetry)
- [ ] WebSocket reconnection with state recovery
- [ ] Order book depth analysis
- [ ] Advanced risk management
- [ ] Portfolio management across multiple pairs

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Ensure all tests pass: `cargo test`
6. Run clippy: `cargo clippy`
7. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
