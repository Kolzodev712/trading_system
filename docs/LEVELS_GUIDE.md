# Levels Guide

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Trading System supports level-based trading, which allows you to define trading levels that automatically place orders based on indicator values, orderbook prices, or fixed prices. Levels are exchange-agnostic and pair-agnostic templates that get expanded to create instances for each configured trading pair.

**Key Features**:
- **Indicator-based pricing**: Use technical indicators (SMA, EMA, RSI, etc.) to dynamically calculate order prices
- **Flexible quantity configuration**: Fixed quantities, indicator-based, balance percentage, or notional values
- **Auto-reposting**: Automatically repost orders after fills or cancellations
- **Automatic order amendments**: Automatically amends existing orders when price/quantity changes (no cancel+place)
- **Risk management**: Per-level and global risk limits
- **Lifecycle management**: Control when levels repost and under what conditions
- **Multiple order types**: Market, Limit, PostOnly, IoC, FoK
- **State tracking**: Monitor level states (Created, Posted, Filled, Cancelled, Paused, Removed)

## Core Concepts

### What is a Level?

A **level** is a trading instruction that defines:
- **When to trade**: Based on price conditions (indicator values, orderbook prices, or fixed prices)
- **What to trade**: Trading pair (expanded from configuration)
- **How much to trade**: Quantity configuration (fixed, indicator-based, or balance-based)
- **Order type**: Market, Limit, PostOnly, IoC, or FoK
- **Risk limits**: Maximum position, order count, and daily loss limits
- **Lifecycle rules**: Auto-repost behavior and conditions

### Level States

Levels transition through the following states:

1. **Created**: Level created but not yet posted (no active orders)
2. **Posted**: Order placed, waiting for fill
3. **Filled**: Order filled, waiting to repost (if auto-repost enabled)
4. **Cancelled**: Order cancelled, waiting to repost (if auto-repost on cancel enabled)
5. **Paused**: Level paused (manual or condition-based)
6. **Removed**: Level removed from system

### Level Expansion

Level configurations are **pair-agnostic templates**. When you configure a level, it gets automatically expanded to create instances for each trading pair configured in the bot. For example:

- **Config**: `buy_level_1` (template)
- **Expanded**: `buy_level_1_ETHUSDC`, `buy_level_1_BTCUSDT` (if both pairs are configured)

This allows you to define a single level configuration that applies to multiple trading pairs.

## Level Configuration

### Configuration File

Levels are configured in `config/levels.json`:

```json
{
  "levels": [
    {
      "id": "buy_level_1",
      "name": "Buy at SMA-0.5%",
      "side": "Buy",
      "order_type": "Limit",
      "quantity": {
        "type": "Fixed",
        "value": "0.002"
      },
      "price": {
        "Indicator": {
          "indicator_name": "sma_20",
          "offset_type": "PercentBelow",
          "offset_value": "0.5"
        }
      },
      "risk": {
        "max_position": "0.01",
        "max_orders": 2,
        "daily_loss_limit": "50.0"
      },
      "lifecycle": {
        "auto_repost": true,
        "auto_repost_on_cancel": true,
        "max_reposts": 5,
        "repost_delay_secs": 5,
        "repost_conditions": []
      },
      "enabled": true,
      "group": "main_strategy"
    }
  ],
  "risk_limits": {
    "total_max_position": "0.1",
    "max_total_orders": 20,
    "daily_loss_limit_all_pairs": "500.0",
    "daily_loss_limit_per_pair": {
      "ETHUSDC": "200.0"
    }
  }
}
```

### Basic Parameters

#### Required Fields

