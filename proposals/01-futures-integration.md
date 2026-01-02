# Proposal: Futures Trading Integration

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft

## Executive Summary

This proposal outlines the integration of futures trading capabilities into the trading system. The design ensures that futures support is **optional** and **non-breaking** for existing spot trading functionality. The architecture will support multiple futures exchanges (Binance USD Futures, Bybit) with a unified interface while maintaining exchange-specific drivers.

## Objectives

1. **Add futures-specific data types** to `trading_core` types
2. **Extend traits** with optional futures methods (using trait extensions/default implementations)
3. **Create Binance USD Futures driver** (separate from spot, different API endpoints)
4. **Design for future Bybit integration** (same patterns, different driver)
5. **Maintain backward compatibility** - all futures methods are optional
6. **Support futures in all components** - Exchange Engine, Market Data Engine, Trading Engine, Asset Engine

## Key Design Principles

1. **Unified TradingClient**: Futures methods are added directly to `TradingClient` with default "not supported" implementations
2. **Single Driver Per Exchange**: One driver handles both spot and futures (unless API is separate like Binance)
3. **Differentiation by InstrumentType**: Spot vs futures differentiated by `OrderRequest.instrument_type` field
4. **Optional Support**: Exchanges that don't support futures simply return "not supported" from default implementations
5. **Type Safety**: Futures-specific types are clearly distinguished from spot types
6. **Exchange Agnostic**: Core types and traits are exchange-agnostic; exchange-specific logic lives in drivers
7. **Incremental Implementation**: Can be implemented phase-by-phase without breaking existing functionality
8. **Exception for Separate APIs**: Binance is an exception (separate API URLs) and requires separate drivers

## Design Rationale

### Why Unified Client Instead of Separate Traits?

**Benefits of Unified Approach:**
1. **Simpler Architecture**: One client, one driver per exchange (unless API forces separation)
2. **Less Code Duplication**: Shared authentication, rate limiting, connection management
3. **More Flexible**: Can trade both spot and futures with same client, same connection
4. **Easier Maintenance**: Single codebase to maintain per exchange
5. **Natural Pattern**: Most exchanges (Bybit, OKX, etc.) use unified APIs

**When Separate Drivers Are Needed:**
- **Binance Exception**: Separate API URLs (`api.binance.com` vs `fapi.binance.com`)
- **Different Protocols**: If authentication or protocol differs significantly
- **Independent Scaling**: If spot and futures need completely independent scaling (rare)

**Differentiation Strategy:**
- `OrderRequest.instrument_type` field determines spot vs futures
- Same `open_order()` method handles both (routes internally)
- Futures-specific methods (`fetch_positions`, `set_leverage`) are optional
- Default implementations return "not supported" for exchanges without futures

