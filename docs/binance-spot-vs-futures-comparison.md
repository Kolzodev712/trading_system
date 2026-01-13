# Binance Spot vs Futures Integration Comparison

This document outlines the key differences and similarities between the Binance Spot and Binance USD Futures integrations in the trading system. It covers data structures, API interactions, commands, and system usage patterns.

## Executive Summary

**Binance Spot** and **Binance USD Futures** are **separate integrations** using different API base URLs and endpoint structures. They share similar patterns but have fundamental differences in:
- Account representation (balances vs positions)
- Market data (tickers vs mark price/funding rate)
- Order types (basic vs advanced)
- WebSocket protocols (RPC vs REST-only trading)

## Architecture Differences

### API Base URLs

| Component | Binance Spot | Binance USD Futures |
|-----------|-------------|-------------------|
| **REST API** | `https://api.binance.com/api/v3/` | `https://fapi.binance.com/fapi/v1/` |
| **Market Data WS** | `wss://stream.binance.com:9443/ws` | `wss://fstream.binance.com/stream` |
| **User Data WS** | `wss://stream.binance.com:9443/ws` (via session.logon) | `wss://fstream.binance.com/ws/<listenKey>` |
| **RPC WebSocket** | `wss://ws-api.binance.com:443/ws-api/v3` | **NOT SUPPORTED** |

### Driver Structure

Both integrations follow the same directory structure:
```
exchanges/
├── binance/          # Spot integration
│   ├── common/       # Rate limiting, types
│   ├── rest/         # REST client
│   ├── websocket/    # WebSocket (RPC + streaming)
│   ├── event_loop.rs # Coordinates RPC + streaming
│   └── driver.rs
│
└── binance_futures/  # Futures integration
    ├── common/       # Rate limiting, types
    ├── rest/         # REST client (ALL trading operations)
    ├── websocket/    # WebSocket (streaming ONLY)
    ├── event_loop.rs # Coordinates streaming (no RPC)
    └── driver.rs
```

**Key Difference**: Futures does NOT support RPC WebSocket, so all trading operations must use REST API.

## Data Structures & State Management

### Account Data

#### Spot: Balances

**Data Structure**: `Balance`
- `asset: Symbol` - Currency symbol (e.g., "BTC", "USDT")
- `free: Decimal` - Available balance for trading
- `locked: Decimal` - Balance locked in open orders

**How It's Used**:
- Positions are **derived** from balances (if you have 1 BTC, you have a 1 BTC position)
- Balances are updated via REST API (`GET /api/v3/account`) and WebSocket user data stream
- Single source of truth: `ExchangeState.balances: HashMap<Symbol, Balance>`

**Example**:
```rust
// Spot balance update
Balance {
    asset: "BTC".into(),
    free: Decimal::from(1.5),
    locked: Decimal::from(0.5),  // 0.5 BTC in open orders
}
// This represents: 1.5 BTC available, 0.5 BTC in orders = 2.0 BTC total
```

#### Futures: Positions + Collateral Balance

**Data Structures**:

1. **`FuturesPosition`** - Explicit position tracking
   - `symbol: Pair` - Trading pair (e.g., "BTCUSDT")
   - `quantity: Decimal` - Position size (positive = long, negative = short)
   - `entry_price: Decimal` - Average entry price
   - `mark_price: Decimal` - Current mark price
   - `unrealized_pnl: Decimal` - Unrealized profit/loss
   - `leverage: u32` - Leverage used
   - `margin_type: MarginType` - Isolated or Cross
   - `liquidation_price: Decimal` - Price at which position would be liquidated

2. **`FuturesBalance`** - Collateral balance (separate from positions)
   - `asset: Symbol` - Collateral asset (usually USDT)
   - `balance: Decimal` - Total wallet balance
   - `available_balance: Decimal` - Available for new positions
   - `cross_un_pnl: Decimal` - Unrealized P&L (if cross margin)

