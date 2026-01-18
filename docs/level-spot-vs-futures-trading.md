# Level-Based Trading: Spot vs Futures

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

This document explains how level-based trading works in the trading system for both **Spot** and **Futures** instrument types. It covers the data structures, fields, and examples used in each system, showing what information is tracked, displayed, and utilized during trading operations.

## Key Concepts

### Spot Trading
- **Ownership**: You own the actual assets (BTC, ETH, etc.)
- **Position Representation**: Derived from balances (if you have 1 BTC balance, you have a 1 BTC position)
- **Risk**: Lower risk - no leverage, no liquidation
- **Use Case**: Buying/selling actual assets, simple trading strategies

### Futures Trading
- **Ownership**: You trade derivative contracts (don't own the underlying asset)
- **Position Representation**: Explicit positions tracked separately from collateral
- **Risk**: Higher risk - leverage available, liquidation risk
- **Use Case**: Leveraged trading, hedging, directional trading without owning assets

## Data Structures

### Spot Trading Data

#### Balance (`Balance`)
**Purpose**: Tracks available assets in your spot account.

**Fields Used**:
```rust
pub struct Balance {
    pub symbol: Symbol,        // Currency: "BTC", "USDT", "ETH", etc.
    pub free: Decimal,         // Available for new orders
    pub used: Decimal,         // Locked in open orders
    pub total: Decimal,        // Total balance (free + used)
}
```

**How It's Used in Levels**:
- **Quantity Resolution**: When resolving level quantity, the system checks the `free` balance of the quote currency (for buy orders) or base currency (for sell orders)
- **Risk Checks**: Validates sufficient balance before placing orders
- **Position Tracking**: Position is derived from balance (e.g., 0.5 BTC balance = 0.5 BTC position)

**Example**:
```rust
// Spot balance for BTC/USDT trading
Balance {
    symbol: "USDT".into(),
    free: Decimal::from(10000),   // $10,000 available
    used: Decimal::from(5000),     // $5,000 in open orders
    total: Decimal::from(15000),   // $15,000 total
}

Balance {
    symbol: "BTC".into(),
    free: Decimal::from_str("0.5")?,  // 0.5 BTC available
    used: Decimal::ZERO,               // No BTC in orders
    total: Decimal::from_str("0.5")?,  // 0.5 BTC total
}
```

#### Position (`Position`) - Spot
**Purpose**: Represents a spot position (derived from balance).

**Fields Used**:
```rust
pub struct Position {
    pub instrument: Instrument,      // { type: Spot, pair: BTC/USDT }
    pub quantity: Decimal,           // Position size (positive = long)
    pub avg_price: Decimal,          // Average entry price
    pub current_price: Decimal,      // Current market price
    pub unrealized_pnl: Decimal,     // (current_price - avg_price) * quantity
    pub last_update: u64,            // Last update timestamp
    
    // Futures fields are None for spot
    pub mark_price: None,
    pub leverage: None,
    pub liquidation_price: None,
    // ... other futures fields are None
}
```

**How It's Used in Levels**:
- **Position Tracking**: System calculates position from balance changes
- **P&L Calculation**: `unrealized_pnl = (current_price - avg_price) * quantity`
- **Risk Management**: Monitors position size against risk limits

**Example**:
```rust
// Spot position (derived from 0.5 BTC balance)
Position {
    instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
    quantity: Decimal::from_str("0.5")?,      // 0.5 BTC position
    avg_price: Decimal::from(50000),          // Bought at $50,000
    current_price: Decimal::from(51000),      // Current price $51,000
    unrealized_pnl: Decimal::from(500),       // +$500 profit
    last_update: 1234567890,
    // All futures fields are None
}
```

#### Order (`Order`) - Spot
**Purpose**: Represents a spot trading order.

**Fields Used**:
```rust
pub struct Order {
    pub order_id: OrderId,
    pub instrument: Instrument,      // { type: Spot, pair: BTC/USDT }
    pub side: Side,                  // Buy or Sell
    pub price: Decimal,              // Order price
    pub amount: Decimal,             // Order quantity
    pub status: OrderStatus,         // New, PartiallyFilled, Filled, Canceled
    pub filled_amount: Decimal,      // How much has been filled
    pub internal_created_at: u64,
    pub exchange_created_at: Option<u64>,
}
```

**How It's Used in Levels**:
- **Order Tracking**: Each level tracks its `active_orders` list
- **State Management**: Level state changes based on order status (Posted → Filled/Cancelled)
- **Balance Locking**: When order is placed, balance is locked (`used` increases)

### Futures Trading Data

#### Futures Balance (`FuturesBalance`)
**Purpose**: Tracks collateral in your futures account (separate from positions).

**Fields Used**:
```rust
pub struct FuturesBalance {
    pub asset: Symbol,                    // Collateral asset: "USDT"
    pub available_balance: Decimal,       // Available for new positions
    pub wallet_balance: Decimal,          // Total wallet balance
    pub cross_wallet_balance: Decimal,    // Cross margin wallet balance
    pub cross_unrealized_pnl: Decimal,    // Cross margin unrealized P&L
    pub available_withdraw: Decimal,      // Available for withdrawal
    pub last_update: u64,
}
```

**How It's Used in Levels**:
- **Margin Calculation**: Determines how much margin is available for new positions
- **Quantity Resolution**: For futures, quantity resolution considers available margin and leverage
- **Risk Checks**: Validates sufficient margin before placing orders

**Example**:
```rust
// Futures collateral balance
FuturesBalance {
    asset: "USDT".into(),
    available_balance: Decimal::from(5000),    // $5,000 available for new positions
    wallet_balance: Decimal::from(10000),       // $10,000 total
    cross_wallet_balance: Decimal::from(10000),
    cross_unrealized_pnl: Decimal::from(1000),  // +$1,000 unrealized P&L
    available_withdraw: Decimal::from(5000),
    last_update: 1234567890,
}
```

#### Position (`Position`) - Futures
**Purpose**: Represents an explicit futures position (not derived from balance).

**Fields Used**:
```rust
pub struct Position {
    pub instrument: Instrument,              // { type: Futures, pair: BTC/USDT }
    pub quantity: Decimal,                   // Position size (positive = long, negative = short)
    pub avg_price: Decimal,                  // Average entry price
    pub current_price: Decimal,              // Last trade price
    pub unrealized_pnl: Decimal,             // Unrealized profit/loss
    pub last_update: u64,
    
    // Futures-specific fields (all populated)
    pub mark_price: Some(Decimal),           // Mark price for P&L calculation
    pub index_price: Some(Decimal),          // Index price for funding
    pub realized_pnl: Some(Decimal),         // Cumulative realized P&L
    pub initial_margin: Some(Decimal),       // Initial margin used
    pub maintenance_margin: Some(Decimal),   // Maintenance margin required
    pub leverage: Some(u32),                 // Leverage used (e.g., 10 = 10x)
    pub position_mode: Some(PositionMode),  // OneWay or Hedge
    pub margin_type: Some(MarginType),      // Isolated or Cross
    pub liquidation_price: Some(Decimal),   // Estimated liquidation price
}
```

**How It's Used in Levels**:
- **Position Tracking**: Explicit position tracking (can have position without owning asset)
- **P&L Calculation**: Uses `mark_price` for accurate P&L: `(mark_price - avg_price) * quantity`
- **Liquidation Risk**: Monitors `liquidation_price` and `maintenance_margin`
- **Margin Management**: Tracks `initial_margin` and `maintenance_margin` usage

**Example**:
```rust
// Futures position (1 BTC long with 10x leverage)
Position {
    instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
    quantity: Decimal::from(1),              // 1 BTC long
    avg_price: Decimal::from(50000),         // Entered at $50,000
    current_price: Decimal::from(51000),     // Last trade $51,000
    unrealized_pnl: Decimal::from(1000),    // +$1,000 unrealized P&L
    last_update: 1234567890,
    
    // Futures-specific fields
    mark_price: Some(Decimal::from(51000)),      // Mark price $51,000
    index_price: Some(Decimal::from(50950)),     // Index price $50,950
    realized_pnl: Some(Decimal::ZERO),            // No realized P&L yet
    initial_margin: Some(Decimal::from(5000)),   // $5,000 margin used (10x leverage)
    maintenance_margin: Some(Decimal::from(2500)), // $2,500 maintenance margin
    leverage: Some(10),                           // 10x leverage
    position_mode: Some(PositionMode::OneWay),   // One-way mode
    margin_type: Some(MarginType::Isolated),      // Isolated margin
    liquidation_price: Some(Decimal::from(45000)), // Liquidation at $45,000
}
```

#### Leverage Info (`LeverageInfo`)
**Purpose**: Tracks leverage settings per trading pair.

**Fields Used**:
```rust
pub struct LeverageInfo {
    pub pair: Pair,              // Trading pair
    pub leverage: u32,            // Current leverage (e.g., 10 = 10x)
    pub max_leverage: u32,        // Maximum allowed leverage
    pub last_update: u64,
}
```

**How It's Used in Levels**:
- **Quantity Calculation**: When resolving futures level quantity, considers leverage
- **Margin Requirements**: Calculates required margin: `notional_value / leverage`
- **Risk Limits**: Enforces maximum leverage per pair

**Example**:
```rust
LeverageInfo {
    pair: Pair::new("BTC", "USDT"),
    leverage: 10,                 // 10x leverage
    max_leverage: 125,            // Can go up to 125x
    last_update: 1234567890,
}
```

#### Funding Rate (`FundingRate`)
**Purpose**: Tracks funding rate information for futures contracts.

**Fields Used**:
```rust
pub struct FundingRate {
    pub pair: Pair,                    // Trading pair
    pub rate: Decimal,                  // Funding rate (e.g., 0.0001 = 0.01%)
    pub next_funding_time: u64,         // Next funding payment time
    pub last_funding_time: u64,         // Last funding payment time
    pub mark_price: Decimal,            // Mark price at funding
    pub index_price: Decimal,           // Index price at funding
}
```

**How It's Used in Levels**:
- **Strategy Decisions**: High funding rates can influence level placement
- **P&L Adjustments**: Funding payments affect total P&L over time

## Level Configuration

Levels are configured identically for both spot and futures, but the system behavior differs based on the `instrument_type` in the bot configuration.

### Level Config Structure
```json
{
  "id": "buy_level_1",
  "side": "Buy",
  "order_type": "Limit",
  "quantity": {
    "type": "Fixed",
    "value": "0.001"
  },
  "price": {
    "type": "Fixed",
    "value": "50000"
  },
  "risk": {
    "max_position": "0.01",
    "daily_loss_limit": "100"
  },
  "lifecycle": {
    "repost_on_fill": true,
    "repost_on_cancel": true
  },
  "enabled": true
}
```

**Key Point**: The same level configuration works for both spot and futures. The difference is in:
1. **Bot Configuration**: Sets `instrument_type: "Spot"` or `instrument_type: "Futures"`
2. **Data Resolution**: System uses different data sources (balances vs positions, margin vs balance)

## Detailed Examples

### Example 1: Level Execution - Buying BTC

#### Spot: Level Places Buy Order for 0.001 BTC at $50,000

**Step 1: Level Resolution**
```rust
// System resolves level quantity and price
let resolved_level = level_executor.resolve_level(
    &level,
    &indicators,
    Some(&orderbook),
    &balances
).await?;

// Resolved values:
// - resolved_price: 50000.0 (from level config)
// - resolved_quantity: 0.001 (from level config)
// - resolved_order_type: Limit
```

**Step 2: Balance Check (Spot)**
```rust
// System checks USDT balance (quote currency for buy order)
let usdt_balance = balances.iter()
    .find(|b| b.symbol.as_ref() == "USDT")
    .map(|b| b.free)
    .unwrap_or(Decimal::ZERO);

// Required: 0.001 * 50000 = $50
if usdt_balance < Decimal::from(50) {
    return Err("Insufficient balance");
}

// Balance data used:
// Balance { symbol: "USDT", free: 10000.0, used: 0.0, total: 10000.0 }
```

**Step 3: Order Placement**
```rust
// Create order request
let order_request = OrderRequest {
    instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
    side: Side::Buy,
    order_type: OrderType::Limit,
    price: Decimal::from(50000),
    amount: Decimal::from_str("0.001")?,
    timestamp: Some(current_timestamp()),
    exchange_params: HashMap::new(),
};

// Place order via bulk operation
let bulk_request = BulkOrderRequest {
    requests: vec![order_request],
};
let response = trading_client.bulk_open_orders(bulk_request).await?;
```

**Step 4: Order Created**
```rust
// Order data structure
Order {
    order_id: OrderId::new("spot_order_123"),
    instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
    side: Side::Buy,
    price: Decimal::from(50000),
    amount: Decimal::from_str("0.001")?,
    status: OrderStatus::New,
    filled_amount: Decimal::ZERO,
    internal_created_at: 1234567890,
    exchange_created_at: Some(1234567891),
}
```

**Step 5: Balance Updated (Order Locked)**
```rust
// Balance updated: USDT locked in order
// Before: Balance { symbol: "USDT", free: 10000.0, used: 0.0, total: 10000.0 }
// After:  Balance { symbol: "USDT", free: 9950.0, used: 50.0, total: 10000.0 }
```

**Step 6: Order Filled**
```rust
// Order status updated via WebSocket
Order {
    order_id: OrderId::new("spot_order_123"),
    status: OrderStatus::Filled,
    filled_amount: Decimal::from_str("0.001")?,
    // ... other fields unchanged
}

// Balance updated: USDT decreased, BTC increased
// USDT: Balance { symbol: "USDT", free: 9950.0, used: 0.0, total: 9950.0 }
// BTC:  Balance { symbol: "BTC", free: 0.001, used: 0.0, total: 0.001 }
```

**Step 7: Position Derived**
```rust
// Position calculated from balance
Position {
    instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
    quantity: Decimal::from_str("0.001")?,      // From BTC balance
    avg_price: Decimal::from(50000),            // From filled order
    current_price: Decimal::from(50000),        // Current market price
    unrealized_pnl: Decimal::ZERO,              // No P&L yet
    last_update: 1234567892,
    // All futures fields are None
}
```

#### Futures: Level Places Buy Order for 0.1 BTC at $50,000 with 10x Leverage

**Step 1: Level Resolution**
```rust
// Same resolution process, but considers leverage
let resolved_level = level_executor.resolve_level(
    &level,
    &indicators,
    Some(&orderbook),
    &futures_balances  // Note: futures_balances, not balances
).await?;

// Resolved values:
// - resolved_price: 50000.0
// - resolved_quantity: 0.1 (from level config)
// - resolved_order_type: Limit
```

**Step 2: Margin Check (Futures)**
```rust
// System checks available margin
let usdt_balance = futures_balances.iter()
    .find(|b| b.asset.as_ref() == "USDT")
    .map(|b| b.available_balance)
    .unwrap_or(Decimal::ZERO);

// Get leverage for pair
let leverage_info = trading_client.get_leverage(&Pair::new("BTC", "USDT")).await?;
// LeverageInfo { leverage: 10, max_leverage: 125, ... }

// Required margin: (0.1 * 50000) / 10 = $500
let required_margin = (resolved_quantity * resolved_price) / Decimal::from(leverage_info.leverage);

if usdt_balance < required_margin {
    return Err("Insufficient margin");
}

// Futures balance data used:
// FuturesBalance {
//     asset: "USDT",
//     available_balance: 5000.0,      // $5,000 available
//     wallet_balance: 10000.0,         // $10,000 total
//     ...
// }
```

**Step 3: Order Placement**
```rust
// Create order request (same structure, but instrument_type is Futures)
let order_request = OrderRequest {
    instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
    side: Side::Buy,
    order_type: OrderType::Limit,
    price: Decimal::from(50000),
    amount: Decimal::from_str("0.1")?,
    timestamp: Some(current_timestamp()),
    exchange_params: HashMap::new(),
};

// Place order via bulk operation
let bulk_request = BulkOrderRequest {
    requests: vec![order_request],
};
let response = trading_client.bulk_open_orders(bulk_request).await?;
```

**Step 4: Order Created**
```rust
// Order data structure (same as spot, but instrument_type is Futures)
Order {
    order_id: OrderId::new("futures_order_456"),
    instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
    side: Side::Buy,
    price: Decimal::from(50000),
    amount: Decimal::from_str("0.1")?,
    status: OrderStatus::New,
    filled_amount: Decimal::ZERO,
    internal_created_at: 1234567890,
    exchange_created_at: Some(1234567891),
}
```

**Step 5: Margin Updated (Order Locked)**
```rust
// Futures balance updated: margin locked
// Before: FuturesBalance { available_balance: 5000.0, wallet_balance: 10000.0, ... }
// After:  FuturesBalance { available_balance: 4500.0, wallet_balance: 10000.0, ... }
// Note: wallet_balance unchanged until order fills
```

**Step 6: Order Filled**
```rust
// Order status updated via WebSocket
Order {
    order_id: OrderId::new("futures_order_456"),
    status: OrderStatus::Filled,
    filled_amount: Decimal::from_str("0.1")?,
    // ... other fields unchanged
}

// Position created (explicit, not derived from balance)
Position {
    instrument: Instrument::new(InstrumentType::Futures, Pair::new("BTC", "USDT")),
    quantity: Decimal::from_str("0.1")?,       // 0.1 BTC long
    avg_price: Decimal::from(50000),           // Entry price
    current_price: Decimal::from(50000),       // Last trade price
    unrealized_pnl: Decimal::ZERO,             // No P&L yet
    last_update: 1234567892,
    
    // Futures-specific fields populated
    mark_price: Some(Decimal::from(50000)),         // Mark price
    index_price: Some(Decimal::from(49950)),        // Index price
    realized_pnl: Some(Decimal::ZERO),              // No realized P&L
    initial_margin: Some(Decimal::from(500)),        // $500 margin used
    maintenance_margin: Some(Decimal::from(250)),    // $250 maintenance margin
    leverage: Some(10),                              // 10x leverage
    position_mode: Some(PositionMode::OneWay),       // One-way mode
    margin_type: Some(MarginType::Isolated),         // Isolated margin
    liquidation_price: Some(Decimal::from(45000)),   // Liquidation price
}

// Futures balance updated: margin used
// FuturesBalance {
//     available_balance: 4500.0,      // $500 margin used
//     wallet_balance: 10000.0,         // Total unchanged
//     ...
// }
```

**Step 7: Mark Price Updates (Continuous)**
```rust
// Mark price updates continuously via WebSocket
// When mark price changes to $51,000:

Position {
    // ... previous fields ...
    mark_price: Some(Decimal::from(51000)),         // Updated mark price
    current_price: Decimal::from(51000),             // Updated current price
    unrealized_pnl: Decimal::from(100),              // (51000 - 50000) * 0.1 = $100
    // ... other fields ...
}

// Futures balance updated with unrealized P&L
// FuturesBalance {
//     available_balance: 4500.0,
//     wallet_balance: 10100.0,         // +$100 unrealized P&L
//     cross_unrealized_pnl: Decimal::from(100),
//     ...
// }
```

### Example 2: Level State Management

#### Spot: Level Lifecycle

**State Transitions**:
1. **Created** → Level created, no orders
2. **Posted** → Order placed, waiting for fill
3. **Filled** → Order filled, balance updated, position derived
4. **Cancelled** → Order cancelled, balance unlocked
5. **Paused** → Level disabled manually

**Data Tracked**:
```rust
Level {
    config: LevelConfig { ... },
    pair: Pair::new("BTC", "USDT"),
    state: LevelState::Posted,                    // Current state
    active_orders: vec![OrderId::new("order_123")], // Active order IDs
    repost_count: 0,                              // Number of reposts
    last_posted_at: Some(1234567890),            // Last post timestamp
    last_filled_at: None,                         // Last fill timestamp
}
```

**Balance Tracking**:
- Before order: `Balance { symbol: "USDT", free: 10000.0, used: 0.0 }`
- Order placed: `Balance { symbol: "USDT", free: 9950.0, used: 50.0 }`
- Order filled: `Balance { symbol: "USDT", free: 9950.0, used: 0.0 }`
- Position: `Position { quantity: 0.001, ... }` (derived from BTC balance)

#### Futures: Level Lifecycle

**State Transitions**:
1. **Created** → Level created, no orders
2. **Posted** → Order placed, margin locked
3. **Filled** → Order filled, position created
4. **Cancelled** → Order cancelled, margin unlocked
5. **Paused** → Level disabled manually

**Data Tracked**:
```rust
Level {
    config: LevelConfig { ... },
    pair: Pair::new("BTC", "USDT"),
    state: LevelState::Posted,                    // Current state
    active_orders: vec![OrderId::new("order_456")], // Active order IDs
    repost_count: 0,                              // Number of reposts
    last_posted_at: Some(1234567890),            // Last post timestamp
    last_filled_at: None,                         // Last fill timestamp
}
```

**Position Tracking**:
- Before order: No position, `FuturesBalance { available_balance: 5000.0, ... }`
- Order placed: Margin locked, `FuturesBalance { available_balance: 4500.0, ... }`
- Order filled: Position created, `Position { quantity: 0.1, leverage: 10, ... }`
- Mark price update: `Position { unrealized_pnl: 100, ... }` (continuous updates)

### Example 3: Risk Management

#### Spot: Risk Checks

**Data Used for Risk Checks**:
```rust
// 1. Balance check
let balance = balances.iter()
    .find(|b| b.symbol.as_ref() == "USDT")
    .map(|b| b.free)
    .unwrap_or(Decimal::ZERO);

// 2. Position check (derived from balance)
let btc_position = balances.iter()
    .find(|b| b.symbol.as_ref() == "BTC")
    .map(|b| b.total)
    .unwrap_or(Decimal::ZERO);

// 3. Risk limit check
if btc_position > level.config.risk.max_position {
    return Err("Max position limit exceeded");
}

// 4. Daily loss limit check
let daily_pnl = calculate_daily_pnl(&trades, &current_prices);
if daily_pnl < -level.config.risk.daily_loss_limit {
    return Err("Daily loss limit exceeded");
}
```

**Risk Data Structures**:
- `Balance` - Used for balance checks
- `Position` (derived) - Used for position size limits
- `Trade` - Used for daily P&L calculation

#### Futures: Risk Checks

**Data Used for Risk Checks**:
```rust
// 1. Margin check
let available_margin = futures_balances.iter()
    .find(|b| b.asset.as_ref() == "USDT")
    .map(|b| b.available_balance)
    .unwrap_or(Decimal::ZERO);

// 2. Position check (explicit)
let btc_position = positions.iter()
    .find(|p| p.instrument.pair == Pair::new("BTC", "USDT"))
    .map(|p| p.quantity)
    .unwrap_or(Decimal::ZERO);

// 3. Leverage check
let leverage_info = trading_client.get_leverage(&Pair::new("BTC", "USDT")).await?;
if leverage_info.leverage > max_allowed_leverage {
    return Err("Leverage too high");
}

// 4. Liquidation risk check
if let Some(position) = positions.get(&Pair::new("BTC", "USDT")) {
    if let Some(liquidation_price) = position.liquidation_price {
        let current_price = position.mark_price.unwrap_or(position.current_price);
        let distance_to_liquidation = (current_price - liquidation_price).abs() / current_price;
        if distance_to_liquidation < Decimal::from_str("0.05")? {  // 5% threshold
            return Err("Too close to liquidation");
        }
    }
}

// 5. Margin ratio check
if let Some(position) = positions.get(&Pair::new("BTC", "USDT")) {
    if let (Some(initial_margin), Some(maintenance_margin)) = 
        (position.initial_margin, position.maintenance_margin) {
        let margin_ratio = maintenance_margin / initial_margin;
        if margin_ratio > Decimal::from_str("0.8")? {  // 80% threshold
            return Err("Margin ratio too high");
        }
    }
}

// 6. Daily loss limit check
let daily_pnl = calculate_daily_pnl(&trades, &positions, &current_prices);
if daily_pnl < -level.config.risk.daily_loss_limit {
    return Err("Daily loss limit exceeded");
}
```

**Risk Data Structures**:
- `FuturesBalance` - Used for margin availability checks
- `Position` (explicit) - Used for position size, liquidation risk
- `LeverageInfo` - Used for leverage limits
- `Trade` - Used for daily P&L calculation

### Example 4: TUI Display

#### Spot Monitor Display

**Data Displayed**:
```
┌─ Balances ─────────────────────────┐
│ USDT: 10,000.00 (free: 9,950.00)   │
│ BTC:  0.00100 (free: 0.00100)      │
└─────────────────────────────────────┘

┌─ Positions ────────────────────────┐
│ BTC/USDT: +0.00100 @ $50,000       │
│ P&L: +$0.00                         │
└─────────────────────────────────────┘

┌─ Orders ────────────────────────────┐
│ Order #123: BUY 0.001 BTC @ $50,000│
│ Status: Filled                      │
└─────────────────────────────────────┘
```

**Data Sources**:
- `balances: Vec<Balance>` - From `MonitoringData.balances`
- `positions: Vec<Position>` - Derived from balances or from `MonitoringData.positions`
- `orders: Vec<Order>` - From `MonitoringData.orders`

#### Futures Monitor Display

**Data Displayed**:
```
┌─ Margin Info ───────────────────────┐
│ Collateral: $10,000.00              │
│ Used: $500.00                       │
│ Available: $9,500.00                │
│ Total P&L: +$100.00                 │
│ Positions: 1                        │
└─────────────────────────────────────┘

┌─ Positions ────────────────────────┐
│ BTC/USDT: LONG 0.1 @ $50,000        │
│ Mark: $51,000 | Entry: $50,000      │
│ P&L: +$100.00 (unrealized)          │
│ Leverage: 10x | Margin: $500.00      │
│ Liquidation: $45,000.00             │
└─────────────────────────────────────┘

┌─ Orders ────────────────────────────┐
│ Order #456: BUY 0.1 BTC @ $50,000  │
│ Status: Filled                      │
└─────────────────────────────────────┘
```

**Data Sources**:
- `futures_balances: Vec<FuturesBalance>` - From `MonitoringData` (futures-specific)
- `positions: Vec<Position>` - From `MonitoringData.positions` (with futures fields populated)
- `orders: Vec<Order>` - From `MonitoringData.orders`
- `leverage_info: HashMap<Pair, LeverageInfo>` - From exchange state

## Summary: Data Utilization by System

### Spot Trading System

**Primary Data Structures**:
1. **`Balance`** - Core data structure
   - Tracks asset ownership
   - Used for quantity resolution
   - Used for risk checks
   - Used for position derivation

2. **`Position`** (derived) - Calculated from balances
   - `quantity` = balance amount
   - `avg_price` = from trade history
   - `unrealized_pnl` = (current_price - avg_price) * quantity
   - Futures fields: All `None`

3. **`Order`** - Standard order structure
   - `instrument.instrument_type = Spot`
   - No futures-specific fields

**Key Differences**:
- Positions are **derived** from balances
- No leverage, margin, or liquidation concepts
- Simpler risk management (balance checks only)

### Futures Trading System

**Primary Data Structures**:
1. **`FuturesBalance`** - Collateral tracking
   - Tracks margin availability
   - Used for margin checks
   - Separate from positions

2. **`Position`** (explicit) - Explicit position tracking
   - `quantity` = position size (can be long or short)
   - `mark_price` = used for P&L calculation
   - `leverage` = leverage used
   - `liquidation_price` = liquidation risk
   - `initial_margin` / `maintenance_margin` = margin usage
   - All futures fields populated

3. **`LeverageInfo`** - Leverage settings
   - Used for margin calculations
   - Used for risk limits

4. **`FundingRate`** - Funding rate information
   - Used for strategy decisions
   - Affects total P&L

5. **`Order`** - Standard order structure
   - `instrument.instrument_type = Futures`
   - Same structure as spot, but processed differently

**Key Differences**:
- Positions are **explicit** (separate from collateral)
- Leverage, margin, and liquidation are core concepts
- More complex risk management (margin, liquidation, leverage checks)

## Configuration Examples

### Spot Configuration
```json
{
  "exchange_engine": {
    "exchange": "Binance",
    "config": {
      "pairs": [{"base": "BTC", "quote": "USDT"}]
    }
  },
  "instrument_type": "Spot"
}
```

### Futures Configuration
```json
{
  "exchange_engine": {
    "exchange": "Bybit",
    "config": {
      "pairs": [{"base": "BTC", "quote": "USDT"}]
    }
  },
  "instrument_type": "Futures"
}
```

**Note**: The same level configuration works for both. The system automatically uses the appropriate data structures based on `instrument_type`.

---

**Last Updated**: December 2024  
**Maintained By**: Development Team