This design follows the principle: **"One driver per exchange, unless the API forces separation"**

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Trading Core Types                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │   Spot Types │  │ Futures Types│  │ Common Types │      │
│  │  (existing)  │  │   (new)      │  │  (existing)  │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Core Traits                               │
│  ┌────────────────────────────────────────────────────┐    │
│  │ TradingClient                                      │    │
│  │  - Spot methods (existing)                         │    │
│  │  - Futures methods (new, optional defaults)       │    │
│  │  - Same client handles both via InstrumentType    │    │
│  └────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ MarketDataClient                                   │    │
│  │  - Spot methods (existing)                         │    │
│  │  - Futures methods (new, optional defaults)       │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Exchange-Specific Drivers                       │
│  ┌──────────────────┐  ┌──────────────────┐              │
│  │ Binance Spot     │  │ Binance Futures  │              │
│  │ (existing)       │  │ (separate API)    │              │
│  │                   │  │ (exception)      │              │
│  └──────────────────┘  └──────────────────┘              │
│  ┌──────────────────┐                                     │
│  │ Bybit Unified    │                                     │
│  │ (spot + futures) │                                     │
│  │ (single driver)  │                                     │
│  └──────────────────┘                                     │
└─────────────────────────────────────────────────────────────┘
```

## Phase 1: Core Types & Data Structures

### 1.1 Futures-Specific Types in `trading_core`

**Location**: `libs/trading_core/src/types/futures.rs` (new file)

#### Position (Enhanced)
```rust
/// Futures position with margin and leverage information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesPosition {
    /// Trading pair
    pub pair: Pair,
    /// Position size (positive for long, negative for short)
    pub quantity: Decimal,
    /// Average entry price
    pub avg_entry_price: Decimal,
    /// Current mark price (for P&L calculation)
    pub mark_price: Decimal,
    /// Current index price (for funding rate calculation)
    pub index_price: Decimal,
    /// Unrealized P&L
    pub unrealized_pnl: Decimal,
    /// Realized P&L (cumulative)
    pub realized_pnl: Decimal,
    /// Position margin (initial margin)
    pub initial_margin: Decimal,
    /// Maintenance margin (minimum margin to maintain position)
    pub maintenance_margin: Decimal,
    /// Leverage used for this position
    pub leverage: u32,
    /// Position mode (one-way or hedge mode)
    pub position_mode: PositionMode,
    /// Margin type (isolated or cross margin)
    pub margin_type: MarginType,
    /// Liquidation price (estimated)
    pub liquidation_price: Option<Decimal>,
    /// Last update timestamp
    pub last_update: u64,
}
```

#### Collateral & Margin
```rust
/// Futures account collateral/balance
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesBalance {
    /// Asset symbol (e.g., "USDT")
    pub asset: Symbol,
    /// Available balance (can be used for new positions)
    pub available_balance: Decimal,
    /// Wallet balance (total balance including unrealized P&L)
    pub wallet_balance: Decimal,
    /// Cross wallet balance (for cross margin mode)
    pub cross_wallet_balance: Decimal,
    /// Cross unrealized P&L (for cross margin mode)
    pub cross_unrealized_pnl: Decimal,
    /// Available for withdrawal
    pub available_withdraw: Decimal,
    /// Last update timestamp
    pub last_update: u64,
}

/// Margin information for a position
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginInfo {
    /// Initial margin required
    pub initial_margin: Decimal,
    /// Maintenance margin required
    pub maintenance_margin: Decimal,
    /// Margin ratio (maintenance / position value)
    pub margin_ratio: Decimal,
    /// Isolated margin (for isolated margin mode)
    pub isolated_margin: Option<Decimal>,
}
```

#### Leverage & Position Mode
```rust
/// Position mode (one-way vs hedge mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionMode {
    /// One-way mode: only one position per symbol (long or short)
    OneWay,
    /// Hedge mode: can have both long and short positions simultaneously
    Hedge,
}

/// Margin type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarginType {
    /// Isolated margin: margin is isolated per position
    Isolated,
    /// Cross margin: margin is shared across all positions
    Cross,
}

/// Leverage information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeverageInfo {
    /// Symbol/pair
    pub pair: Pair,
    /// Current leverage (e.g., 10 = 10x)
    pub leverage: u32,
    /// Maximum leverage allowed
    pub max_leverage: u32,
    /// Last update timestamp
    pub last_update: u64,
}
```

#### Funding Rate
```rust
/// Funding rate information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    /// Trading pair
    pub pair: Pair,
    /// Current funding rate (e.g., 0.0001 = 0.01%)
    pub rate: Decimal,
    /// Next funding time (timestamp)
    pub next_funding_time: u64,
    /// Last funding time (timestamp)
    pub last_funding_time: u64,
    /// Mark price at last funding
    pub mark_price: Decimal,
    /// Index price at last funding
    pub index_price: Decimal,
}
```

#### Futures Order Types
```rust
/// Futures-specific order types (extends OrderType)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FuturesOrderType {
    /// Standard limit order
    Limit,
    /// Market order
    Market,
    /// Stop market order (triggered by stop price)
    StopMarket,
    /// Stop limit order (triggered by stop price, then limit order)
    StopLimit,
    /// Take profit market order
    TakeProfitMarket,
    /// Take profit limit order
    TakeProfitLimit,
    /// Trailing stop order
    TrailingStopMarket,
    /// Reduce-only order (can only reduce position, not increase)
    ReduceOnly,
}