**How It's Used**:
- Positions are **explicit** - you can have a long BTC position regardless of balance
- You can be long BTC and short ETH simultaneously (hedge mode)
- Positions are tracked separately from collateral balance
- Multiple sources of truth:
  - `ExchangeState.positions: HashMap<Pair, FuturesPosition>`
  - `ExchangeState.futures_balances: HashMap<Symbol, FuturesBalance>`

**Example**:
```rust
// Futures position (you can have position without owning the asset)
FuturesPosition {
    symbol: "BTCUSDT".into(),
    quantity: Decimal::from(1),  // 1 BTC long position
    entry_price: Decimal::from(50000),
    mark_price: Decimal::from(51000),
    unrealized_pnl: Decimal::from(1000),  // +$1000 P&L
    leverage: 10,
    margin_type: MarginType::Isolated,
    liquidation_price: Decimal::from(45000),
}

// Collateral balance (separate from position)
FuturesBalance {
    asset: "USDT".into(),
    balance: Decimal::from(10000),  // $10,000 collateral
    available_balance: Decimal::from(5000),  // $5,000 available for new positions
    cross_un_pnl: Decimal::ZERO,  // Using isolated margin
}
```

### Order Data

#### Spot: Basic Order Types

**Supported Order Types**:
- `Limit` - Limit order with price
- `Market` - Market order (execute at best available price)

**Order Structure**: `Order`
- Standard fields: `symbol`, `side`, `quantity`, `price`, `status`
- Simpler order lifecycle: `New` → `PartiallyFilled` → `Filled` or `Canceled`

#### Futures: Advanced Order Types

**Supported Order Types**:
- `Limit` - Limit order
- `Market` - Market order
- `Stop` - Stop-loss order
- `StopMarket` - Stop-market order
- `TakeProfit` - Take-profit order
- `TakeProfitMarket` - Take-profit market order
- `TrailingStop` - Trailing stop order

**Additional Order Fields**:
- `reduce_only: bool` - Only reduce position, don't open new
- `close_position: bool` - Close entire position
- `position_side: PositionSide` - LONG, SHORT, or BOTH (hedge mode)
- `stop_price: Decimal` - Stop price for stop orders
- `working_type: WorkingType` - Mark price or contract price trigger

**Order Structure**: Same `Order` type but with futures-specific fields populated.

### Market Data

#### Spot: Ticker & Trade Data

**Data Types**:
- **Orderbook**: Best bid/ask prices and depths
- **Trades**: Public trade executions (price, quantity, timestamp)
- **24hr Ticker**: Last price, volume, high, low, price change

**Streams**:
- `wss://stream.binance.com:9443/ws/<symbol>@depth@100ms` - Orderbook updates
- `wss://stream.binance.com:9443/ws/<symbol>@trade` - Trade stream

**Usage in System**:
- Orderbooks feed `StrategyExecutor` for decision making
- Trades are used for execution analysis
- Ticker data provides quick price reference

#### Futures: Mark Price, Funding Rate, Index Price

**Data Types**:
- **Mark Price**: Fair value price used for P&L and liquidation calculations
- **Funding Rate**: Periodic payment between long and short positions
- **Index Price**: Underlying asset price (from spot exchanges)
- **Open Interest**: Total number of open contracts

**Streams**:
- `wss://fstream.binance.com/ws/<symbol>@markPrice@1s` - Mark price (includes funding rate)
- `wss://fstream.binance.com/ws/<symbol>@indexPrice@1s` - Index price
- `wss://fstream.binance.com/ws/<symbol>@openInterest` - Open interest

**Usage in System**:
- Mark price is used for position P&L calculations
- Funding rate is used for strategy decisions (funding rate arbitrage)
- Index price is used for mark price calculation (not directly traded)
- Open interest indicates market sentiment

**Key Difference**: Futures has **funding rate** which is a periodic fee paid/received based on position direction. Spot has no equivalent.

## Commands & Operations

### Trading Operations

#### Spot: RPC WebSocket + REST

**Order Placement**:
- **Via RPC WebSocket**: `session.logon` → `order.place` (low latency)
- **Via REST**: `POST /api/v3/order` (higher latency, but always available)
- Both methods supported, RPC preferred for active trading

