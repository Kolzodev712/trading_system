# Level-Based Trading: Spot vs Futures

**Version**: 1.0  
**Last Updated**: December 2024

## Overview

This document provides a comprehensive guide to level-based trading in the trading system, covering both **Spot** and **Futures** instrument types. It explains real-world trading concepts, how they work in practice, all relevant terminology, and how these concepts are implemented in the system.

## Table of Contents

1. [Understanding Spot Trading](#understanding-spot-trading)
2. [Understanding Futures Trading](#understanding-futures-trading)
3. [Key Trading Terms Explained](#key-trading-terms-explained)
4. [Real-World Trading Scenarios](#real-world-trading-scenarios)
5. [System Implementation](#system-implementation)
6. [Data Structures and Usage](#data-structures-and-usage)
7. [Level Configuration Examples](#level-configuration-examples)

---

## Understanding Spot Trading

### What is Spot Trading?

**Spot trading** is the purchase or sale of financial instruments (like cryptocurrencies) for immediate delivery. When you buy BTC on the spot market, you're buying actual BTC that you own and can withdraw to your wallet.

### Real-World Example: Buying Coffee

Think of spot trading like buying coffee at a café:
- You pay $5 cash → You receive a cup of coffee
- You own the coffee → You can drink it, take it home, or give it away
- The transaction is complete → No ongoing obligations

**In Crypto Spot Trading:**
- You pay 10,000 USDT → You receive 0.2 BTC
- You own the BTC → You can withdraw it, hold it, or sell it later
- The transaction is complete → You have the asset in your account

### Characteristics of Spot Trading

1. **Direct Ownership**: You own the actual asset
   - If you buy 1 BTC, you have 1 BTC in your account
   - You can withdraw it to your personal wallet
   - You can use it for payments, staking, or other purposes

2. **No Leverage**: You can only trade with what you have
   - If you have $10,000, you can buy up to $10,000 worth of BTC
   - No borrowing or margin trading (unless using margin features)

3. **Simple Risk Profile**: 
   - Maximum loss: The amount you invested
   - If you buy BTC at $50,000 and it drops to $0, you lose $50,000
   - No liquidation risk (you can't lose more than you invested)

4. **Long-Only Positions**: 
   - You can only profit if the price goes up
   - To profit from price drops, you must sell first, then buy back later

### Common Spot Trading Strategies

1. **Buy and Hold (HODL)**: Buy assets and hold them long-term
2. **Dollar-Cost Averaging (DCA)**: Buy fixed amounts at regular intervals
3. **Grid Trading**: Place buy/sell orders at regular price intervals
4. **Arbitrage**: Buy on one exchange, sell on another for profit
5. **Market Making**: Provide liquidity by placing orders on both sides

### How Spot Trading Works in Our System

- **Balances**: Track actual assets you own (BTC, USDT, ETH, etc.)
- **Positions**: Derived from balances (if you have 0.5 BTC balance, you have a 0.5 BTC position)
- **Orders**: Buy/sell actual assets
- **Risk**: Simple balance checks (do you have enough USDT to buy?)

---

## Understanding Futures Trading

### What is Futures Trading?

**Futures trading** is trading derivative contracts that represent an agreement to buy or sell an asset at a predetermined price at a future date. You don't own the underlying asset - you're trading a contract that derives its value from the asset.

### Real-World Example: Buying a House Contract

Think of futures like a house purchase contract:
- You put down a $20,000 deposit (margin) → You get a contract to buy a $200,000 house
- You don't own the house yet → You own a contract
- If the house value increases to $250,000 → Your contract is worth more
- If the house value drops to $150,000 → You might lose your deposit (liquidation)
- You can close the contract anytime → Sell the contract for profit/loss

**In Crypto Futures Trading:**
- You put down $1,000 margin (10x leverage) → You get a contract for $10,000 worth of BTC
- You don't own BTC → You have a position (long or short)
- If BTC price increases → Your position gains value
- If BTC price drops too much → You get liquidated (lose your margin)
- You can close anytime → Close the position for profit/loss

### Characteristics of Futures Trading

1. **No Direct Ownership**: You trade contracts, not assets
   - You can't withdraw BTC from a futures position
   - You can only profit/loss from price movements
   - The contract settles in cash (USDT)

2. **Leverage Available**: Trade with borrowed capital
   - With $1,000 and 10x leverage, you can control $10,000 worth of BTC
   - Amplifies both profits and losses
   - Common leverage: 1x to 125x (depending on exchange)

3. **Long and Short Positions**:
   - **Long**: Profit when price goes up (like buying spot)
   - **Short**: Profit when price goes down (betting against the asset)
   - Can switch between long/short without owning the asset

4. **Liquidation Risk**: 
   - If price moves against you too much, you get liquidated
   - You lose your margin (the money you put up)
   - Liquidation price depends on leverage and margin

5. **Funding Rates**: 
   - Periodic fees paid/received based on position direction
   - Long positions pay shorts when funding rate is positive
   - Short positions pay longs when funding rate is negative
   - Typically every 8 hours

### Common Futures Trading Strategies

1. **Directional Trading**: Bet on price direction (long or short)
2. **Hedging**: Protect spot positions by taking opposite futures positions
3. **Arbitrage**: Exploit price differences between spot and futures
4. **Funding Rate Arbitrage**: Profit from funding rate differences
5. **Leverage Trading**: Use leverage to amplify returns (and risks)

### How Futures Trading Works in Our System

- **Futures Balance**: Tracks collateral (margin) you have available
- **Positions**: Explicit positions (long/short) tracked separately from balance
- **Orders**: Open/close positions (not buying/selling assets)
- **Risk**: Margin checks, liquidation monitoring, leverage limits

---

## Key Trading Terms Explained

### Spot Trading Terms

#### Balance
**Definition**: The actual amount of an asset you own in your account.

**Real-World Analogy**: Your bank account balance - the actual money you have.

**In System**: 
- `Balance { symbol: "BTC", free: 0.5, used: 0.0, total: 0.5 }`
- `free`: Available for new orders
- `used`: Locked in open orders
- `total`: Total balance (free + used)

**Example**: If you have 0.5 BTC and 10,000 USDT:
- You can buy up to 10,000 USDT worth of BTC
- You can sell up to 0.5 BTC
- Your position is simply your BTC balance (0.5 BTC)

#### Position (Spot)
**Definition**: Your exposure to an asset, derived from your balance.

**Real-World Analogy**: If you own 10 shares of Apple stock, your position is 10 shares long.

**In System**:
- Position is calculated from balance: `Position { quantity: 0.5 }` (from 0.5 BTC balance)
- `avg_price`: Average price you bought at
- `unrealized_pnl`: Current profit/loss: `(current_price - avg_price) * quantity`

**Example**: 
- You bought 0.5 BTC at $50,000
- Current price is $55,000
- Unrealized P&L: `(55,000 - 50,000) * 0.5 = $2,500 profit`

#### Order (Spot)
**Definition**: An instruction to buy or sell an asset at a specific price.

**Types**:
- **Market Order**: Execute immediately at current market price
- **Limit Order**: Execute only at specified price or better
- **Stop-Loss Order**: Sell if price drops below threshold (not native in spot, must be monitored)

**In System**: `Order { side: Buy, price: 50000, amount: 0.001, status: Filled }`

---

### Futures Trading Terms

#### Margin
**Definition**: The collateral you deposit to open and maintain a futures position.

**Real-World Analogy**: A security deposit when renting an apartment - you need it to enter, and you might lose it if you break the contract.

**Types**:
- **Initial Margin**: Required to open a position
- **Maintenance Margin**: Minimum required to keep position open
- **Available Margin**: Margin not currently used in positions

**Example**:
- You want to open a $10,000 BTC position with 10x leverage
- Required margin: $10,000 / 10 = $1,000
- If you have $5,000 available margin, you can open the position

**In System**: `FuturesBalance { available_balance: 5000.0, ... }`

#### Leverage
**Definition**: The multiplier that determines how much you can control with your margin.

**Real-World Analogy**: Like a mortgage - you put down 10% ($20,000) to control a $200,000 house (10x leverage).

**How It Works**:
- 1x leverage: Trade with your own money (like spot)
- 10x leverage: Control $10,000 with $1,000 margin
- 100x leverage: Control $100,000 with $1,000 margin (very risky!)

**Example**:
- You have $1,000 margin
- With 10x leverage, you can open a $10,000 position
- If price moves 10% in your favor: You make $1,000 (100% return on margin)
- If price moves 10% against you: You lose $1,000 (100% loss, liquidation risk)

**In System**: `LeverageInfo { leverage: 10, max_leverage: 125 }`

#### Position (Futures)
**Definition**: Your exposure to price movements, tracked separately from your balance.

**Types**:
- **Long Position**: Profit when price goes up (like buying spot)
- **Short Position**: Profit when price goes down (betting against the asset)

**Real-World Analogy**: 
- Long: You have a contract to buy a house at $200,000. If house value goes to $250,000, your contract is worth $50,000 more.
- Short: You have a contract to sell a house at $200,000. If house value drops to $150,000, your contract is worth $50,000 more.

**In System**:
- `Position { quantity: 1.0 }` = 1 BTC long
- `Position { quantity: -0.5 }` = 0.5 BTC short
- `mark_price`: Used for P&L calculation (more accurate than last trade price)
- `liquidation_price`: Price at which you get liquidated

**Example**:
- You open 1 BTC long at $50,000 with 10x leverage
- Margin used: $5,000
- If BTC goes to $55,000: Unrealized P&L = $5,000 (100% return on margin)
- If BTC drops to $45,000: Unrealized P&L = -$5,000 (liquidation risk)

#### Liquidation
**Definition**: When your position is automatically closed because you've lost too much margin.

**How It Works**:
- You open a position with margin
- If price moves against you, your unrealized loss increases
- When your margin can't cover the loss anymore, exchange liquidates you
- You lose your margin (the money you put up)

**Example**:
- You open 1 BTC long at $50,000 with 10x leverage ($5,000 margin)
- Liquidation price: ~$45,000 (depends on maintenance margin)
- If BTC drops to $45,000: You get liquidated, lose $5,000
- If BTC drops to $40,000: Still liquidated at $45,000 (you don't lose more)

**In System**: `Position { liquidation_price: Some(45000.0), ... }`

#### Funding Rate
**Definition**: A periodic fee paid between long and short positions to keep futures price aligned with spot price.

**How It Works**:
- Calculated every 8 hours based on price difference between futures and spot
- If futures price > spot price: Funding rate is positive
  - Long positions pay short positions
- If futures price < spot price: Funding rate is negative
  - Short positions pay long positions

**Example**:
- BTC spot price: $50,000
- BTC futures price: $50,500
- Funding rate: 0.01% (positive)
- You have 1 BTC long position
- Every 8 hours, you pay: `1 BTC * $50,000 * 0.01% = $5` to short positions

**In System**: `FundingRate { rate: 0.0001, next_funding_time: 1234567890 }`

#### Mark Price
**Definition**: A fair value price used for calculating unrealized P&L and liquidation, based on spot price and funding rate.

**Why It Exists**:
- Last trade price can be manipulated (whale orders)
- Mark price prevents unfair liquidations
- More accurate representation of true position value

**How It's Calculated**:
- Based on spot price (index price) and funding rate
- Updated frequently (every few seconds)
- Used for P&L calculation instead of last trade price

**In System**: `Position { mark_price: Some(51000.0), current_price: 51050.0 }`

#### Margin Types

**Isolated Margin**:
- Margin is isolated per position
- If one position gets liquidated, other positions are safe
- Lower risk, but less capital efficient

**Example**:
- Position 1: $1,000 margin, 1 BTC long
- Position 2: $1,000 margin, 0.5 ETH long
- If BTC position gets liquidated, ETH position is unaffected

**Cross Margin**:
- All margin is shared across all positions
- More capital efficient (can use margin from profitable positions)
- Higher risk (one bad position can affect all positions)

**Example**:
- Total margin: $5,000
- Position 1: Uses $2,000 margin, 1 BTC long
- Position 2: Uses $3,000 margin, 0.5 ETH long
- If BTC position loses $2,000, it affects available margin for ETH position

**In System**: `Position { margin_type: Some(MarginType::Isolated), ... }`

#### Position Mode

**One-Way Mode**:
- Can only have one position per pair (long OR short)
- Simpler, easier to manage
- Most common for retail traders

**Example**:
- You have 1 BTC long position
- To go short, you must close the long position first

**Hedge Mode**:
- Can have both long AND short positions simultaneously
- Used for hedging strategies
- More complex, requires more margin

**Example**:
- You have 1 BTC long position
- You can also have 0.5 BTC short position (hedging)
- Net position: 0.5 BTC long

**In System**: `Position { position_mode: Some(PositionMode::OneWay), ... }`

---

## Real-World Trading Scenarios

### Scenario 1: Buying Bitcoin for Long-Term Hold

**Goal**: Buy 0.5 BTC and hold it for potential price appreciation.

**Spot Trading Approach**:
1. You have $25,000 USDT in your account
2. Place a buy order for 0.5 BTC at $50,000
3. Order fills → You now own 0.5 BTC
4. You can withdraw it to your wallet
5. If BTC goes to $60,000: You have $30,000 worth (profit: $5,000)
6. If BTC drops to $40,000: You have $20,000 worth (loss: $5,000)
7. **Maximum loss**: $25,000 (if BTC goes to $0)

**Futures Trading Approach** (Not Recommended for Long-Term Hold):
1. You have $2,500 margin (10x leverage)
2. Open 0.5 BTC long position at $50,000
3. You don't own BTC, just have a position
4. If BTC goes to $60,000: Unrealized P&L = $5,000 (200% return on margin!)
5. If BTC drops to $40,000: Unrealized P&L = -$5,000 (liquidation risk!)
6. **Maximum loss**: $2,500 (if liquidated), but you can't hold long-term (funding fees)

**Why Spot is Better for This**: You actually own the asset, no funding fees, no liquidation risk.

### Scenario 2: Short-Term Price Speculation

**Goal**: Profit from BTC price dropping from $50,000 to $45,000.

**Spot Trading Approach** (Limited):
1. You must first own BTC (buy at $50,000)
2. Then sell it (hoping to buy back lower)
3. If price drops to $45,000: You can buy back, profit = $5,000 per BTC
4. **Problem**: You need to own BTC first (requires capital)

**Futures Trading Approach** (Better for This):
1. You have $1,000 margin (10x leverage)
2. Open 1 BTC short position at $50,000
3. You don't need to own BTC
4. If price drops to $45,000: Unrealized P&L = $5,000 (500% return on margin!)
5. Close position → Realize profit
6. **Advantage**: Can profit from price drops without owning the asset

**Why Futures is Better for This**: Can go short easily, leverage amplifies returns.

### Scenario 3: Hedging a Spot Position

**Goal**: You own 1 BTC in spot, want to protect against price drops.

**The Problem**:
- You own 1 BTC bought at $50,000
- Current price: $55,000 (unrealized profit: $5,000)
- You're worried price might drop back to $50,000

**The Solution (Futures Hedge)**:
1. Open 1 BTC short position in futures at $55,000
2. If price drops to $50,000:
   - Spot position: Loses $5,000
   - Futures short: Gains $5,000
   - Net: Break even (protected your profit)
3. If price goes to $60,000:
   - Spot position: Gains $5,000
   - Futures short: Loses $5,000
   - Net: Break even (but you still have the BTC)

**Why This Works**: Futures position offsets spot position losses.

### Scenario 4: Grid Trading Strategy

**Goal**: Profit from price volatility by placing orders at regular intervals.

**Spot Grid Trading**:
1. Buy orders at: $49,000, $48,000, $47,000
2. Sell orders at: $51,000, $52,000, $53,000
3. As price bounces, orders fill and you profit from the spread
4. **Requirement**: You need actual BTC and USDT to place orders

**Futures Grid Trading**:
1. Same grid structure
2. Can use leverage to control larger positions with less capital
3. **Risk**: Funding fees eat into profits, liquidation risk if price moves too far

**In Our System**: Levels can implement grid trading with indicator-based price resolution.

---

## System Implementation

### How Spot Trading Works in the System

**Data Flow**:
1. **Exchange Engine** receives WebSocket updates → Updates `Balance`
2. **Trading Engine** subscribes to balance updates → Derives `Position` from balance
3. **Level Executor** resolves level → Checks balance → Places order
4. **Order fills** → Balance updates → Position recalculated

**Key Components**:
- `ExchangeStateManager`: Manages balances from exchange
- `TradingStateManager`: Derives positions from balances
- `LevelExecutor`: Resolves levels, checks balances, places orders
- `RiskManager`: Checks balance limits, position limits

### How Futures Trading Works in the System

**Data Flow**:
1. **Exchange Engine** receives WebSocket updates → Updates `FuturesBalance` and `Position`
2. **Trading Engine** subscribes to position updates → Tracks explicit positions
3. **Level Executor** resolves level → Checks margin → Places order
4. **Order fills** → Position created/updated → Margin locked/unlocked
5. **Mark price updates** → Unrealized P&L recalculated → Liquidation risk monitored

**Key Components**:
- `ExchangeStateManager`: Manages futures balances and positions from exchange
- `TradingStateManager`: Tracks explicit positions (separate from balance)
- `LevelExecutor`: Resolves levels, checks margin, considers leverage
- `RiskManager`: Checks margin limits, liquidation risk, leverage limits

---

## Key Concepts

### Spot Trading
- **Ownership**: You own the actual assets (BTC, ETH, etc.)
- **Position Representation**: Derived from balances (if you have 1 BTC balance, you have a 1 BTC position)
- **Risk**: Lower risk - no leverage, no liquidation
- **Use Case**: Buying/selling actual assets, simple trading strategies, long-term holding

### Futures Trading
- **Ownership**: You trade derivative contracts (don't own the underlying asset)
- **Position Representation**: Explicit positions tracked separately from collateral
- **Risk**: Higher risk - leverage available, liquidation risk, funding fees
- **Use Case**: Leveraged trading, hedging, directional trading without owning assets, short-term speculation

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