/// Time in force for futures orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FuturesTimeInForce {
    /// Good Till Cancel
    GTC,
    /// Immediate Or Cancel
    IOC,
    /// Fill Or Kill
    FOK,
    /// Post Only (maker order)
    PostOnly,
    /// Good Till Date/Time
    GTD,
}
```

#### Futures Order Request (extends OrderRequest)
```rust
/// Futures-specific order request
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesOrderRequest {
    /// Base order request fields
    #[serde(flatten)]
    pub base: OrderRequest,
    /// Position side (for hedge mode: Both, Long, Short)
    pub position_side: Option<PositionSide>,
    /// Reduce-only flag
    pub reduce_only: Option<bool>,
    /// Close position flag (close entire position)
    pub close_position: Option<bool>,
    /// Stop price (for stop orders)
    pub stop_price: Option<Decimal>,
    /// Activation price (for trailing stop)
    pub activation_price: Option<Decimal>,
    /// Callback rate (for trailing stop)
    pub callback_rate: Option<Decimal>,
    /// Working type (Mark price or Contract price for stop orders)
    pub working_type: Option<WorkingType>,
    /// Price protect (protect against adverse price movements)
    pub price_protect: Option<bool>,
}

/// Position side (for hedge mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionSide {
    /// Both sides (one-way mode)
    Both,
    /// Long side only (hedge mode)
    Long,
    /// Short side only (hedge mode)
    Short,
}

/// Working type for stop orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkingType {
    /// Mark price
    MarkPrice,
    /// Contract price (last traded price)
    ContractPrice,
}
```

### 1.2 Update Existing Types

**Location**: `libs/trading_core/src/types/exchange.rs`

- Keep existing `Position` type for spot (backward compatible)
- Add `InstrumentType` field to `OrderRequest` to distinguish spot vs futures
- Add `InstrumentType` field to `Order` to track order type

```rust
// Add to OrderRequest
pub struct OrderRequest {
    // ... existing fields ...
    /// Instrument type (Spot or Futures)
    #[serde(default)]
    pub instrument_type: InstrumentType,
}

// Add to Order
pub struct Order {
    // ... existing fields ...
    /// Instrument type (Spot or Futures)
    #[serde(default)]
    pub instrument_type: InstrumentType,
}
```

### 1.3 Update Common Types

**Location**: `libs/trading_core/src/types/common.rs`

- `InstrumentType` already exists (Spot, Futures, Derivatives) ✓
- `AccountType` already has `Futures` variant ✓
- Add `Exchange` variants for futures-specific exchanges:

```rust
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
pub enum Exchange {
    Binance,
    BinanceFutures,  // New
    Bybit,           // New (for future)
    Unsupported,
}
```

## Phase 2: Extend TradingClient Trait

### 2.1 Add Futures Methods to TradingClient

**Location**: `libs/trading_core/src/traits/trading.rs`

**Key Design Decision**: Futures methods are added directly to `TradingClient` with default implementations that return "not supported" errors. This allows:
- **Unified clients**: Same client can trade both spot and futures (differentiated by `InstrumentType` in orders)
- **Optional support**: Exchanges that don't support futures simply return "not supported"
- **Single driver**: One driver per exchange (unless API is separate like Binance)

```rust
#[async_trait]
pub trait TradingClient<OrderRequestType = OrderRequest, InstrumentId = Pair>:
    Debug + Send + Sync + 'static