**Order Cancellation**:
- **Via RPC WebSocket**: `order.cancel`, `openOrders.cancelAll`
- **Via REST**: `DELETE /api/v3/order`, `DELETE /api/v3/openOrders`

**Balance Query**:
- **Via RPC WebSocket**: `account.status`
- **Via REST**: `GET /api/v3/account`

#### Futures: REST Only (No RPC)

**Order Placement**:
- **Via REST ONLY**: `POST /fapi/v1/order`
- No RPC WebSocket support - all trading via REST
- Lower latency trading requires fast REST API calls

**Order Cancellation**:
- **Via REST ONLY**: `DELETE /fapi/v1/order`, `DELETE /fapi/v1/allOpenOrders`

**Position Query**:
- **Via REST**: `GET /fapi/v2/positionRisk` - Get all positions
- **Via REST**: `GET /fapi/v2/balance` - Get collateral balance

**Futures-Specific Operations** (REST only):
- `POST /fapi/v1/leverage` - Change leverage
- `POST /fapi/v1/marginType` - Change margin type (isolated/cross)
- `POST /fapi/v1/positionSide/dual` - Change position mode (one-way/hedge)

### WebSocket Streaming

#### Spot: Dual Connection Pattern

**Two WebSocket Connections**:
1. **RPC Connection**: `wss://ws-api.binance.com:443/ws-api/v3`
   - Authenticated via `session.logon` (API key + signature)
   - Handles trading operations (order placement, cancellation)
   - Handles account queries

2. **Streaming Connection**: `wss://stream.binance.com:9443/ws`
   - Authenticated via `session.logon` (API key + signature)
   - Handles user data stream (order updates, balance updates, trade fills)
   - Long-lived, read-only stream

**Event Loop**: Coordinates both connections, handles reconnection for each independently.

#### Futures: Single Streaming Connection

**One WebSocket Connection**:
- **User Data Stream**: `wss://fstream.binance.com/ws/<listenKey>`
  - Authenticated via `listenKey` (created via REST: `POST /fapi/v1/listenKey`)
  - Handles user data stream only (position updates, balance updates, order updates)
  - No trading operations via WebSocket

**Listen Key Management**:
- Created via REST: `POST /fapi/v1/listenKey`
- Renewed every 60 minutes via REST: `PUT /fapi/v1/listenKey`
- Deleted on shutdown: `DELETE /fapi/v1/listenKey`

**Event Loop**: Simpler - only manages streaming connection (no RPC coordination).

### WebSocket Events

#### Spot: User Data Stream Events

**Event Types**:
- `executionReport` - Order status updates (NEW, PARTIALLY_FILLED, FILLED, CANCELED)
- `outboundAccountPosition` - Balance updates (when balance changes)
- `balanceUpdate` - Detailed balance change events

**Example Event**:
```json
{
  "e": "executionReport",
  "s": "BTCUSDT",
  "c": "my_order_123",
  "S": "BUY",
  "o": "LIMIT",
  "q": "0.1",
  "p": "50000",
  "x": "FILLED",
  "z": "0.1"
}
```

#### Futures: User Data Stream Events

**Event Types**:
- `ORDER_TRADE_UPDATE` - Order and trade updates (fills, cancellations)
- `ACCOUNT_UPDATE` - Position and balance updates
- `MARGIN_CALL` - Margin call warnings (optional, for monitoring)

**Example Events**:
```json
// Order fill event
{
  "e": "ORDER_TRADE_UPDATE",
  "o": {
    "s": "BTCUSDT",
    "c": "my_order_123",
    "S": "BUY",
    "o": "LIMIT",
    "q": "1",
    "p": "50000",
    "X": "FILLED",
    "z": "1",
    "ap": "50000",  // Average price
    "rp": "0",      // Realized P&L
    "ps": "LONG"    // Position side
  }
}

// Position update event
{
  "e": "ACCOUNT_UPDATE",
  "a": {
    "P": [
      {
        "s": "BTCUSDT",
        "pa": "1",          // Position amount
        "ep": "50000",      // Entry price
        "cr": "0",          // Unrealized P&L
        "up": "1000",       // Unrealized P&L
        "mt": "isolated",   // Margin type
        "ps": "LONG"        // Position side
      }
    ],
    "B": [
      {
        "a": "USDT",
        "wb": "10000",      // Wallet balance
        "cw": "5000"        // Available balance
      }
    ]
  }
}
```

