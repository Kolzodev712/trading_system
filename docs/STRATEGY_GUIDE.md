# Strategy Guide

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Trading System supports algorithmic trading strategies that can run continuously in the background. Strategies analyze market data in real-time and make trading decisions based on configurable parameters.

## Available Strategies

### Buy Low, Sell High

A mean reversion strategy that buys when prices drop below the average and sells when prices rise above the average.

**Core Concept**:
- Maintains a rolling average of recent prices
- Buys when current price is significantly below the average (mean reversion opportunity)
- Sells when current price is significantly above the average (profit taking)
- Uses adaptive thresholds based on market volatility

**Key Features**:
- **Volatility-adaptive thresholds**: Adjusts buy/sell thresholds based on market volatility
- **EMA smoothing**: Uses Exponential Moving Average for price smoothing
- **Trend filter**: Avoids trades against strong market trends
- **Dynamic position sizing**: Adjusts trade size based on market conditions
- **Spread/liquidity filters**: Avoids trades in illiquid or high-spread conditions
- **Time-based filters**: Restricts trading during specific times
- **RSI/momentum confirmation**: Uses Relative Strength Index for trade confirmation
- **Smart order type selection**: Dynamically chooses between limit and market orders
- **Performance tracking**: Adjusts strategy parameters based on recent performance

## Strategy Configuration

### Configuration File

Strategies are configured in `config/strategies.json`:

```json
{
  "strategy_type": "BuyLowSellHigh",
  "symbol": {
    "base": "BTC",
    "quote": "USDT"
  },
  "position_size": 0.001,
  "max_position": 0.01,
  "buy_threshold_percent": 2.0,
  "sell_threshold_percent": 2.0,
  "stop_loss_percent": 5.0,
  "take_profit_percent": 10.0,
  "max_active_orders": 5,
  "price_history_size": 100,
  "min_price_history": 20
}
```

### Parameters

#### Basic Parameters

- **`strategy_type`** (string, required): Strategy name ("BuyLowSellHigh")
- **`symbol`** (object, required): Trading pair
  - `base`: Base currency (e.g., "BTC")
  - `quote`: Quote currency (e.g., "USDT")
- **`position_size`** (decimal, required): Base position size per trade (e.g., 0.001 BTC)
- **`max_position`** (decimal, required): Maximum total position size (e.g., 0.01 BTC)
- **`buy_threshold_percent`** (decimal, required): Percentage below average price to trigger buy (e.g., 2.0 = 2%)
- **`sell_threshold_percent`** (decimal, required): Percentage above average price to trigger sell (e.g., 2.0 = 2%)
- **`stop_loss_percent`** (decimal, optional): Stop loss percentage (e.g., 5.0 = 5%)
- **`take_profit_percent`** (decimal, optional): Take profit percentage (e.g., 10.0 = 10%)
- **`max_active_orders`** (integer, required): Maximum number of active orders (e.g., 5)
- **`price_history_size`** (integer, required): Size of price history buffer (e.g., 100)
- **`min_price_history`** (integer, required): Minimum price history required before trading (e.g., 20)

## Running Strategies

### Starting the Strategy

1. **Configure the strategy** in `config/strategies.json` (see above)

2. **Start the runner** in strategies mode:
   ```bash
   cargo run --release --bin runner -- --mode strategies
   ```

3. **Monitor the strategy** using the TUI CLI:
   ```bash
   cargo run --release --package cli
   ```

### Strategy Monitoring

The TUI CLI displays a **Strategy panel** (replaces the Levels panel in strategies mode) showing:

- **Strategy Name**: Current strategy (e.g., "BuyLowSellHigh")
- **Status**: Running (● green) or Paused (⏸ red)
- **Symbol**: Trading pair (e.g., "BTCUSDT")
- **Position**: Current position quantity and average price
- **P&L**: Unrealized profit/loss (if available)
- **Orders**: Active orders count (e.g., "2/5")
- **Last Signal**: Last trading signal (Buy/Sell/None)

### Trading Control

**Per-Strategy Enable/Disable**:
- Press `T` in the Trading Monitor to toggle the **selected** strategy
- Each strategy can be enabled/disabled independently
- When disabled, the strategy continues monitoring but won't place orders
- Strategy panel shows status: "●" (green, enabled) or "⏸" (red, disabled) for each strategy
- Disabling a strategy automatically cancels all active orders for that strategy's trading pair

**How It Works**:
- The `StrategyManager` maintains per-strategy enabled flags (keyed by `strategy_name:symbol`)
- The strategy executor checks the enabled flag before processing orderbook updates
- If a strategy is disabled, it's skipped during orderbook processing
- Market analysis continues for all strategies, but disabled strategies won't place orders
- This allows you to pause individual strategies without stopping the entire system

**Multiple Strategies**:
- When running multiple strategies (e.g., ETH/USDC and SOL/USDC), each can be toggled independently
- Use arrow keys (↑/↓) to select a strategy, then press `T` to toggle it
- The strategy ID format is `strategy_name:symbol` (e.g., "BuyLowSellHigh:ETH/USDC")

## Strategy Behavior

### Buy Low, Sell High Strategy

#### Entry Conditions