where
    OrderRequestType: Send + Sync + 'static,
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    // ... existing spot methods ...
    
    // ===== FUTURES METHODS (Optional) =====
    // All have default implementations that return "not supported"
    // Exchanges with unified APIs (like Bybit) implement these in the same client
    // Exchanges with separate APIs (like Binance) have separate clients
    
    /// Fetch all open futures positions
    /// Default implementation returns "not supported"
    async fn fetch_positions(&self) -> Result<Vec<FuturesPosition>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures positions not supported".into()
        ))
    }

    /// Fetch position for a specific pair
    /// Default implementation returns "not supported"
    async fn fetch_position(&self, pair: &InstrumentId) -> Result<Option<FuturesPosition>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures positions not supported".into()
        ))
    }

    /// Fetch futures account balance/collateral
    /// Default implementation returns "not supported"
    async fn fetch_futures_balance(&self) -> Result<Vec<FuturesBalance>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures balance not supported".into()
        ))
    }

    /// Set leverage for a trading pair
    /// Default implementation returns "not supported"
    async fn set_leverage(&self, pair: &InstrumentId, leverage: u32) -> Result<LeverageInfo, EngineError> {
        Err(EngineError::NotSupported(
            "Leverage setting not supported".into()
        ))
    }

    /// Get current leverage for a trading pair
    /// Default implementation returns "not supported"
    async fn get_leverage(&self, pair: &InstrumentId) -> Result<LeverageInfo, EngineError> {
        Err(EngineError::NotSupported(
            "Leverage query not supported".into()
        ))
    }

    /// Set position mode (one-way or hedge)
    /// Default implementation returns "not supported"
    async fn set_position_mode(&self, mode: PositionMode) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Position mode setting not supported".into()
        ))
    }

    /// Get current position mode
    /// Default implementation returns "not supported"
    async fn get_position_mode(&self) -> Result<PositionMode, EngineError> {
        Err(EngineError::NotSupported(
            "Position mode query not supported".into()
        ))
    }

    /// Set margin type (isolated or cross)
    /// Default implementation returns "not supported"
    async fn set_margin_type(&self, pair: &InstrumentId, margin_type: MarginType) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Margin type setting not supported".into()
        ))
    }

    /// Get current margin type for a pair
    /// Default implementation returns "not supported"
    async fn get_margin_type(&self, pair: &InstrumentId) -> Result<MarginType, EngineError> {
        Err(EngineError::NotSupported(
            "Margin type query not supported".into()
        ))
    }

    /// Place a futures order (uses FuturesOrderRequest, but can be called via open_order with InstrumentType::Futures)
    /// This is a convenience method. The standard open_order() should handle both spot and futures
    /// based on OrderRequest.instrument_type
    /// Default implementation returns "not supported"
    async fn place_futures_order(&self, req: FuturesOrderRequest) -> Result<Order, EngineError> {
        Err(EngineError::NotSupported(
            "Futures orders not supported".into()
        ))
    }

    /// Cancel all futures orders for a pair
    /// Default implementation returns "not supported"
    async fn cancel_all_futures_orders(&self, pair: &InstrumentId) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Futures order cancellation not supported".into()
        ))
    }

    /// Fetch open futures orders
    /// Default implementation returns "not supported"
    async fn fetch_open_futures_orders(&self, pair: Option<&InstrumentId>) -> Result<Vec<Order>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures orders not supported".into()
        ))
    }
}
```

**Important**: The standard `open_order()` method should handle both spot and futures orders based on `OrderRequest.instrument_type`. The `place_futures_order()` method is a convenience wrapper.

### 2.2 Extend MarketDataClient Trait

**Location**: `libs/trading_core/src/traits/market_data.rs`

```rust
#[async_trait]
pub trait MarketDataClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    // ... existing spot methods ...
    
    // ===== FUTURES MARKET DATA METHODS (Optional) =====
    
    /// Fetch current funding rate for a pair
    /// Default implementation returns "not supported"
    async fn fetch_funding_rate(&self, pair: &InstrumentId) -> Result<FundingRate, EngineError> {
        Err(EngineError::NotSupported(
            "Funding rate not supported".into()
        ))
    }

    /// Fetch mark price for a pair
    /// Default implementation returns "not supported"
    async fn fetch_mark_price(&self, pair: &InstrumentId) -> Result<Decimal, EngineError> {
        Err(EngineError::NotSupported(
            "Mark price not supported".into()
        ))
    }

    /// Fetch index price for a pair
    /// Default implementation returns "not supported"
    async fn fetch_index_price(&self, pair: &InstrumentId) -> Result<Decimal, EngineError> {
        Err(EngineError::NotSupported(
            "Index price not supported".into()
        ))
    }

    /// Stream funding rate updates
    /// Default implementation returns "not supported"
    async fn stream_funding_rate(&mut self) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Funding rate streaming not supported".into()
        ))
    }

    /// Stream mark price updates
    /// Default implementation returns "not supported"
    async fn stream_mark_price(&mut self) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Mark price streaming not supported".into()
        ))
    }

    /// Fetch open interest
    /// Default implementation returns "not supported"
    async fn fetch_open_interest(&self, pair: &InstrumentId) -> Result<Decimal, EngineError> {
        Err(EngineError::NotSupported(
            "Open interest not supported".into()
        ))
    }
}
```

### 2.3 Update Error Types

**Location**: `libs/trading_core/src/error.rs`

Add `NotSupported` variant if it doesn't exist:

```rust
pub enum EngineError {
    // ... existing variants ...
    NotSupported(String),
}
```

## Phase 3: Exchange Engine Integration

### 4.1 Futures State Management

**Location**: `src/exchange_engine/src/state/state.rs`

Extend `ExchangeState` to support futures:

```rust
pub struct ExchangeState {
    // ... existing spot fields ...
    