## System Integration

### Exchange Engine State

#### Spot: `ExchangeState`

```rust
pub struct ExchangeState {
    pub balances: HashMap<Symbol, Balance>,      // Currency balances
    pub orders: HashMap<OrderId, Order>,         // Active orders
    pub trades: HashMap<TradeId, Trade>,         // Trade history
    // No explicit positions - derived from balances
}
```

**Reconciliation**:
- Fetches balances from REST API on startup
- Fetches open orders
- Fetches trade history
- WebSocket stream keeps state updated in real-time

#### Futures: `ExchangeState` (Extended)

```rust
pub struct ExchangeState {
    // Spot fields (still present for compatibility)
    pub balances: HashMap<Symbol, Balance>,
    pub orders: HashMap<OrderId, Order>,
    pub trades: HashMap<TradeId, Trade>,
    
    // Futures-specific fields
    pub positions: HashMap<Pair, FuturesPosition>,           // Explicit positions
    pub futures_balances: HashMap<Symbol, FuturesBalance>,   // Collateral balances
    pub leverage_info: HashMap<Pair, LeverageInfo>,          // Leverage per pair
}
```

**Reconciliation**:
- Fetches positions from REST API (`/fapi/v2/positionRisk`)
- Fetches collateral balance (`/fapi/v2/balance`)
- Fetches open orders
- Fetches trade history
- WebSocket stream keeps state updated in real-time

### Market Data Engine State

#### Spot: `MarketDataState`

```rust
pub struct MarketDataState {
    pub orderbooks: HashMap<Pair, StructuredOrderbook>,  // Best bid/ask
    pub trades: HashMap<Pair, Vec<StructuredPublicTrade>>, // Recent trades
    pub tickers: HashMap<Pair, Ticker>,                   // 24hr ticker data
}
```

#### Futures: `MarketDataState` (Extended)

```rust
pub struct MarketDataState {
    // Spot fields (still present)
    pub orderbooks: HashMap<Pair, StructuredOrderbook>,
    pub trades: HashMap<Pair, Vec<StructuredPublicTrade>>,
    pub tickers: HashMap<Pair, Ticker>,
    
    // Futures-specific fields
    pub mark_prices: HashMap<Pair, Decimal>,               // Mark price
    pub index_prices: HashMap<Pair, Decimal>,              // Index price
    pub funding_rates: HashMap<Pair, FundingRate>,         // Funding rate info
    pub open_interest: HashMap<Pair, Decimal>,             // Open interest
}
```

### Asset Engine

#### Spot: Trading Rules

**Fetched Data**:
- Symbol information (base/quote, min/max quantities)
- Trading rules (price filters, lot size filters, min notional)
- Exchange-specific restrictions

**Usage**:
- Validates order requests before sending to exchange
- Enforces minimum order sizes, price precision, etc.

#### Futures: Trading Rules + Leverage Brackets

**Fetched Data**:
- Symbol information (same as spot)
- Trading rules (same as spot)
- **Leverage brackets** (max leverage per position size)
- **Initial margin requirements**
- **Maintenance margin requirements**

**Usage**:
- Validates order requests (same as spot)
- **Enforces leverage limits** based on position size
- **Calculates margin requirements** before allowing positions
- **Monitors liquidation risk** based on mark price and margin

## Trading Flow Comparison

### Spot Trading Flow

