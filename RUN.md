# Running the Trading System

## Prerequisites

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

2. **Binance API Keys** - Already configured in `config/trading_system.json`

3. **Rust** - Ensure you have Rust installed (1.70+)

## Building the System

```bash
# Build in release mode (optimized)
cargo build --release

# Or build in debug mode (faster compilation, slower runtime)
cargo build
```

## Running the System

### Option 1: Using Cargo Run (Recommended)

```bash
# Run from project root
cargo run --release --bin runner

# Or in debug mode
cargo run --bin runner
```

### Option 2: Run the Binary Directly

```bash
# After building
./target/release/runner

# Or debug binary
./target/debug/runner
```

## What Happens When You Run

1. **Configuration Loading**: System loads `config/trading_system.json`
2. **Market Data Engine**: Starts consuming orderbook data from Binance and publishing to Redis
3. **Trading Client**: Connects to Binance WebSocket for order execution
4. **Strategy Executor**: Begins running the BuyLowSellHigh strategy
5. **Health Server**: Starts HTTP server on port 8080

## Testing the System

### 1. Check Health Endpoint

Once the system is running, test the health endpoint:

```bash
# Check if system is healthy
curl http://localhost:8080/health

# Expected response:
# {"status":"healthy","timestamp":"2024-..."}

# Check current positions
curl http://localhost:8080/positions

# Expected response:
# {"positions":[...]}
```

### 2. Monitor Logs

The system logs important events:
- Strategy signals (BUY/SELL)
- Order placements
- Execution reports
- Errors

Watch logs in real-time:
```bash
# If using cargo run
cargo run --release --bin runner

# Logs will show:
# - "Starting Trading System"
# - "Market Data Engine ready"
# - "Trading client ready"
# - "BUY signal: price=..., avg=..."
# - "SELL signal: ..."
```

### 3. Test Strategy Logic

The strategy will:
- Build price history (needs 20 price points minimum)
- Calculate moving average
- Generate BUY signals when price drops 2% below average
- Generate SELL signals when price rises 2% above average
- Apply stop-loss (5%) and take-profit (10%) if configured

### 4. Graceful Shutdown

Press `Ctrl+C` to initiate graceful shutdown:
- All components receive shutdown signal
- Active orders are handled
- Connections are closed cleanly
- Timeout: 30 seconds (configurable)

## Configuration

Edit `config/trading_system.json` to adjust:

- **Strategy parameters**: position size, thresholds, stop-loss, take-profit
- **Log level**: "trace", "debug", "info", "warn", "error"
- **Server port**: Default 8080
- **Redis URL**: Default "redis://127.0.0.1/"

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

## Monitoring

### Health Check Endpoints

- `GET /health` - System health status
- `GET /positions` - Current trading positions
- `GET /` - Same as `/health`

### Log Levels

Set in config:
- `"trace"` - Very verbose, all events
- `"debug"` - Detailed debugging info
- `"info"` - Normal operation (recommended)
- `"warn"` - Warnings only
- `"error"` - Errors only

## Example Run

```bash
$ cargo run --release --bin runner

2024-01-15T10:00:00.123 INFO Starting Trading System
2024-01-15T10:00:00.456 INFO Starting Market Data Engine...
2024-01-15T10:00:02.789 INFO Creating Trading Client...
2024-01-15T10:00:03.012 INFO Trading client ready
2024-01-15T10:00:03.234 INFO Creating Buy Low Sell High Strategy...
2024-01-15T10:00:03.456 INFO Creating Strategy Executor...
2024-01-15T10:00:03.678 INFO Health check server listening on 0.0.0.0:8080
2024-01-15T10:00:03.890 INFO All systems ready! Starting strategy execution...
2024-01-15T10:00:04.123 DEBUG Building price history: 5/20
2024-01-00:04.456 DEBUG Building price history: 15/20
2024-01-15T10:00:05.789 INFO BUY signal: price=45000.0, avg=45900.0, diff=1.96%
2024-01-15T10:00:05.890 INFO Order placed: Order { ... }
...
```

## Next Steps

1. **Start with small position sizes** (0.001 BTC as configured)
2. **Monitor the health endpoint** to track positions
3. **Watch logs** to understand strategy behavior
4. **Adjust strategy parameters** in config as needed
5. **Test stop-loss/take-profit** by monitoring positions