    /// Futures positions by pair
    positions: HashMap<Pair, FuturesPosition>,
    
    /// Futures balances by asset
    futures_balances: HashMap<String, FuturesBalance>,
    
    /// Leverage info by pair
    leverage_info: HashMap<Pair, LeverageInfo>,
}
```

### 4.2 Futures State Publisher

**Location**: `src/exchange_engine/src/state/publisher.rs`

Add futures-specific update channels:

```rust
pub enum ExchangeUpdate {
    // ... existing variants ...
    Position(FuturesPosition),
    FuturesBalance(FuturesBalance),
    Leverage(LeverageInfo),
    FundingRate(FundingRate),
}
```

### 4.3 Futures State Manager

**Location**: `src/exchange_engine/src/state/manager.rs`

Add methods for futures state management:

```rust
impl ExchangeStateManager {
    // ... existing methods ...
    
    pub fn update_position(&self, position: FuturesPosition) { ... }
    pub fn update_futures_balance(&self, balance: FuturesBalance) { ... }
    pub fn update_leverage(&self, pair: &Pair, leverage: LeverageInfo) { ... }
    
    // Futures reconciliation
    pub async fn reconcile_futures_positions(&self) -> Result<(), EngineError> { ... }
    pub async fn reconcile_futures_balance(&self) -> Result<(), EngineError> { ... }
}
```

## Phase 5: Market Data Engine Integration

### 5.1 Futures Market Data State

**Location**: `src/market_data_engine/src/state/state.rs`

Add futures-specific market data:

```rust
pub struct MarketDataState {
    // ... existing fields ...
    
    /// Mark prices by pair
    mark_prices: HashMap<Pair, Decimal>,
    
    /// Index prices by pair
    index_prices: HashMap<Pair, Decimal>,
    
    /// Funding rates by pair
    funding_rates: HashMap<Pair, FundingRate>,
    
    /// Open interest by pair
    open_interest: HashMap<Pair, Decimal>,
}
```

### 4.2 Futures Market Data Publisher

**Location**: `src/market_data_engine/src/state/publisher.rs`

Add futures-specific updates:

```rust
pub enum MarketDataUpdate {
    // ... existing variants ...
    MarkPrice { pair: Pair, price: Decimal },
    IndexPrice { pair: Pair, price: Decimal },
    FundingRate(FundingRate),
    OpenInterest { pair: Pair, value: Decimal },
}
```

## Phase 6: Trading Engine Integration

### 6.1 Futures Position Tracking

**Location**: `src/trading_engine/src/state/state.rs`

Extend `TradingState` to track futures positions:

```rust
pub struct TradingState {
    // ... existing fields ...
    
    /// Active futures positions
    futures_positions: HashMap<Pair, FuturesPosition>,
    
    /// Futures orders
    futures_orders: HashMap<OrderId, Order>,
}
```

### 5.2 Futures Strategy Support

**Location**: `src/trading_engine/src/strategies/`

- Strategies can check `instrument_type` to determine if they're trading futures
- Strategies can access futures-specific data (funding rate, mark price, leverage)
- Risk manager can check futures-specific limits (maintenance margin, liquidation price)

### 5.3 Futures Level Support (Future Enhancement)

- Levels can be configured for futures instruments
- Level executor can place futures orders
- Position management for futures levels

## Phase 6: Asset Engine Integration

### 6.1 Futures Trading Rules

**Location**: `src/asset_engine/src/service.rs`

Extend `AssetService` to fetch futures trading rules:

```rust
impl AssetService {
    // ... existing methods ...
    