1. **Order Placement**:
   - Strategy decides to buy 0.1 BTC at $50,000
   - `StrategyExecutor` creates `OrderRequest`
   - `BinanceClient.open_order()` called
   - **Via RPC WebSocket** (preferred): Send `order.place` request
   - **Or via REST**: `POST /api/v3/order`
   - Order added to `ExchangeState.orders`

2. **Order Fill**:
   - WebSocket receives `executionReport` event with status `FILLED`
   - `ExchangeState` updated: order status changed, trade created
   - Balance updated: BTC increased, USDT decreased
   - Position derived: You now have 0.1 BTC (balance change)

3. **Position Tracking**:
   - No explicit position - your BTC balance IS your position
   - To "close position", sell the BTC you own

### Futures Trading Flow

1. **Order Placement**:
   - Strategy decides to go long 1 BTC at $50,000 with 10x leverage
   - `StrategyExecutor` creates `OrderRequest` (or `FuturesOrderRequest`)
   - `BinanceFuturesClient.open_order()` called
   - **Via REST ONLY**: `POST /fapi/v1/order` (no RPC option)
   - Order added to `ExchangeState.orders`

2. **Order Fill**:
   - WebSocket receives `ORDER_TRADE_UPDATE` event with status `FILLED`
   - `ExchangeState` updated: order status changed, trade created
   - **Position created/updated**: 1 BTC long position added to `ExchangeState.positions`
   - **Collateral balance updated**: USDT collateral decreased (margin used)
   - No actual BTC/USDT exchange - it's a derivative contract

3. **Position Tracking**:
   - Explicit position: `FuturesPosition` with quantity, entry price, unrealized P&L
   - Mark price updates continuously calculate unrealized P&L
   - To "close position", place opposite order (sell 1 BTC to close long)

4. **Leverage & Margin**:
   - Leverage determines how much margin is required (10x = 10% margin)
   - Margin type determines how margin is allocated (isolated vs cross)
   - Liquidation price calculated based on margin and leverage

## Detailed Trading Examples

### Example 1: Buying BTC on Spot vs Futures

#### Spot: Buying 0.1 BTC at $50,000

**Step 1: Check Balance**
```rust
// REST API call: GET /api/v3/account
let balances = client.fetch_balances().await?;
// Returns: { "BTC": { free: 0.0, locked: 0.0 }, "USDT": { free: 10000.0, locked: 0.0 } }
```

**Step 2: Place Limit Order**
```rust
// Via RPC WebSocket (preferred for low latency)
let order_request = OrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Buy,
    order_type: OrderType::Limit,
    price: Decimal::from(50000),
    amount: Decimal::from_str("0.1")?,
    instrument_type: InstrumentType::Spot,
    // ...
};

// RPC WebSocket: order.place
let order = client.open_order(order_request).await?;
// Returns: Order { order_id: "abc123", status: OrderStatus::New, ... }
```

**Step 3: Order Fill Event (WebSocket)**
```json
{
  "e": "executionReport",
  "s": "BTCUSDT",
  "c": "my_order_123",
  "S": "BUY",
  "o": "LIMIT",
  "q": "0.1",
  "p": "50000",
  "x": "FILLED",
  "z": "0.1",
  "Z": "5000"  // Cumulative quote quantity (0.1 * 50000)
}
```

**Step 4: Balance Update**
```rust
// Updated balances after fill:
// { "BTC": { free: 0.1, locked: 0.0 }, "USDT": { free: 5000.0, locked: 0.0 } }
// You now OWN 0.1 BTC
```

**Step 5: Sell to Close Position**
```rust
let sell_order = OrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Sell,
    order_type: OrderType::Limit,
    price: Decimal::from(51000),  // Selling at profit
    amount: Decimal::from_str("0.1")?,
    instrument_type: InstrumentType::Spot,
    // ...
};
let order = client.open_order(sell_order).await?;
// After fill: Balance is back to USDT only
```

#### Futures: Going Long 1 BTC at $50,000 with 10x Leverage

**Step 1: Set Leverage (One-time setup)**
```rust
// REST API call: POST /fapi/v1/leverage
let leverage_info = client.set_leverage(&"BTCUSDT".into(), 10).await?;
// Returns: LeverageInfo { leverage: 10, max_notional: 1000000, ... }
```

