# Proposal: Binance USD Futures Integration

**Version**: 1.1  
**Date**: December 2024  
**Status**: Structure Complete (Implementation Pending)  
**Prerequisites**: Proposal 01 (Futures Integration) must be completed first

## Current Status

**Directory structure has been created** to match Binance Spot structure:
- ✅ All directories and module files in place
- ✅ `common/` directory with `endpoints.rs`, `rate_limit.rs`, `types.rs`
- ✅ `rest/` directory with `client.rs`, `sign.rs`, `utils.rs`, `types/`
- ✅ `websocket/` directory with `client.rs`, `streaming_client.rs`, `ws_client.rs`, `sign.rs`, `types/`
- ✅ `event_loop.rs` created (placeholder - simpler than Spot since no RPC)
- ✅ `driver.rs` and `config.rs` created
- ✅ All `mod.rs` files updated

**Note**: Binance Futures does **NOT** support RPC WebSocket trading. All trading operations must be done via REST API. WebSocket is only used for streaming user data updates (positions, balances, order fills).

## Executive Summary

This proposal details the implementation of Binance USD Futures support in the Exchange Engine, Market Data Engine, and Asset Engine. **Binance is an exception** - it uses completely separate API base URLs and endpoint structures for spot vs futures (`api.binance.com` vs `fapi.binance.com`), requiring a **separate driver implementation**. This is different from most exchanges (like Bybit) which use unified APIs where a single driver handles both spot and futures.

## Objectives

1. **Create Binance USD Futures driver** in Exchange Engine (REST + WebSocket)
   - **Note**: Separate driver is required because Binance uses separate API URLs
   - This is an exception to the general rule of unified drivers
2. **Implement futures market data streams** in Market Data Engine (mark price, funding rate)
3. **Add futures trading rules** support in Asset Engine
4. **Maintain consistency** with existing Binance Spot implementation patterns
5. **Support all futures operations**: positions, leverage, margin, orders, balance
6. **Implement futures methods** from `TradingClient` trait (same trait, separate implementation)

## Key Differences from Binance Spot

| Aspect | Binance Spot | Binance USD Futures |
|--------|--------------|-------------------|
| REST Base URL | `https://api.binance.com/api/v3/` | `https://fapi.binance.com/fapi/v1/` |
| WebSocket RPC URL | `wss://ws-api.binance.com:443/ws-api/v3` | **NOT SUPPORTED** - No RPC WebSocket |
| Market Data WS | `wss://stream.binance.com:9443/ws` | `wss://fstream.binance.com/stream` |
| API Version | v3 | v1/v2 (mixed) |
| Authentication | HMAC-SHA256 | HMAC-SHA256 (same) |
| Position Model | Derived from balances | Explicit positions |
| Order Types | Basic (Limit, Market) | Advanced (Stop, Take Profit, Trailing Stop) |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│              Binance USD Futures Driver                       │
│  ┌──────────────────┐  ┌──────────────────┐                │
│  │ REST Client      │  │ WebSocket Client  │                │
│  │ (fapi.binance)   │  │ (fstream.binance) │                │
│  └──────────────────┘  └──────────────────┘                │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Exchange Engine                                 │
│  ┌────────────────────────────────────────────────────┐    │
│  │ ExchangeStateManager (futures state)                │    │
│  │  - Positions                                        │    │
│  │  - Futures Balance                                 │    │
│  │  - Leverage Info                                   │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Market Data Engine                              │
│  ┌────────────────────────────────────────────────────┐    │
│  │ MarketDataStateManager (futures data)               │    │
│  │  - Mark Price                                       │    │
│  │  - Index Price                                     │    │
│  │  - Funding Rate                                    │    │
│  │  - Open Interest                                   │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Asset Engine                                    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ AssetService (futures rules)                       │    │
│  │  - Leverage Limits                                 │    │
│  │  - Margin Requirements                            │    │
│  │  - Position Limits                                │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## Phase 1: Exchange Engine - Binance Futures Driver

### 1.1 Directory Structure