    pub async fn get_futures_trading_rules(&self, pair: &Pair) -> Result<FuturesTradingRules, EngineError> {
        // Fetch futures-specific rules (leverage limits, margin requirements, etc.)
    }
}
```

## Phase 7: Configuration & Runner Integration

### 7.1 Configuration Updates

**Location**: `config/bot.json`

Futures exchanges can be configured alongside spot exchanges. The exact configuration format depends on the exchange-specific implementation (see exchange-specific proposals).

### 7.2 Runner Updates

**Location**: `src/runner/src/lib.rs`

- Initialize futures exchange drivers alongside spot drivers
- Pass futures clients to trading engine
- Support both spot and futures strategies simultaneously

## Implementation Phases

### Phase 1: Foundation (Week 1-2)
- [ ] Create futures types in `trading_core`
- [ ] Add trait extensions with default implementations
- [ ] Update error types
- [ ] Add unit tests for new types

### Phase 2: Exchange Engine Integration (Week 2-3)
- [ ] Extend `ExchangeState` for futures
- [ ] Add futures state management methods
- [ ] Implement futures reconciliation
- [ ] Update state publisher for futures events

### Phase 3: Market Data Engine Integration (Week 3-4)
- [ ] Extend `MarketDataState` for futures market data
- [ ] Add mark price, index price, funding rate tracking
- [ ] Update market data publisher
- [ ] Stream futures market data

### Phase 4: Trading Engine Integration (Week 4-5)
- [ ] Extend `TradingState` for futures positions
- [ ] Update risk manager for futures-specific checks
- [ ] Add futures order management
- [ ] Update strategies to support futures (optional)

### Phase 5: Asset Engine Integration (Week 5-6)
- [ ] Extend `AssetService` for futures trading rules
- [ ] Add futures-specific rule fetching
- [ ] Update rule validation for futures

### Phase 6: Testing & Documentation (Week 6-7)
- [ ] Unit tests for all futures types
- [ ] Integration tests for futures state management
- [ ] Update documentation
- [ ] Error handling and edge cases

## Testing Strategy

### Unit Tests
- All new types (Position, Collateral, Margin, etc.)
- Trait default implementations
- Type conversions and serialization

### Integration Tests
- Binance Futures testnet connection
- Order placement and cancellation
- Position management
- Leverage and margin operations
- WebSocket stream handling

### End-to-End Tests
- Full futures trading flow (place order → fill → position update)
- Reconciliation after disconnection
- Error recovery scenarios

## Migration Path

1. **No Breaking Changes**: All existing spot trading continues to work
2. **Gradual Adoption**: Futures features can be enabled per-exchange
3. **Backward Compatible**: Old configs work without futures configuration
4. **Optional Features**: Strategies can opt-in to futures support

## Risk Mitigation

1. **Testnet First**: All development on Binance testnet
2. **Feature Flags**: Futures features behind configuration flags
3. **Separate Drivers**: Futures bugs don't affect spot trading
4. **Comprehensive Testing**: Extensive test coverage before production
5. **Gradual Rollout**: Enable futures for one pair at a time

## Success Criteria

- [ ] All futures types defined and tested
- [ ] Trait extensions with default implementations working
- [ ] Exchange Engine supports futures state management
- [ ] Market Data Engine supports futures market data
- [ ] Trading Engine supports futures positions and orders
- [ ] Asset Engine supports futures trading rules
- [ ] No regressions in spot trading
- [ ] Documentation complete
- [ ] Ready for exchange-specific implementations (Binance, Bybit, etc.)

## Future Enhancements

1. **Exchange-Specific Implementations**: Binance USD Futures, Bybit Futures, etc. (see separate proposals)
2. **Futures-Specific Strategies**: Strategies optimized for futures (funding rate arbitrage, etc.)
3. **Advanced Order Types**: 
   - OCO (One-Cancels-Other) orders
   - TWAP (Time-Weighted Average Price) orders
   - VWAP (Volume-Weighted Average Price) orders
   - Iceberg orders (display quantity)
   - Conditional orders (complex stop-loss/take-profit combinations)
4. **Cross-Exchange Futures**: Trade futures across multiple exchanges
5. **Futures Risk Management**: Advanced liquidation protection, margin call handling, auto-deleveraging (ADL) awareness
6. **Advanced Market Data**: 
   - Full orderbook depth snapshots
   - Aggregated trades for volume analysis
   - Kline/candlestick streaming
   - 24hr ticker statistics
   - Basis (spot-futures spread) tracking

## Decisions Made

1. **Position Mode Default**: One-way mode (simpler, recommended for most use cases)
2. **Margin Type Default**: Isolated margin (better risk control, recommended)
3. **Leverage Limits**: Yes, configurable maximum leverage enforcement
4. **Funding Rate Handling**: Optional, strategy-specific (strategies can opt-in)
5. **Liquidation Protection**: Yes, as risk management feature (future enhancement)

---

**Next Steps**: Review proposal, approve architecture, begin Phase 1 implementation.