**Step 2: Check Collateral Balance**
```rust
// REST API call: GET /fapi/v2/balance
let balances = client.fetch_futures_balances().await?;
// Returns: [{ asset: "USDT", balance: 10000.0, available_balance: 10000.0, ... }]
```

**Step 3: Place Limit Order (REST only)**
```rust
// Via REST API (no RPC WebSocket for Futures)
let order_request = FuturesOrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Buy,
    order_type: FuturesOrderType::Limit,
    quantity: Decimal::from(1),  // 1 BTC contract
    price: Some(Decimal::from(50000)),
    position_side: PositionSide::Long,
    reduce_only: false,
    leverage: 10,
    // ...
};

// REST: POST /fapi/v1/order
let order = client.open_order(order_request).await?;
// Returns: Order { order_id: "xyz789", status: OrderStatus::New, ... }
```

**Step 4: Order Fill Event (WebSocket)**
```json
{
  "e": "ORDER_TRADE_UPDATE",
  "o": {
    "s": "BTCUSDT",
    "c": "my_order_456",
    "S": "BUY",
    "o": "LIMIT",
    "q": "1",
    "p": "50000",
    "ap": "50000",  // Average fill price
    "X": "FILLED",
    "z": "1",  // Cumulative filled quantity
    "ps": "LONG",
    "rp": "0",  // Realized P&L (0 for new position)
    "cp": false  // Close position flag
  }
}
```

**Step 5: Position Created (WebSocket)**
```json
{
  "e": "ACCOUNT_UPDATE",
  "a": {
    "P": [{
      "s": "BTCUSDT",
      "pa": "1",  // Position amount (1 BTC long)
      "ep": "50000",  // Entry price
      "up": "0",  // Unrealized P&L (0 at entry)
      "cr": "0",  // Unrealized P&L
      "mt": "isolated",  // Margin type
      "ps": "LONG",
      "lq": "5000"  // Liquidation price (approximate)
    }],
    "B": [{
      "a": "USDT",
      "wb": "10000",  // Wallet balance
      "cw": "5000"  // Available balance (5000 used as margin)
    }]
  }
}
```

**Step 6: Mark Price Updates (Continuous)**
```json
{
  "e": "markPriceUpdate",
  "s": "BTCUSDT",
  "p": "51000",  // Mark price
  "r": "0.0001",  // Funding rate (0.01% per 8 hours)
  "T": 1234567890
}
```

**Step 7: Unrealized P&L Calculation**
```rust
// System calculates: (Mark Price - Entry Price) * Quantity
// (51000 - 50000) * 1 = $1000 unrealized profit
// Position: { quantity: 1, entry_price: 50000, unrealized_pnl: 1000, ... }
```

**Step 8: Close Position**
```rust
// Sell 1 BTC to close the long position
let close_order = FuturesOrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Sell,
    order_type: FuturesOrderType::Market,  // Market order for immediate close
    quantity: Decimal::from(1),
    position_side: PositionSide::Long,
    reduce_only: true,  // Important: only reduce, don't open short
    // ...
};
let order = client.open_order(close_order).await?;
// After fill: Position quantity becomes 0, realized P&L is calculated
```

### Example 2: Stop-Loss Orders

#### Spot: Stop-Loss via Regular Order
```rust
// Spot doesn't have native stop-loss, so you monitor price and place order manually
// Strategy monitors: if BTC drops to $48,000, place sell order
let stop_loss_order = OrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Sell,
    order_type: OrderType::Market,  // Market order for immediate execution
    amount: Decimal::from_str("0.1")?,  // Sell all BTC you own
    instrument_type: InstrumentType::Spot,
    // ...
};
```