**Note**: Structure mirrors Binance Spot exactly, but **Binance Futures does NOT support RPC WebSocket** - all trading operations are via REST API. WebSocket is only used for streaming user data.

```
src/exchange_engine/src/exchanges/
├── binance/              # Existing spot driver
│   ├── common/           # Shared types, rate_limit
│   ├── config.rs
│   ├── driver.rs
│   ├── event_loop.rs     # WebSocket coordination (RPC + streaming)
│   ├── rest/             # REST client, sign, utils
│   └── websocket/        # WebSocket client (RPC + streaming)
│
└── binance_futures/      # New futures driver (same structure)
    ├── common/           # Shared types, rate_limit
    │   ├── rate_limit.rs # Reuses BinanceRateLimiter
    │   ├── types.rs      # Common futures types
    │   └── mod.rs
    ├── config.rs
    ├── driver.rs
    ├── event_loop.rs     # WebSocket coordination (streaming only, no RPC)
    ├── rest/
    │   ├── client.rs     # BinanceFuturesHttpClient
    │   ├── rest_client.rs
    │   ├── sign.rs       # HMAC-SHA256 authentication
    │   ├── utils.rs
    │   ├── types/
    │   │   ├── position.rs
    │   │   ├── balance.rs
    │   │   ├── leverage.rs
    │   │   ├── order.rs
    │   │   ├── account.rs
    │   │   ├── common.rs
    │   │   ├── error.rs
    │   │   └── mod.rs
    │   └── mod.rs
    └── websocket/
        ├── client.rs           # Basic WebSocket client
        ├── sign.rs             # WebSocket auth (listenKey-based)
        ├── streaming_client.rs # User data stream handler
        ├── ws_client.rs        # High-level WebSocket wrapper
        ├── types/
        │   ├── common.rs
        │   ├── user_data/
        │   │   └── mod.rs
        │   └── mod.rs
        └── mod.rs
```

**Key Differences from Spot**:
- **No RPC WebSocket**: Futures does not support RPC-style WebSocket trading
- **No stream splitting**: WebSocket is simpler - only handles user data streams
- **All trading via REST**: Order placement, cancellation, position management all via REST API
- **ListenKey-based auth**: User data streams use listenKey (created via REST), not session.logon
- **Same structure otherwise**: Reuses same patterns, types, and organization

### 1.2 Configuration