- **`id`** (string, required): Unique identifier for the level (e.g., "buy_level_1")
- **`side`** (string, required): Trading side - "Buy" or "Sell"
- **`order_type`** (string, required): Order type - "Market", "Limit", "PostOnly", "IoC", or "FoK"
- **`quantity`** (object, required): Quantity configuration (see [Quantity Configuration](#quantity-configuration))
- **`price`** (object, required): Price configuration (see [Price Configuration](#price-configuration))
- **`risk`** (object, required): Risk parameters (see [Risk Management](#risk-management))
- **`lifecycle`** (object, required): Lifecycle rules (see [Lifecycle Management](#lifecycle-management))

#### Optional Fields

- **`name`** (string, optional): Human-readable name/description (e.g., "Buy at SMA-0.5%")
- **`enabled`** (boolean, optional, default: `false`): Whether the level is enabled
- **`group`** (string, optional): Level group for management (e.g., "main_strategy")

## Price Configuration

Price configuration determines where orders are placed. You can use fixed prices, indicator-based prices, or orderbook-based prices.

### Fixed Price

Place orders at a fixed price:

```json
"price": {
  "Fixed": "3500.0"
}
```

### Indicator-Based Price

Place orders relative to an indicator value:

```json
"price": {
  "Indicator": {
    "indicator_name": "sma_20",
    "offset_type": "PercentBelow",
    "offset_value": "0.5"
  }
}
```

**Parameters**:
- **`indicator_name`** (string, required): Indicator name (e.g., "sma_20", "ema_50", "rsi_14")
  - If pair-agnostic (e.g., "sma_20"), it gets resolved to pair-specific (e.g., "sma_20_ETHUSDC")
  - If pair-specific (e.g., "sma_20_ETHUSDC"), it's used as-is
- **`offset_type`** (string, required): Offset type - "Absolute", "Percent", "PercentBelow", or "PercentAbove"
- **`offset_value`** (decimal, required): Offset value

**Offset Types**:
- **`Absolute`**: Add/subtract absolute value (e.g., `indicator_value + offset_value`)
- **`Percent`**: Add/subtract percentage (e.g., `indicator_value * (1 + offset_value/100)`)
- **`PercentBelow`**: X% below indicator (e.g., `indicator_value * (1 - offset_value/100)`)
- **`PercentAbove`**: X% above indicator (e.g., `indicator_value * (1 + offset_value/100)`)

**Examples**:
```json
// Buy 0.5% below SMA-20
{
  "Indicator": {
    "indicator_name": "sma_20",
    "offset_type": "PercentBelow",
    "offset_value": "0.5"
  }
}

// Sell 1.0% above EMA-50
{
  "Indicator": {
    "indicator_name": "ema_50",
    "offset_type": "PercentAbove",
    "offset_value": "1.0"
  }
}

// Buy $10 below RSI-based price
{
  "Indicator": {
    "indicator_name": "custom_price_indicator",
    "offset_type": "Absolute",
    "offset_value": "-10.0"
  }
}
```

### Orderbook-Based Price

Place orders relative to orderbook prices:

```json
// Best bid with offset
"price": {
  "BestBid": {
    "offset": "-0.5"
  }
}

// Best ask with offset
"price": {
  "BestAsk": {
    "offset": "0.5"
  }
}

// Mid price with percentage offset
"price": {
  "MidPrice": {
    "offset_percent": "-0.25"
  }
}
```

**Parameters**:
- **`BestBid`**: Best bid price + offset (absolute value)
- **`BestAsk`**: Best ask price + offset (absolute value)
- **`MidPrice`**: Mid price + percentage offset (e.g., `mid_price * (1 + offset_percent/100)`)

## Quantity Configuration

Quantity configuration determines how much to trade. You can use fixed quantities, indicator-based quantities, balance percentages, or notional values.

### Fixed Quantity

Trade a fixed amount:

```json
"quantity": {
  "type": "Fixed",
  "value": "0.002"
}
```

### Indicator-Based Quantity

Trade based on an indicator value:

```json
"quantity": {
  "type": "Indicator",
  "indicator_name": "volume_indicator",
  "multiplier": "1.5",
  "offset": "0.001"
}
```

**Parameters**:
- **`indicator_name`** (string, required): Indicator name
- **`multiplier`** (decimal, optional): Multiply indicator value (e.g., `indicator_value * multiplier`)
- **`offset`** (decimal, optional): Add/subtract from indicator (e.g., `indicator_value + offset`)

**Calculation**: `(indicator_value * multiplier) + offset`

### Balance Percentage

Trade a percentage of available balance:

```json
"quantity": {
  "type": "BalancePercent",
  "value": "10.0"
}
```

**Note**: 
- For **Buy** orders: Uses quote currency balance (e.g., USDT)
- For **Sell** orders: Uses base currency balance (e.g., ETH)

### Notional Value

Trade a fixed notional value (price * quantity = notional):

```json
"quantity": {
  "type": "Notional",
  "value": "100.0"
}
```

**Calculation**: `quantity = notional_value / price`

## Risk Management

### Per-Level Risk Limits

Each level can have its own risk limits:

```json
"risk": {
  "max_position": "0.01",
  "max_orders": 2,
  "daily_loss_limit": "50.0"
}
```

**Parameters**:
- **`max_position`** (decimal, optional): Maximum position size for this level (in base currency)
- **`max_orders`** (integer, optional): Maximum number of active orders for this level
- **`daily_loss_limit`** (decimal, optional): Daily loss limit for this level (in quote currency)

### Global Risk Limits

Global risk limits apply to all levels:

```json
"risk_limits": {
  "total_max_position": "0.1",
  "max_total_orders": 20,
  "daily_loss_limit_all_pairs": "500.0",
  "daily_loss_limit_per_pair": {
    "ETHUSDC": "200.0"
  }
}
```

**Parameters**:
- **`total_max_position`** (decimal, optional): Maximum total position size across all levels
- **`max_total_orders`** (integer, optional): Maximum total number of orders across all levels
- **`daily_loss_limit_all_pairs`** (decimal, optional): Daily loss limit for all pairs combined
- **`daily_loss_limit_per_pair`** (object, optional): Daily loss limit per trading pair (key: pair symbol like "ETHUSDC", value: limit)

### Risk Check Order

Risk checks are performed in this order:

1. **Circuit Breaker**: System-wide circuit breaker (highest priority)
2. **Daily Loss Limits**: Per-level, per-pair, and global daily loss limits
3. **Balance Check**: Sufficient balance for the order
4. **Total Position Limits**: Global and per-level position limits
5. **Order Count Limits**: Global and per-level order count limits

If any check fails, the order is blocked.

## Lifecycle Management

### Auto-Reposting

Levels can automatically repost orders after fills or cancellations:

```json
"lifecycle": {
  "auto_repost": true,
  "auto_repost_on_cancel": true,
  "max_reposts": 5,
  "repost_delay_secs": 5,
  "repost_conditions": []
}
```

**Parameters**:
- **`auto_repost`** (boolean, optional, default: `true`): Auto-repost after order is filled
- **`auto_repost_on_cancel`** (boolean, optional, default: `true`): Auto-repost after order is cancelled
- **`max_reposts`** (integer, optional): Maximum number of reposts before stopping (if not set, unlimited)
- **`repost_delay_secs`** (integer, optional): Delay in seconds before reposting (if not set, no delay)
- **`repost_conditions`** (array, optional): Conditions that prevent reposting (see below)

### Repost Conditions

You can configure conditions that prevent reposting:

```json
"repost_conditions": [
  "DailyLossLimitReached",
  "PositionLimitReached",
  "InsufficientBalance",
  {"PnLBelowThreshold": "-100.0"}
]
```

**Available Conditions**:
- **`DailyLossLimitReached`**: Don't repost if daily loss limit reached
- **`PositionLimitReached`**: Don't repost if position limit reached
- **`InsufficientBalance`**: Don't repost if balance insufficient
- **`PnLBelowThreshold`** (decimal): Don't repost if P&L below threshold (negative value)

### Repost Logic

1. **Order Filled**: If `auto_repost` is `true`, level transitions to `Filled` state and will repost after delay (if configured)
2. **Order Cancelled**: If `auto_repost_on_cancel` is `true`, level transitions to `Cancelled` state and will repost after delay (if configured)
3. **Repost Count**: Increments on each repost; stops if `max_reposts` is reached
4. **Repost Conditions**: Checks all conditions before reposting; if any condition is met, repost is blocked

## Running Levels

### Starting Level-Based Trading

1. **Configure levels** in `config/levels.json` (see [Level Configuration](#level-configuration))

2. **Start the runner** in levels mode:
   ```bash
   cargo run --release --bin runner -- --mode levels
   ```

3. **Monitor levels** using the TUI CLI:
   ```bash
   cargo run --release --package cli
   ```

### Level Monitoring

The TUI CLI displays a **Levels panel** showing:

- **Level ID**: Unique identifier (e.g., "buy_level_1_ETHUSDC")
- **Status**: Current state (Created, Posted, Filled, Cancelled, Paused, Removed)
- **Enabled**: ● (green, enabled) or ⏸ (red, disabled)
- **Pair**: Trading pair (e.g., "ETH/USDC")
- **Side**: Buy or Sell
- **Price**: Resolved price (from indicator or fixed)
- **Quantity**: Resolved quantity
- **Active Orders**: Number of active orders for this level
- **Repost Count**: Number of times the level has been reposted

### Trading Control

**Per-Level Enable/Disable**:
- Press `T` in the Trading Monitor to toggle the **selected** level
- Each level can be enabled/disabled independently
- When disabled, the level won't post new orders (existing orders remain active)
- Level panel shows status: "●" (green, enabled) or "⏸" (red, disabled) for each level

**How It Works**:
- The `LevelManager` maintains per-level enabled flags
- The event loop checks the enabled flag before posting orders
- If a level is disabled, it's skipped during orderbook processing
- Market data processing continues for all levels, but disabled levels won't post orders

**Multiple Levels**:
- When running multiple levels (e.g., buy and sell levels for ETH/USDC), each can be toggled independently
- Use arrow keys (↑/↓) to select a level, then press `T` to toggle it
- The level ID format is `level_id_PAIR` (e.g., "buy_level_1_ETHUSDC")

## Level Behavior

### Order Posting and Amendment

Levels automatically handle order placement and amendments:

1. **New Order Placement**: When level is ready (state is `Created` and no active orders)
2. **Order Amendment**: When level has an active order and resolved price/quantity changes
3. **Automatic Decision**: System automatically chooses amend vs. cancel+place based on order state and changes

**Amendment Logic**:
- **Amend is used** when:
  - Level has an active order in `Posted` state
  - Only price and/or quantity changed (side and order type unchanged)
  - Order status is `New` or `PartiallyFilled` (can be amended)
  
- **Cancel+Place is used** when:
  - Order is filled or cancelled (cannot be amended)
  - Side changed (must cancel and place new)
  - Order type changed (must cancel and place new)
  - Amendment fails (fallback to cancel+place)

**Benefits**:
- **Reduced latency**: Amendments are faster than cancel+place
- **Better execution**: Maintains order priority and reduces market impact
- **Automatic**: No manual intervention needed - system decides automatically

### Order Posting

Levels post orders when:

1. **Level is ready**: State is `Created` and no active orders
2. **Level is enabled**: `enabled` flag is `true`
3. **Risk checks pass**: All risk checks (circuit breaker, balance, limits) pass
4. **Indicators available**: Required indicators are available (if using indicator-based price/quantity)
5. **Orderbook available**: Orderbook data is available (if using orderbook-based price)

### Price Resolution

Price resolution happens in real-time:

1. **Indicator Resolution**: If using indicator-based price, the indicator value is fetched from the indicator cache
2. **Offset Calculation**: Offset is applied based on `offset_type` and `offset_value`
3. **Tick Size Rounding**: Price is rounded to exchange tick size (e.g., 0.01 for ETH/USDC)
4. **Validation**: Price is validated against exchange rules (min/max price, price precision)

### Quantity Resolution

Quantity resolution happens in real-time:

1. **Quantity Calculation**: Based on quantity configuration (Fixed, Indicator, BalancePercent, or Notional)
2. **Balance Check**: For BalancePercent or Notional, balance is checked
3. **Step Size Rounding**: Quantity is rounded to exchange step size (e.g., 0.0001 for ETH)
4. **Validation**: Quantity is validated against exchange rules (min order size, quantity precision)

### State Transitions

```
Created → Posted → Filled → Created (if auto_repost)
         ↓
      Cancelled → Created (if auto_repost_on_cancel)
         ↓
      Paused (manual or condition)
         ↓
      Removed (manual)
```

**Note**: When a level is in `Posted` state and the resolved price/quantity changes (e.g., due to indicator updates), the system automatically amends the existing order instead of canceling and placing a new one. This maintains order priority and reduces market impact.

## Best Practices

### Configuration

1. **Start Conservative**: Begin with small quantities and wide price offsets
2. **Test Indicators**: Ensure indicators are available and working before using them
3. **Monitor Performance**: Watch the Levels panel for state transitions and order activity
4. **Adjust Gradually**: Modify parameters based on observed behavior
5. **Use Groups**: Organize levels into groups for easier management

### Risk Management

1. **Set Position Limits**: Always configure `max_position` to prevent overexposure
2. **Limit Order Count**: Set `max_orders` to prevent order spam
3. **Configure Daily Loss Limits**: Set per-level and global daily loss limits
4. **Monitor Risk Metrics**: Watch for risk limit breaches in logs
5. **Use Repost Conditions**: Configure repost conditions to prevent reposting in adverse conditions

### Price Configuration

1. **Use Indicators**: Prefer indicator-based prices for dynamic trading
2. **Test Offsets**: Verify offset calculations match your expectations
3. **Consider Market Conditions**: Adjust offsets based on volatility
4. **Use Orderbook Prices**: For market-making strategies, use BestBid/BestAsk/MidPrice

### Quantity Configuration

1. **Start Small**: Use small fixed quantities initially
2. **Balance Percentages**: Use BalancePercent for position sizing relative to available capital
3. **Notional Values**: Use Notional for fixed dollar amounts
4. **Indicator-Based**: Use indicator-based quantities for dynamic sizing (advanced)

### Lifecycle Management

1. **Enable Auto-Repost**: Use `auto_repost` for continuous trading
2. **Set Max Reposts**: Configure `max_reposts` to prevent infinite reposting
3. **Add Delays**: Use `repost_delay_secs` to avoid rapid reposting
4. **Configure Conditions**: Use repost conditions to prevent reposting in adverse conditions

## Troubleshooting

### Level Not Posting Orders

**Possible Causes**:
- Level is disabled (check Levels panel status)
- Level state is not `Created` (check state in Levels panel)
- Risk checks failing (check logs for risk check failures)
- Indicators not available (check indicator availability)
- Orderbook not available (check market data flow)
- Circuit breaker is open (check logs)

**Debugging**:
1. Check Levels panel status (should show "●" and state "Created")
2. Check logs for risk check failures
3. Verify indicators are available (check indicator names match)
4. Monitor market data flow (check orderbook updates)
5. Check circuit breaker status in logs

### Level Posting Too Many Orders

**Possible Causes**:
- `max_orders` not set or too high
- `max_reposts` not set or too high
- Auto-repost conditions not configured
- Multiple levels posting for same pair

**Solutions**:
1. Reduce `max_orders` in level risk configuration
2. Set `max_reposts` to limit reposting
3. Add repost conditions (e.g., `PositionLimitReached`)
4. Review level configurations for duplicate levels

### Level Not Reposting

**Possible Causes**:
- `auto_repost` or `auto_repost_on_cancel` is `false`
- `max_reposts` reached
- Repost condition met (e.g., `DailyLossLimitReached`)
- Level disabled

**Debugging**:
1. Check lifecycle configuration (`auto_repost`, `auto_repost_on_cancel`)
2. Check repost count vs. `max_reposts`
3. Check repost conditions in configuration
4. Check level enabled status

### Indicator Not Found

**Possible Causes**:
- Indicator name mismatch (check exact name)
- Indicator not configured in Indicator Engine
- Pair-specific indicator not resolved correctly

**Solutions**:
1. Verify indicator name matches exactly (case-sensitive)
2. Check Indicator Engine configuration
3. Use pair-agnostic indicator names (e.g., "sma_20") for automatic resolution
4. Check logs for indicator resolution details

### Price/Quantity Resolution Errors

**Possible Causes**:
- Invalid offset calculation
- Balance insufficient
- Exchange rules violation (tick size, step size, min order size)

**Debugging**:
1. Check resolved price/quantity in logs
2. Verify balance is sufficient
3. Check exchange rules (tick size, step size, min order size)
4. Review offset calculations

## Advanced Configuration

### Indicator Integration

Levels can use any indicator from the Indicator Engine:

- **Technical Indicators**: SMA, EMA, RSI, MACD, Bollinger Bands, etc.
- **Custom Indicators**: Any custom indicator configured in the Indicator Engine
- **Pair Resolution**: Pair-agnostic indicators (e.g., "sma_20") are automatically resolved to pair-specific (e.g., "sma_20_ETHUSDC")

**Example**:
```json
"price": {
  "Indicator": {
    "indicator_name": "sma_20",
    "offset_type": "PercentBelow",
    "offset_value": "0.5"
  }
}
```

### Multiple Levels Per Pair

You can configure multiple levels for the same trading pair:

```json
"levels": [
  {
    "id": "buy_level_1",
    "side": "Buy",
    "price": { "Indicator": { "indicator_name": "sma_20", "offset_type": "PercentBelow", "offset_value": "0.5" } },
    ...
  },
  {
    "id": "buy_level_2",
    "side": "Buy",
    "price": { "Indicator": { "indicator_name": "sma_20", "offset_type": "PercentBelow", "offset_value": "1.0" } },
    ...
  }
]
```

This creates a ladder of buy orders at different price levels.

### Level Groups

Use `group` to organize levels:

```json
{
  "id": "buy_level_1",
  "group": "main_strategy",
  ...
}
```

Groups can be used for:
- Filtering levels in the TUI
- Batch enable/disable operations
- Organizing related levels

### Persistence

Levels are persisted to MongoDB (if configured):

- **Save**: Levels are saved automatically when configuration changes
- **Load**: Levels are loaded from MongoDB on startup
- **Expansion**: Pair-agnostic level configs are expanded to create instances for each pair

## See Also

- [Trading Engine Architecture](../architecture/03-trading-engine.md)
- [Strategy Guide](./STRATEGY_GUIDE.md)
- [TUI CLI Architecture](../architecture/09-tui-cli.md)
- [Main README](../README.md)