#### Futures: Native Stop-Loss Order
```rust
// Futures has native stop-loss order type
let stop_loss = FuturesOrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Sell,
    order_type: FuturesOrderType::StopMarket,  // Stop-market order
    quantity: Decimal::from(1),
    stop_price: Decimal::from(48000),  // Trigger price
    working_type: WorkingType::ContractPrice,  // Use contract price (not mark price)
    position_side: PositionSide::Long,
    reduce_only: true,  // Close position, don't open short
    // ...
};
// Order sits on exchange, automatically triggers if price drops to $48,000
```

### Example 3: Position Management

#### Spot: Managing Multiple Assets
```rust
// You can hold multiple assets simultaneously
// Balances:
// { "BTC": { free: 0.5, locked: 0.0 },
//   "ETH": { free: 10.0, locked: 0.0 },
//   "USDT": { free: 5000.0, locked: 0.0 } }

// To "close" BTC position: sell BTC for USDT
// To "close" ETH position: sell ETH for USDT
// Each asset is independent
```

#### Futures: Managing Multiple Positions (Hedge Mode)
```rust
// Enable hedge mode (one-way mode is default)
client.set_position_mode(true).await?;  // true = hedge mode

// Open long BTC position
let long_order = FuturesOrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Buy,
    position_side: PositionSide::Long,  // Explicit long
    quantity: Decimal::from(1),
    // ...
};

// Simultaneously open short BTC position (hedging)
let short_order = FuturesOrderRequest {
    symbol: "BTCUSDT".into(),
    side: Side::Sell,
    position_side: PositionSide::Short,  // Explicit short
    quantity: Decimal::from(0.5),
    // ...
};

// Result: Long 1 BTC, Short 0.5 BTC = Net long 0.5 BTC
// Positions tracked separately:
// positions["BTCUSDT"] = [
//   { position_side: Long, quantity: 1, ... },
//   { position_side: Short, quantity: 0.5, ... }
// ]
```

### Example 4: Margin Types

#### Futures: Isolated Margin
```rust
// Set margin type to isolated (one-time setup per symbol)
client.set_margin_type(&"BTCUSDT".into(), MarginType::Isolated).await?;

// Each position has its own margin
// Position 1: 1 BTC long, margin: $5,000 (isolated)
// Position 2: 1 ETH long, margin: $1,000 (isolated)
// Total collateral: $10,000
// Used margin: $6,000 (isolated positions don't share margin)
// Available: $4,000

// If Position 1 is liquidated, Position 2 is unaffected
```

#### Futures: Cross Margin
```rust
// Set margin type to cross (one-time setup per symbol)
client.set_margin_type(&"BTCUSDT".into(), MarginType::Cross).await?;

// All positions share margin pool
// Position 1: 1 BTC long, margin used: $5,000
// Position 2: 1 ETH long, margin used: $1,000
// Total collateral: $10,000
// Total used: $6,000
// Available: $4,000 (shared across all positions)

// If one position loses money, it reduces available margin for all positions
// More efficient margin usage, but higher risk
```

### Example 5: Order Cancellation Patterns

#### Spot: Cancel by Order ID (RPC WebSocket)
```rust
// Via RPC WebSocket (low latency)
let cancel_params = BinanceWsCancelOrderReqParams::new_with_order_id(
    "BTCUSDT".into(),
    OrderId::new("order_123".into()),
);
let result = client.cancel_order_rpc(cancel_params).await?;
// Returns immediately via RPC response
```

#### Spot: Cancel All Orders (RPC WebSocket)
```rust
// Cancel all open orders for a symbol
let cancel_all_params = BinanceWsCancelAllOrdersReqParams::new("BTCUSDT".into());
let result = client.cancel_all_orders_rpc(cancel_all_params).await?;
// Returns list of cancelled orders
```

#### Futures: Cancel by Order ID (REST only)
```rust
// Via REST API (no RPC option)
let result = client.cancel_order(
    &"BTCUSDT".into(),
    Some(&OrderId::new("order_456".into())),
    None,  // No client_order_id
).await?;
// Returns cancelled order details
```

#### Futures: Cancel All Orders (REST only)
```rust
// Cancel all open orders for a symbol
let result = client.cancel_all_orders(&"BTCUSDT".into()).await?;
// Returns confirmation
```