**Location**: `src/exchange_engine/src/exchanges/binance_futures/config.rs`

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BinanceFuturesConfig {
    pub rest: BinanceFuturesRestConfig,
    pub websocket: BinanceFuturesWebSocketConfig,
    pub testnet: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BinanceFuturesRestConfig {
    pub api_key: String,
    pub secret_key: String,
    #[serde(default = "default_rest_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BinanceFuturesWebSocketConfig {
    #[serde(default = "default_ws_url")]
    pub base_url: String,
}

fn default_rest_url() -> String {
    "https://fapi.binance.com".to_string()
}

fn default_ws_url() -> String {
    "wss://fstream.binance.com".to_string()
}
```

### 1.3 REST Client Implementation

**Location**: `src/exchange_engine/src/exchanges/binance_futures/rest/client.rs`

#### Base URL and Endpoints

```rust
const BINANCE_FUTURES_REST_URL: &str = "https://fapi.binance.com/fapi/v1/";
const BINANCE_FUTURES_TESTNET_URL: &str = "https://testnet.binancefuture.com/fapi/v1/";
```

#### Key REST Endpoints to Implement

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/order` | POST | Place order |
| `/order` | GET | Query order |
| `/order` | DELETE | Cancel order |
| `/openOrders` | GET | Get all open orders |
| `/openOrders` | DELETE | Cancel all open orders |
| `/positionRisk` | GET | Get position information |
| `/balance` | GET | Get futures account balance |
| `/leverage` | POST | Change initial leverage |
| `/leverageBracket` | GET | Get leverage bracket |
| `/positionSide/dual` | POST | Change position mode (one-way/hedge) |
| `/marginType` | POST | Change margin type (isolated/cross) |
| `/account` | GET | Get account information |
| `/time` | GET | Get server time |

#### Implementation Checklist

- [ ] Create `BinanceFuturesHttpClient` struct
- [ ] Implement `TradingClient<OrderRequest, Pair>` trait
- [ ] Implement all futures methods from `TradingClient` (fetch_positions, set_leverage, etc.)
- [ ] Implement order placement (`POST /fapi/v1/order`)
- [ ] Implement order cancellation (`DELETE /fapi/v1/order`)
- [ ] Implement position fetching (`GET /fapi/v2/positionRisk`)
- [ ] Implement balance fetching (`GET /fapi/v2/balance`)
- [ ] Implement leverage management (`POST /fapi/v1/leverage`)
- [ ] Implement position mode setting (`POST /fapi/v1/positionSide/dual`)
- [ ] Implement margin type setting (`POST /fapi/v1/marginType`)
- [ ] Add rate limiting (reuse Binance rate limiter pattern)
- [ ] Add authentication (reuse Binance HMAC-SHA256 pattern)
- [ ] Handle testnet vs production URLs

### 1.4 WebSocket Client Implementation

**Location**: `src/exchange_engine/src/exchanges/binance_futures/websocket/`

**Important**: Binance Futures does **NOT** support RPC WebSocket like Spot. All trading operations (order placement, cancellation) are done via REST API. WebSocket is **only** used for streaming user data updates.

#### WebSocket URLs

- **User Data Stream**: `wss://fstream.binance.com/ws/<listenKey>`
  - ListenKey is created/renewed via REST API (`POST /fapi/v1/listenKey`, `PUT /fapi/v1/listenKey`)
  - No authentication on WebSocket connection itself (auth is via listenKey)

#### Key WebSocket Streams (User Data Only)

1. **User Data Stream** (authenticated via listenKey):
   - `ACCOUNT_UPDATE` - Balance and position updates
   - `ORDER_TRADE_UPDATE` - Order and trade updates
   - `MARGIN_CALL` - Margin call warnings

**Note**: Mark price and funding rate streams are handled by Market Data Engine, not Exchange Engine.

#### Implementation Checklist

- [ ] Create `BinanceFuturesWsClient` struct (in `websocket/ws_client.rs`)
- [ ] Create `BinanceFuturesStreamingClient` struct (in `websocket/streaming_client.rs`)
- [ ] Create `BinanceFuturesEventLoop` struct (in `event_loop.rs`) - simpler than Spot (no RPC coordination)
- [ ] Implement user data stream connection using listenKey
- [ ] Handle `ACCOUNT_UPDATE` events (balance, position updates)
- [ ] Handle `ORDER_TRADE_UPDATE` events (order fills, position changes)
- [ ] Handle `MARGIN_CALL` events (optional, for monitoring)
- [ ] Implement listen key management (create, renew every 60min, delete)
- [ ] Add reconnection logic with exponential backoff
- [ ] Add heartbeat/ping-pong handling
- [ ] Parse and convert Binance Futures messages to core types
- [ ] Update `ExchangeStateManager` on position/balance/order updates

### 1.5 Type Definitions

**Location**: `src/exchange_engine/src/exchanges/binance_futures/rest/types/`

#### Position Response

```rust
// GET /fapi/v2/positionRisk
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesPositionRisk {
    pub symbol: String,
    pub position_amt: Decimal,           // Position size
    pub entry_price: Decimal,              // Entry price
    pub mark_price: Decimal,              // Mark price
    pub un_realized_profit: Decimal,      // Unrealized P&L
    pub liquidation_price: Decimal,        // Liquidation price
    pub leverage: String,                 // Leverage (as string)
    pub max_notional_value: Decimal,      // Max notional
    pub margin_type: String,              // "isolated" or "cross"
    pub isolated_margin: Decimal,         // Isolated margin
    pub position_side: String,            // "BOTH", "LONG", "SHORT"
    pub notional: Decimal,                 // Position notional
    pub isolated_wallet: Decimal,         // Isolated wallet
    pub update_time: u64,                  // Update timestamp
}
```

#### Balance Response

```rust
// GET /fapi/v2/balance
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesBalance {
    pub asset: String,
    pub balance: Decimal,                 // Wallet balance
    pub cross_wallet_balance: Decimal,     // Cross wallet balance
    pub cross_un_pnl: Decimal,             // Cross unrealized P&L
    pub available_balance: Decimal,         // Available balance
    pub max_withdraw_amount: Decimal,       // Max withdraw
    pub margin_available: bool,             // Margin available
    pub update_time: u64,                  // Update timestamp
}
```

#### Order Request/Response

```rust
// POST /fapi/v1/order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesOrderRequest {
    pub symbol: String,
    pub side: String,                      // "BUY" or "SELL"
    #[serde(rename = "type")]
    pub order_type: String,                // "LIMIT", "MARKET", "STOP", etc.
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub time_in_force: Option<String>,     // "GTC", "IOC", "FOK"
    pub reduce_only: Option<String>,        // "true" or "false"
    pub close_position: Option<String>,     // "true" or "false"
    pub position_side: Option<String>,      // "BOTH", "LONG", "SHORT"
    pub stop_price: Option<Decimal>,        // For stop orders
    pub working_type: Option<String>,       // "MARK_PRICE" or "CONTRACT_PRICE"
    pub price_protect: Option<String>,      // "true" or "false"
    pub new_order_resp_type: Option<String>, // "ACK", "RESULT", "FULL"
    pub recv_window: Option<u64>,
    pub timestamp: u64,
}
```

### 1.6 Driver Implementation

**Location**: `src/exchange_engine/src/exchanges/binance_futures/driver.rs`

The driver should:
- Initialize REST and WebSocket clients
- Manage WebSocket connections
- Handle reconnection logic
- Publish updates to Exchange Engine state manager
- Implement `ExchangeDriver` trait

## Phase 2: Market Data Engine - Binance Futures Streams

### 2.1 Market Data WebSocket Client

**Location**: `src/market_data_engine/src/exchanges/binance_futures/`

#### Streams to Implement

1. **Mark Price Stream**
   - URL: `wss://fstream.binance.com/ws/<symbol>@markPrice@1s`
   - Updates: Mark price every 1 second (or 3 seconds)
   - Data: `{ "e": "markPriceUpdate", "s": "BTCUSDT", "p": "50000", "r": "0.0001", "T": 1234567890 }`

2. **Funding Rate Stream**
   - URL: `wss://fstream.binance.com/ws/<symbol>@markPrice@1s` (same stream as mark price)
   - Updates: Funding rate every 8 hours (or on change)
   - Data: Included in mark price stream

3. **Index Price Stream** (optional)
   - URL: `wss://fstream.binance.com/ws/<symbol>@indexPrice@1s`
   - Updates: Index price every 1 second

4. **Open Interest Stream** (optional)
   - URL: `wss://fstream.binance.com/ws/<symbol>@openInterest`
   - Updates: Open interest changes

#### Implementation Checklist

- [ ] Create `BinanceFuturesMarketDataClient` struct
- [ ] Implement `MarketDataClient<Pair>` trait
- [ ] Implement `FuturesMarketDataClient` trait
- [ ] Connect to mark price stream
- [ ] Connect to funding rate stream
- [ ] Parse mark price updates
- [ ] Parse funding rate updates
- [ ] Convert to core `FundingRate` type
- [ ] Publish updates to `MarketDataStateManager`
- [ ] Add reconnection logic
- [ ] Support combined streams (multiple symbols)

### 2.2 Market Data State Updates

**Location**: `src/market_data_engine/src/state/state.rs`

The `MarketDataState` should already be extended (from Proposal 02) to include:
- `mark_prices: HashMap<Pair, Decimal>`
- `index_prices: HashMap<Pair, Decimal>`
- `funding_rates: HashMap<Pair, FundingRate>`
- `open_interest: HashMap<Pair, Decimal>`

The Binance Futures market data client will update these fields.

## Phase 3: Asset Engine - Binance Futures Trading Rules

### 3.1 Futures Trading Rules Client

**Location**: `src/asset_engine/src/exchanges/binance_futures/`

#### REST Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/exchangeInfo` | GET | Get exchange information (symbols, filters, limits) |
| `/leverageBracket` | GET | Get leverage bracket (max leverage per symbol) |
| `/premiumIndex` | GET | Get funding rate and mark price |
| `/markPriceKlines` | GET | Get mark price klines (optional) |

#### Key Data to Fetch

1. **Symbol Information** (`/exchangeInfo`):
   - Symbol name
   - Base asset, quote asset
   - Price filters (tick size, min price, max price)
   - Lot size filters (step size, min quantity, max quantity)
   - Min notional
   - **Futures-specific**: Contract size, margin asset

2. **Leverage Bracket** (`/leverageBracket`):
   - Max leverage per symbol
   - Initial margin rate
   - Maintenance margin rate
   - Notional caps per bracket

3. **Funding Rate** (`/premiumIndex`):
   - Current funding rate
   - Next funding time
   - Mark price
   - Index price

#### Implementation Checklist

- [ ] Create `BinanceFuturesPublicClient` struct
- [ ] Implement futures trading rules fetching
- [ ] Parse `/exchangeInfo` response
- [ ] Parse `/leverageBracket` response
- [ ] Convert to core `FuturesTradingRules` type
- [ ] Cache rules (similar to spot asset service)
- [ ] Add periodic refresh logic
- [ ] Handle testnet vs production

### 3.2 Asset Service Integration

**Location**: `src/asset_engine/src/service.rs`

Extend `AssetService` to support Binance Futures:

```rust
impl AssetService {
    // ... existing methods ...
    
    pub async fn get_futures_trading_rules(
        &self,
        exchange: Exchange,
        pair: &Pair,
    ) -> Result<FuturesTradingRules, EngineError> {
        match exchange {
            Exchange::BinanceFutures => {
                // Use BinanceFuturesPublicClient
                self.binance_futures_client.get_futures_rules(pair).await
            }
            _ => Err(EngineError::NotSupported(
                format!("Futures not supported for exchange: {:?}", exchange)
            )),
        }
    }
}
```

## Phase 4: Integration with Exchange Engine State

### 4.1 State Manager Updates

**Location**: `src/exchange_engine/src/state/manager.rs`

The `ExchangeStateManager` should already support futures (from Proposal 02). The Binance Futures driver will:

1. **On Position Update** (from WebSocket):
   ```rust
   state_manager.update_position(futures_position).await;
   ```

2. **On Balance Update** (from WebSocket):
   ```rust
   state_manager.update_futures_balance(futures_balance).await;
   ```

3. **On Reconciliation**:
   ```rust
   // Fetch positions from REST
   let positions = futures_client.fetch_positions().await?;
   for position in positions {
       state_manager.update_position(position).await;
   }
   
   // Fetch balance from REST
   let balances = futures_client.fetch_futures_balance().await?;
   for balance in balances {
       state_manager.update_futures_balance(balance).await;
   }
   ```

### 4.2 State Publisher Updates

**Location**: `src/exchange_engine/src/state/publisher.rs`

The publisher should already support futures events (from Proposal 02). The Binance Futures driver will publish:

- `ExchangeUpdate::Position(FuturesPosition)`
- `ExchangeUpdate::FuturesBalance(FuturesBalance)`
- `ExchangeUpdate::Leverage(LeverageInfo)`

## Phase 5: Configuration & Runner Integration

### 5.1 Configuration Format

**Location**: `config/bot.json`

```json
{
  "exchanges": {
    "binance_spot": {
      "enabled": true,
      "rest": {
        "api_key": "...",
        "secret_key": "..."
      },
      "testnet": false
    },
    "binance_futures": {
      "enabled": true,
      "rest": {
        "api_key": "...",
        "secret_key": "..."
      },
      "websocket": {
        "base_url": "wss://fstream.binance.com"
      },
      "testnet": false,
      "default_position_mode": "OneWay",
      "default_margin_type": "Isolated",
      "max_leverage": 10
    }
  }
}
```

### 5.2 Runner Updates

**Location**: `src/runner/src/lib.rs`

- Initialize `BinanceFuturesDriver` alongside `BinanceDriver`
- Pass futures clients to trading engine
- Support both spot and futures strategies simultaneously
- Initialize futures market data client
- Initialize futures asset service client

## Implementation Phases & User Action Items

### Phase 1: REST Client (Week 1-2)

**Developer Tasks:**
- [ ] Create `binance_futures` module structure
- [ ] Implement configuration
- [ ] Implement REST client base
- [ ] Implement order placement and cancellation
- [ ] Implement position and balance fetching
- [ ] Implement leverage and margin management
- [ ] Add integration tests with testnet

**User Action Items:**
- [ ] **Provide Binance Futures Testnet API Credentials**
  - Create testnet account at https://testnet.binancefuture.com/
  - Generate API key and secret key
  - Provide credentials to developer (store securely, never commit to git)
  - Ensure API key has trading permissions enabled
- [ ] **Review Phase 1 Implementation**
  - Review REST client implementation
  - Test order placement/cancellation via testnet
  - Verify position and balance fetching works correctly
  - Approve before moving to Phase 2
- [ ] Create `binance_futures` module structure
- [ ] Implement configuration
- [ ] Implement REST client base
- [ ] Implement order placement and cancellation
- [ ] Implement position and balance fetching
- [ ] Implement leverage and margin management
- [ ] Add integration tests with testnet

### Phase 2: WebSocket Client (Week 2-3)

**Developer Tasks:**
- [ ] Implement WebSocket client base (no RPC - simpler than Spot)
- [ ] Implement listen key management (REST API calls)
- [ ] Implement user data stream connection
- [ ] Handle position and balance updates (`ACCOUNT_UPDATE`)
- [ ] Handle order and trade updates (`ORDER_TRADE_UPDATE`)
- [ ] Implement event loop (streaming only, no RPC coordination needed)
- [ ] Add reconnection logic
- [ ] Add integration tests

**User Action Items:**
- [ ] **Test WebSocket Connectivity**
  - Verify user data stream connects successfully
  - Monitor logs for `ACCOUNT_UPDATE` and `ORDER_TRADE_UPDATE` events
  - Confirm position and balance updates are received in real-time
  - Test reconnection after network interruption
- [ ] **Review Phase 2 Implementation**
  - Verify event loop handles streaming correctly
  - Confirm no RPC dependencies (as expected for Futures)
  - Approve before moving to Phase 3

### Phase 3: Market Data Integration (Week 3-4)

**Developer Tasks:**
- [ ] Create Binance Futures market data client
- [ ] Implement mark price stream
- [ ] Implement funding rate stream
- [ ] Update market data state manager
- [ ] Add integration tests

**User Action Items:**
- [ ] **Verify Market Data Streams**
  - Confirm mark price updates are received (should update every 1-3 seconds)
  - Verify funding rate data is correct (updates every 8 hours or on change)
  - Check that market data appears in Market Data Engine state
  - Test with multiple symbols/pairs simultaneously
- [ ] **Review Phase 3 Implementation**
  - Verify mark price and funding rate streams integrate correctly
  - Check that data is available to trading strategies
  - Approve before moving to Phase 4

### Phase 4: Asset Engine Integration (Week 4-5)

**Developer Tasks:**
- [ ] Create Binance Futures public client
- [ ] Implement trading rules fetching
- [ ] Implement leverage bracket fetching
- [ ] Integrate with AssetService
- [ ] Add caching and refresh logic

**User Action Items:**
- [ ] **Verify Asset Engine Integration**
  - Confirm trading rules are fetched correctly (symbol info, filters, limits)
  - Verify leverage brackets are retrieved and cached
  - Check that AssetService enforces leverage and margin requirements
  - Test rule refresh logic (rules should update when changed on exchange)
- [ ] **Review Phase 4 Implementation**
  - Verify asset rules are correctly enforced
  - Confirm caching reduces unnecessary API calls
  - Approve before moving to Phase 5

### Phase 5: Testing & Documentation (Week 5-6)

**Developer Tasks:**
- [ ] End-to-end testing with Binance testnet
- [ ] Test all futures operations
- [ ] Test reconciliation
- [ ] Test error handling
- [ ] Update documentation
- [ ] Performance testing

**User Action Items:**
- [ ] **Comprehensive Testing**
  - Execute end-to-end trading flow: place order → fill → position update
  - Test multiple simultaneous positions
  - Test leverage changes and margin type changes
  - Test position mode changes (one-way vs hedge mode)
  - Verify reconciliation works correctly (state matches exchange)
  - Test error recovery (network failures, rate limits, etc.)
- [ ] **Review Documentation**
  - Review all updated documentation
  - Verify examples are correct
  - Check configuration documentation is complete
- [ ] **Production Preparation**
  - Decide on production API credentials (when ready for live trading)
  - Review security considerations (API key storage, listen key rotation)
  - Set up monitoring and alerting for production
  - Define rollback plan if issues arise
- [ ] **Final Approval**
  - Sign off on complete implementation
  - Approve for testnet deployment
  - Approve for production deployment (when ready)

## Testing Strategy

### Unit Tests
- Type serialization/deserialization
- Request/response parsing
- Error handling

### Integration Tests (Testnet)
- Order placement and cancellation
- Position management
- Balance tracking
- Leverage and margin operations
- WebSocket stream handling
- Reconciliation

### End-to-End Tests
- Full trading flow (place order → fill → position update)
- Multiple positions
- Leverage changes
- Margin type changes
- Position mode changes
- Error recovery

## Error Handling

### Common Binance Futures Errors

- `-1003`: Too many requests (rate limit)
- `-1021`: Timestamp out of sync
- `-1022`: Invalid signature
- `-2010`: New order rejected (insufficient balance, invalid price, etc.)
- `-2011`: Unknown order sent
- `-2013`: No such order
- `-2019`: Margin insufficient
- `-4046`: No need to change margin type
- `-4047`: No need to change position side

### Handling Strategy

- Retry with exponential backoff for transient errors
- Log all errors with context
- Return appropriate `EngineError` variants
- Handle rate limits gracefully

## Rate Limiting

Binance Futures has different rate limits than Spot:
- **REST API**: Weight-based (similar to spot)
- **WebSocket**: Connection-based limits
- **Order Rate**: 300 orders per 10 seconds per symbol

Reuse the existing `BinanceRateLimiter` pattern but with futures-specific limits.

## Security Considerations

1. **API Keys**: Store securely, never log
2. **Listen Keys**: Rotate every 60 minutes (Binance requirement)
3. **Signature**: Always use server time for timestamp
4. **Testnet**: Use testnet for all development
5. **Rate Limiting**: Respect all rate limits

## Success Criteria

- [ ] All REST endpoints implemented and tested
- [ ] WebSocket streams working (user data, mark price, funding rate)
- [ ] Positions tracked correctly in ExchangeState
- [ ] Balance tracked correctly
- [ ] Leverage and margin management working
- [ ] Market data (mark price, funding rate) streaming
- [ ] Trading rules fetched and cached
- [ ] Reconciliation working for all futures data
- [ ] No regressions in spot trading
- [ ] Documentation complete
- [ ] Ready for production use (after testnet validation)

## Future Enhancements

1. **Advanced Order Types**: Stop-loss, take-profit, trailing stop
2. **Multi-Asset Mode**: Support for COIN-M futures (in addition to USDT-M)
3. **Portfolio Margin**: Support for portfolio margin mode
4. **Risk Management**: Automatic position reduction near liquidation
5. **Funding Rate Arbitrage**: Strategy support for funding rate trading

---

**Next Steps**: 
1. Complete Proposal 02 (Futures Integration) first
2. Review and approve this proposal
3. Begin Phase 1 implementation (REST Client)

