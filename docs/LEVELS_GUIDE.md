# Levels Guide

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Trading System supports level-based trading, which allows you to define trading levels that automatically place orders based on indicator values, orderbook prices, or fixed prices. Levels are exchange-agnostic and pair-agnostic templates that get expanded to create instances for each configured trading pair.

**Key Features**:
- **Indicator-based pricing**: Levels use `"price": "Binding"`; indicator refs and offsets live in `config/indicators.json` (`level_pricing`) or MongoDB
- **Flexible quantity configuration**: Fixed quantities, balance percentage, or notional values
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
      "price": "Binding",
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

### Binding price (indicator-based)

Levels that trade off indicators use a **binding** price in `config/levels.json`:

```json
"price": "Binding"
```

The level template **`id`** (e.g. `buy_level_1`) must match a row in **`config/indicators.json`** → **`level_pricing`** (or a MongoDB `level_pricing` document). Indicator definitions live in the same file under **`indicators`**.

Example `config/indicators.json`:

```json
{
  "version": 1,
  "indicators": [
    {
      "kind": "simple",
      "id": "sma_20_BTCUSDT",
      "name": "SMA",
      "pair": "BTCUSDT",
      "period": 20,
      "source": "close"
    }
  ],
  "level_pricing": [
    {
      "level_id": "buy_level_1",
      "indicator_ref": "sma_20",
      "offset_type": "PercentBelow",
      "offset_value": "0.5"
    }
  ]
}
```

**`level_pricing` fields**:
- **`level_id`** (string, required): Must match the level template `id` in `config/levels.json`
- **`indicator_ref`** (string, required): Pair-agnostic ref (e.g. `sma_20`) or full id (e.g. `sma_20_BTCUSDT`). Pair-agnostic refs resolve per pair when the level is expanded (e.g. `sma_20` → `sma_20_BTCUSDT`)
- **`offset_type`** (string, required): `Absolute`, `Percent`, `PercentBelow`, or `PercentAbove`
- **`offset_value`** (decimal string, required): Offset magnitude

**Offset types**:
- **`Absolute`**: `indicator_value + offset_value`
- **`Percent`**: percentage add/subtract on the indicator value
- **`PercentBelow`**: X% below (e.g. `indicator_value * (1 - offset_value/100)`)
- **`PercentAbove`**: X% above (e.g. `indicator_value * (1 + offset_value/100)`)

Generate or validate indicators config from `python/`:

```bash
cd python && uv sync
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json
uv run indicator-validate --config ../config/indicators.json
```

At startup the indicator engine loads **MongoDB first** (if enabled), then fills gaps from `config/indicators.json`. Duplicate indicator ids: Mongo wins.

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
   
   The TUI automatically detects the instrument type (Spot or Futures) and displays the appropriate monitor view.

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
- Press `T` in the Spot Monitor to toggle the **selected** level
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
4. **Indicators available**: Required indicators are registered and have values (if using `"price": "Binding"`)
5. **Orderbook available**: Orderbook data is available (if using orderbook-based price)

### Price Resolution

Price resolution happens in real-time:

1. **Binding lookup**: For `"price": "Binding"`, `level_pricing` is resolved by level template `id` and trading pair
2. **Indicator value**: The referenced indicator value is read from the indicator cache
3. **Offset calculation**: Offset is applied from `offset_type` and `offset_value`
4. **Tick size rounding**: Price is rounded to exchange tick size (e.g., 0.01 for ETH/USDC)
5. **Validation**: Price is validated against exchange rules (min/max price, price precision)

### Quantity Resolution

Quantity resolution happens in real-time:

1. **Quantity calculation**: Based on quantity configuration (Fixed, BalancePercent, or Notional)
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

### Indicator not found / binding missing

**Possible causes**:
- No `level_pricing` row for the level template `id`
- `indicator_ref` does not match any registered indicator for that pair
- `config/indicators.json` not loaded (Mongo empty and file missing or invalid)

**Solutions**:
1. Ensure `level_id` in `level_pricing` matches the level template `id` in `config/levels.json`
2. Run `uv run indicator-validate --config ../config/indicators.json` from `python/`
3. Confirm indicators exist for each pair (ids like `sma_20_BTCUSDT`) or use pair-agnostic refs (`sma_20`)
4. Check startup logs for indicator registration and Mongo vs file load order

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

### Indicator integration

1. Register indicators in `config/indicators.json` (or MongoDB `indicators` collection) — simple (`kind: "simple"`) or composite (`kind: "composite"`).
2. Set `"price": "Binding"` on the level template in `config/levels.json`.
3. Add a `level_pricing` entry keyed by that template's `id`.

Pair-agnostic `indicator_ref` values (e.g. `sma_20`) resolve to pair-specific indicator ids (e.g. `sma_20_BTCUSDT`) when levels are expanded per pair.

### Multiple Levels Per Pair

You can configure multiple levels for the same trading pair:

```json
"levels": [
  {
    "id": "buy_level_1",
    "side": "Buy",
    "price": "Binding",
    ...
  },
  {
    "id": "buy_level_2",
    "side": "Buy",
    "price": "Binding",
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

The file catalog is canonical. Mongo may mirror it and must not override it at restart. Catalog load rejects a venue/account/instrument scope mismatch. Terminal filled/cancelled exchange IDs are retired so restart does not restore `Posted`. Delete latches a durable `Deleting` instance (owned IDs kept) and writes the tombstone only after REST verification. `repost_count` and `last_filled_at` are persisted so restart does not reset max-repost and delay bounds.

## Paper / Bybit capability (levels)

| Capability | Paper | Bybit Demo | Bybit mainnet |
|---|---|---|---|
| Durable pair-specific identity + tombstones | Implemented | Implemented, not qualified | Not qualified |
| Persist-before-send + client-ID recovery | Implemented | Implemented, not qualified | Not qualified |
| Per-pair / per-level caps including pending + fills | Implemented | Implemented, not qualified | Not qualified |
| Disable/DeleteLevel owned-ID cancel + REST verify | Implemented | Implemented, not qualified | Not qualified |
| Shadow guard on levels mutation/shutdown | Implemented | n/a (Exchange mode) | n/a |
| Spot flatten | Unavailable (no pair-wide sale) | Unavailable | Unavailable |
| Futures flatten | Journaled reduce-only (strategies path) | Not qualified for levels | Not qualified |
| Per-level daily-loss | Hidden until PnL attribution | Hidden | Hidden |

**GO/NO-GO:** Paper library tests cover the gates above. **NO-GO** for Bybit Demo and live levels until an independent operator run on current `master` with no Demo/mainnet orders from this change. UI repo remains at `d2237ba` (DeleteLevel/gRPC is in `trading_system`; hide daily-loss in UI separately). k-paper remains at `25e1268`.

## See Also

- [Trading Engine Architecture](../architecture/03-trading-engine.md)
- [Strategy Guide](./STRATEGY_GUIDE.md)
- [TUI CLI Architecture](../architecture/09-tui-cli.md)
- [Main README](../README.md)