### Example 6: Funding Rate Strategy

#### Futures: Funding Rate Arbitrage
```rust
// Fetch current funding rate
let funding_rate = client.fetch_funding_rate(&"BTCUSDT".into()).await?;
// Returns: FundingRate { 
//   rate: 0.0001,  // 0.01% per 8 hours
//   next_funding_time: 1234567890,
//   ...
// }

// Strategy: If funding rate is positive, longs pay shorts
// If funding rate is high (e.g., 0.1% per 8 hours), consider going short
// to receive funding payments

if funding_rate.rate > Decimal::from_str("0.001")? {
    // High funding rate - go short to receive payments
    let short_order = FuturesOrderRequest {
        symbol: "BTCUSDT".into(),
        side: Side::Sell,
        order_type: FuturesOrderType::Market,
        quantity: Decimal::from(1),
        position_side: PositionSide::Short,
        // ...
    };
    client.open_order(short_order).await?;
    
    // Hold position until next funding payment (8 hours)
    // Receive: 0.1% of position notional value every 8 hours
}
```

### Example 7: Order Book Depth Analysis

#### Spot: Market Depth
```rust
// Fetch orderbook depth
let orderbook = client.fetch_orderbook(&"BTCUSDT".into(), 20).await?;
// Returns: Orderbook {
//   bids: [(50000, 0.5), (49999, 1.0), ...],  // Best bid first
//   asks: [(50001, 0.3), (50002, 2.0), ...],  // Best ask first
// }

// Calculate spread
let spread = orderbook.asks[0].0 - orderbook.bids[0].0;  // 50001 - 50000 = 1 USDT
let spread_bps = (spread / orderbook.bids[0].0) * 10000;  // 2 bps
```

#### Futures: Liquidity Analysis
```rust
// Same orderbook structure, but analyze for futures-specific metrics
let orderbook = client.fetch_orderbook(&"BTCUSDT".into(), 100).await?;

// Calculate cumulative depth
let cumulative_bid_depth: Decimal = orderbook.bids
    .iter()
    .take(10)
    .map(|(_, qty)| qty)
    .sum();  // Total quantity within 10 levels

// Check for orderbook imbalances (predict short-term price movement)
let bid_ask_ratio = cumulative_bid_depth / cumulative_ask_depth;
if bid_ask_ratio > Decimal::from(2) {
    // Strong buy pressure - price likely to move up
}
```

## Key Takeaways

### Similarities

1. **Structure**: Both follow same directory/module structure
2. **Authentication**: Both use HMAC-SHA256 for REST API
3. **Rate Limiting**: Both use weight-based rate limiting
4. **State Management**: Both use `ExchangeStateManager` pattern
5. **WebSocket Streaming**: Both stream user data updates
6. **Reconciliation**: Both bootstrap state from REST API on startup

### Differences

1. **API Base URLs**: Completely different endpoints (separate APIs)
2. **RPC Support**: Spot has RPC WebSocket, Futures does not
3. **Account Model**: Spot uses balances, Futures uses positions + collateral
4. **Market Data**: Spot has tickers, Futures has mark price/funding rate
5. **Order Types**: Spot basic, Futures advanced (stop-loss, take-profit, etc.)
6. **Leverage**: Futures has leverage, Spot does not
7. **Margin**: Futures has margin types (isolated/cross), Spot does not
8. **Position Mode**: Futures has one-way vs hedge mode, Spot does not

### When to Use Which

**Use Spot**:
- Buying/selling actual assets (BTC, ETH, etc.)
- Simple trading strategies (buy low, sell high)
- When you want to own the underlying asset
- Lower risk (no leverage, no liquidation)

**Use Futures**:
- Trading with leverage (amplify gains/losses)
- Hedging existing positions
- Advanced order types (stop-loss, take-profit)
- Directional trading without owning the asset
- Funding rate arbitrage strategies
- Higher risk (leverage, liquidation risk)

---

**Last Updated**: December 2024  
**Maintained By**: Development Team