**Buy Signal**:
1. Current price is below the EMA (Exponential Moving Average)
2. Price deviation exceeds the buy threshold (adjusted for volatility)
3. RSI indicates oversold conditions (if RSI is available)
4. Market trend is not strongly bearish
5. Spread is within acceptable limits
6. Trading hours are active
7. Minimum price history is available

**Sell Signal**:
1. Current price is above the EMA
2. Price deviation exceeds the sell threshold (adjusted for volatility)
3. RSI indicates overbought conditions (if RSI is available)
4. Market trend is not strongly bullish
5. Spread is within acceptable limits
6. Trading hours are active
7. Minimum price history is available

#### Risk Management

- **Position Limits**: Maximum position size enforced
- **Order Limits**: Maximum active orders enforced
- **Stop Loss**: Optional stop loss on positions
- **Take Profit**: Optional take profit targets
- **Circuit Breaker**: System-wide circuit breaker prevents catastrophic losses
- **Daily Loss Limits**: Global and per-pair daily loss limits

#### Adaptive Features

**Volatility Tracking**:
- Monitors price volatility over a rolling window
- Adjusts buy/sell thresholds based on volatility
- Higher volatility = wider thresholds, lower volatility = tighter thresholds

**Trend Detection**:
- Uses EMA crossovers to detect trends
- Avoids trades against strong trends
- Reduces false signals in trending markets

**Performance Tracking**:
- Monitors win rate and consecutive losses
- Adjusts thresholds based on recent performance
- Reduces trading activity after consecutive losses

**Liquidity Analysis**:
- Monitors orderbook spread
- Monitors orderbook depth
- Avoids trades in illiquid conditions

**Time Filtering**:
- Can restrict trading to specific hours
- Can avoid trading on weekends
- Configurable time-based restrictions

## Strategy Architecture

### Strategy Executor

The `StrategyExecutor` runs strategies in a continuous loop:

1. **Subscribes to market data** (orderbook updates)
2. **Executes strategy logic** on each update
3. **Checks trading control** (enabled/disabled)
4. **Performs risk checks** before placing orders
5. **Places orders** via TradingClient
6. **Updates state** based on execution reports

### Strategy State

Strategies maintain their own state:
- Price history
- Position tracking
- Order tracking
- Performance metrics

### Integration Points

- **Market Data Engine**: Receives orderbook updates
- **Indicator Engine**: Gets EMA and RSI values
- **Trading State Manager**: Tracks positions and orders
- **Risk Manager**: Performs risk checks
- **Trading Client**: Places orders on exchange

## Best Practices

### Configuration

1. **Start Conservative**: Begin with small position sizes and wide thresholds
2. **Monitor Performance**: Watch the Strategy panel for position and P&L
3. **Adjust Gradually**: Modify parameters based on observed behavior
4. **Test First**: Use testnet or small amounts initially

### Risk Management

1. **Set Stop Loss**: Always configure stop loss for protection
2. **Limit Position Size**: Keep max_position reasonable
3. **Monitor Daily P&L**: Watch for daily loss limit breaches
4. **Use Trading Control**: Disable trading when uncertain

### Monitoring

1. **Watch Strategy Panel**: Monitor position, P&L, and active orders
2. **Check Logs**: Review logs for strategy decisions
3. **Monitor Market Conditions**: Be aware of market volatility
4. **Review Performance**: Track win rate and profitability

## Troubleshooting

### Strategy Not Trading

**Possible Causes**:
- Trading is disabled (check Strategy panel status)
- Minimum price history not reached
- Risk limits preventing trades
- Market conditions not met (spread, liquidity, time filters)
- Circuit breaker is open

**Debugging**:
1. Check Strategy panel status (should show "● Running")
2. Check logs for strategy decisions
3. Verify price history is sufficient
4. Check risk limits in config
5. Monitor market data flow

### Strategy Trading Too Much

**Possible Causes**:
- Thresholds too tight
- Volatility tracking not working
- Trend filter not active

**Solutions**:
1. Increase buy/sell thresholds
2. Check volatility tracking configuration
3. Enable trend filter

### Strategy Not Profitable

**Possible Causes**:
- Thresholds not optimal for market conditions
- Position sizing too large/small
- Market conditions not suitable for strategy

**Solutions**:
1. Adjust thresholds based on market volatility
2. Optimize position sizing
3. Consider different strategy or market conditions

## Advanced Configuration

### Indicator Integration

The strategy can use indicators from the Indicator Engine:
- **EMA**: Exponential Moving Average for price smoothing
- **RSI**: Relative Strength Index for momentum confirmation

To enable indicators, ensure the Indicator Engine is running and indicators are configured.

### Performance Optimization

The strategy includes several performance optimizations:
- **Cached Calculations**: EMA and RSI values are cached
- **Efficient State Updates**: Minimal state updates per market data event
- **Smart Order Selection**: Chooses optimal order type based on conditions

## Future Strategies

Additional strategies can be added by:
1. Creating a new strategy module in `src/trading_engine/src/strategies/rust_strategies/`
2. Implementing the `Strategy` trait
3. Registering the strategy in the strategy manager
4. Adding configuration support

See the architecture documentation for details on implementing new strategies.

## See Also

- [Trading Engine Architecture](../architecture/03-trading-engine.md)
- [TUI CLI Architecture](../architecture/09-tui-cli.md)
- [Main README](../README.md)

