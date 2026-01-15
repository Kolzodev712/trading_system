# Proposal: Bybit Integration (Spot & Futures)

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft  
**Prerequisites**: Proposal 02 (Futures Integration) must be completed first

## Executive Summary

This proposal details the implementation of Bybit support for both Spot and Futures trading in the Exchange Engine, Market Data Engine, and Asset Engine. Bybit uses a **unified API** (`api.bybit.com/v5/`) for both spot and futures, allowing a **single driver** to handle both instrument types. This is the standard pattern for most exchanges (unlike Binance which requires separate drivers due to separate API URLs).

## Objectives

1. **Create single Bybit driver** in Exchange Engine (REST + WebSocket) that handles both spot and futures
2. **Implement futures methods** in the same `TradingClient` implementation (differentiated by `InstrumentType`)
3. **Implement market data streams** for both spot and futures in Market Data Engine
4. **Add trading rules support** in Asset Engine for both spot and futures
5. **Support unified account** model (shared balance, separate positions)
6. **Maintain consistency** with existing patterns while demonstrating unified driver approach

## Key Differences from Binance

| Aspect | Binance | Bybit |
|--------|---------|-------|
| **Account Model** | Separate accounts (spot vs futures) | Unified account (spot + futures) |
| **REST Base URL (Spot)** | `https://api.binance.com/api/v3/` | `https://api.bybit.com/v5/` |
| **REST Base URL (Futures)** | `https://fapi.binance.com/fapi/v1/` | `https://api.bybit.com/v5/` (same URL) |
| **WebSocket URL** | Different for spot/futures | `wss://stream.bybit.com/v5/` (unified) |
| **API Version** | v3 (spot), v1/v2 (futures) | v5 (unified) |
| **Authentication** | HMAC-SHA256 | HMAC-SHA256 (different param order) |
| **Request Format** | Query params | Query params (similar) |
| **Response Format** | Direct JSON | Wrapped in `result` object |
| **Rate Limiting** | Weight-based | Request-based (different limits) |
| **Order Types** | Standard | More advanced (conditional orders, etc.) |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│              Bybit Unified Driver                            │
│  ┌────────────────────────────────────────────────────┐    │
│  │ BybitClient (single driver)                        │    │
│  │  - Implements TradingClient                        │    │
│  │  - Handles both spot AND futures                   │    │
│  │  - Differentiated by InstrumentType in orders      │    │
│  │  - Same REST + WebSocket connection                │    │
│  │  - (api.bybit.com/v5/)                             │    │
│  └────────────────────────────────────────────────────┘    │
│                  │                                           │
│                  ▼                                           │
│         Unified Account Model                                 │
│         (shared balance, separate positions)                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Exchange Engine                                 │
│  ┌────────────────────────────────────────────────────┐    │
│  │ ExchangeStateManager                               │    │
│  │  - Spot Orders & Balances                          │    │
│  │  - Futures Positions & Balance                     │    │
│  │  - Unified Account Info                            │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Market Data Engine                              │
│  ┌────────────────────────────────────────────────────┐    │
│  │ MarketDataStateManager                             │    │
│  │  - Spot Orderbooks & Trades                        │    │
│  │  - Futures Mark Price & Funding Rate              │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Asset Engine                                    │
│  ┌────────────────────────────────────────────────────┐    │
│  │ AssetService                                        │    │
│  │  - Spot Trading Rules                              │    │
│  │  - Futures Trading Rules                           │    │
│  │  - Unified Account Limits                          │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## Phase 1: Exchange Engine - Bybit Unified Driver

### 1.1 Directory Structure

```
src/exchange_engine/src/exchanges/
├── binance/              # Existing (spot only)
├── binance_futures/      # Existing (futures only, separate API)
└── bybit/                # New (unified: spot + futures)
    ├── mod.rs
    ├── config.rs
    ├── driver.rs
    ├── rest/
    │   ├── client.rs      # Single REST client for both spot and futures
    │   └── types/
    │       ├── order.rs   # Handles both spot and futures orders
    │       ├── balance.rs  # Unified balance
    │       ├── position.rs # Futures positions
    │       ├── leverage.rs # Futures leverage
    │       ├── common.rs
    │       └── mod.rs
    ├── websocket/
    │   ├── client.rs      # Single WebSocket client
    │   └── types/
    │       ├── orderbook.rs
    │       ├── trade.rs
    │       ├── position_update.rs
    │       ├── funding_rate.rs
    │       └── mod.rs
    ├── common/
    │   ├── auth.rs
    │   ├── rate_limit.rs
    │   ├── sign.rs
    │   └── utils.rs
    └── unified/
        └── account.rs      # Unified account management
```

**Key Point**: Single `BybitClient` implements `TradingClient` with both spot and futures methods. Differentiation is done via `InstrumentType` in `OrderRequest`.

### 1.2 Configuration

**Location**: `src/exchange_engine/src/exchanges/bybit/config.rs`

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BybitConfig {
    pub api_key: String,
    pub api_secret: String,
    #[serde(default = "default_rest_url")]
    pub rest_base_url: String,
    #[serde(default = "default_ws_url")]
    pub ws_base_url: String,
    pub testnet: bool,
    pub spot: BybitSpotConfig,
    pub futures: BybitFuturesConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BybitSpotConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BybitFuturesConfig {
    pub enabled: bool,
    #[serde(default = "default_position_mode")]
    pub default_position_mode: String,  // "MergedSingle" or "BothSide"
    #[serde(default = "default_margin_mode")]
    pub default_margin_mode: String,   // "REGULAR_MARGIN" or "PORTFOLIO_MARGIN"
    #[serde(default = "default_max_leverage")]
    pub max_leverage: u32,
}

fn default_rest_url() -> String {
    "https://api.bybit.com".to_string()
}

fn default_ws_url() -> String {
    "wss://stream.bybit.com".to_string()
}

fn default_position_mode() -> String {
    "MergedSingle".to_string()  // One-way mode
}

fn default_margin_mode() -> String {
    "REGULAR_MARGIN".to_string()
}

fn default_max_leverage() -> u32 {
    10
}
```

### 1.3 REST Client - Unified (Spot + Futures)

**Location**: `src/exchange_engine/src/exchanges/bybit/rest/client.rs`

**Key Design**: Single REST client handles both spot and futures. Methods check `InstrumentType` to route to correct endpoints.

#### Base URL and Endpoints

```rust
const BYBIT_REST_URL: &str = "https://api.bybit.com/v5/";
const BYBIT_TESTNET_URL: &str = "https://api-testnet.bybit.com/v5/";
```

#### Key REST Endpoints (Spot)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/spot/v3/private/order` | POST | Place order |
| `/spot/v3/private/order` | GET | Query order |
| `/spot/v3/private/order` | DELETE | Cancel order |
| `/spot/v3/private/open-orders` | GET | Get all open orders |
| `/spot/v3/private/cancel-all` | DELETE | Cancel all open orders |
| `/spot/v3/private/account` | GET | Get account balance |
| `/spot/v3/public/symbols` | GET | Get trading symbols info |
| `/v5/market/time` | GET | Get server time |

#### Bybit API Response Format

Bybit wraps all responses in a `result` object:

```json
{
  "retCode": 0,
  "retMsg": "OK",
  "result": {
    // Actual data here
  },
  "retExtInfo": {},
  "time": 1234567890000
}
```

#### Implementation Checklist

- [ ] Create `BybitHttpClient` struct (unified for spot + futures)
- [ ] Implement `TradingClient<OrderRequest, Pair>` trait
- [ ] Implement `open_order()` - route to spot or futures endpoint based on `OrderRequest.instrument_type`
- [ ] Implement `cancel_order_by_id()` - route based on order's instrument type
- [ ] Implement `fetch_balance()` - spot balance endpoint
- [ ] Implement `fetch_futures_balance()` - futures balance endpoint
- [ ] Implement `fetch_open_orders()` - route based on instrument type
- [ ] Implement all futures methods (`fetch_positions`, `set_leverage`, etc.)
- [ ] Add authentication (Bybit-specific HMAC-SHA256)
- [ ] Add rate limiting (Bybit-specific limits)
- [ ] Handle response wrapping (`result` object)
- [ ] Handle error codes (`retCode` field)
- [ ] Support testnet vs production

### 1.4 WebSocket Client - Unified

**Location**: `src/exchange_engine/src/exchanges/bybit/websocket/client.rs`

#### WebSocket URLs

- **Public Stream**: `wss://stream.bybit.com/v5/public/spot`
- **Private Stream**: `wss://stream.bybit.com/v5/private` (authenticated)

#### Key WebSocket Topics (Spot)

1. **Public Topics**:
   - `orderbook.{depth}.{symbol}` - Orderbook updates
   - `publicTrade.{symbol}` - Public trades
   - `tickers.{symbol}` - 24h ticker updates

2. **Private Topics** (authenticated):
   - `order` - Order updates
   - `execution` - Trade executions
   - `wallet` - Balance updates

#### Implementation Checklist

- [ ] Create `BybitWsClient` struct (unified for spot + futures)
- [ ] Implement `ExchangeDriver` trait
- [ ] Connect to public WebSocket stream
- [ ] Connect to private WebSocket stream (authenticated)
- [ ] Subscribe to spot orderbook/trade topics
- [ ] Subscribe to futures orderbook/trade topics
- [ ] Subscribe to position update topics (futures)
- [ ] Subscribe to funding rate topics (futures)
- [ ] Handle order updates (both spot and futures)
- [ ] Handle balance updates (unified account)
- [ ] Handle position updates (futures)
- [ ] Add reconnection logic
- [ ] Handle authentication for private stream

## Phase 2: Futures Methods Implementation

### 2.1 Futures REST Endpoints

**Location**: `src/exchange_engine/src/exchanges/bybit/rest/client.rs`

The same `BybitHttpClient` implements futures methods:

#### Key REST Endpoints (Futures)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v5/order/create` | POST | Place order |
| `/v5/order/cancel` | POST | Cancel order |
| `/v5/order/replace` | POST | Amend order |
| `/v5/order/history` | GET | Get order history |
| `/v5/position/list` | GET | Get positions |
| `/v5/account/wallet-balance` | GET | Get wallet balance |
| `/v5/account/fee-rate` | GET | Get fee rate |
| `/v5/position/set-leverage` | POST | Set leverage |
| `/v5/position/switch-mode` | POST | Switch position mode |
| `/v5/account/set-margin-mode` | POST | Set margin mode |
| `/v5/market/funding/history` | GET | Get funding rate history |
| `/v5/market/kline` | GET | Get klines (optional) |

#### Implementation Checklist

- [ ] Implement `fetch_positions()` - `GET /v5/position/list`
- [ ] Implement `fetch_position()` - `GET /v5/position/list` with symbol filter
- [ ] Implement `fetch_futures_balance()` - `GET /v5/account/wallet-balance`
- [ ] Implement `set_leverage()` - `POST /v5/position/set-leverage`
- [ ] Implement `get_leverage()` - Extract from position data
- [ ] Implement `set_position_mode()` - `POST /v5/position/switch-mode`
- [ ] Implement `get_position_mode()` - `GET /v5/position/switch-mode`
- [ ] Implement `set_margin_type()` - `POST /v5/account/set-margin-mode`
- [ ] Implement `get_margin_type()` - Extract from position data
- [ ] Implement `place_futures_order()` - Route to futures endpoint in `open_order()`
- [ ] Implement `cancel_all_futures_orders()` - `POST /v5/order/cancel-all`
- [ ] Implement `fetch_open_futures_orders()` - `GET /v5/order/realtime` with category filter

**Note**: These are all methods from `TradingClient` trait, implemented in the same `BybitHttpClient` struct.

### 2.2 WebSocket Futures Topics

**Location**: `src/exchange_engine/src/exchanges/bybit/websocket/client.rs`

The same `BybitWsClient` handles futures WebSocket topics:

#### Key WebSocket Topics (Futures)

1. **Public Topics**:
   - `orderbook.{depth}.{symbol}` - Orderbook updates
   - `publicTrade.{symbol}` - Public trades
   - `tickers.{symbol}` - 24h ticker updates
   - `kline.{interval}.{symbol}` - Kline updates
   - `liquidation.{symbol}` - Liquidation events

2. **Private Topics** (authenticated):
   - `order` - Order updates
   - `execution` - Trade executions
   - `position` - Position updates
   - `wallet` - Balance updates

3. **Futures-Specific Topics**:
   - `orderbook.200.{symbol}` - Orderbook depth 200
   - `publicTrade.{symbol}` - Public trades
   - `tickers.{symbol}` - Ticker updates

#### Implementation Checklist

- [ ] Subscribe to futures position topics (`position`)
- [ ] Subscribe to futures order topics (`order`)
- [ ] Subscribe to futures execution topics (`execution`)
- [ ] Subscribe to funding rate topics (`tickers` for funding rate)
- [ ] Handle position update events
- [ ] Handle futures order update events
- [ ] Handle futures balance update events
- [ ] Publish to ExchangeStateManager

**Note**: All handled in the same `BybitWsClient` that also handles spot topics.

## Phase 3: Common Infrastructure

### 3.1 Authentication

**Location**: `src/exchange_engine/src/exchanges/bybit/common/sign.rs`

Bybit uses HMAC-SHA256 but with different parameter ordering than Binance:

```rust
pub struct BybitAuthenticator {
    api_key: String,
    api_secret: String,
}

impl BybitAuthenticator {
    pub fn sign(&self, method: &str, path: &str, params: &BTreeMap<String, String>, timestamp: u64) -> String {
        // Build query string (sorted by key)
        let mut query_parts = Vec::new();
        query_parts.push(format!("api_key={}", self.api_key));
        query_parts.push(format!("timestamp={}", timestamp));
        
        for (key, value) in params.iter() {
            query_parts.push(format!("{}={}", key, value));
        }
        
        let query_string = query_parts.join("&");
        
        // Sign with HMAC-SHA256
        let signature = hmac_sha256(&self.api_secret, &query_string);
        hex::encode(signature)
    }
}
```

### 3.2 Rate Limiting

**Location**: `src/exchange_engine/src/exchanges/bybit/common/rate_limit.rs`

**Status**: ✅ Rate limiter structure implemented. See `src/exchange_engine/src/exchanges/bybit/common/rate_limit.rs`

Bybit uses a **two-level rate limiting system**:
1. **IP-based HTTP limit**: 600 requests per 5 seconds (applies to ALL REST calls)
2. **Per-endpoint limits**: Each endpoint has its own limit (rolling window per second)

**Rate Limit Algorithm**: Rolling window (not fixed window like Binance)
- Uses `RollingWindowRateLimiter` from `trading_core`
- 1-second rolling windows for endpoints
- 5-second rolling window for IP limit

**Default Limits** (from Bybit documentation):
- IP HTTP: 600 requests per 5 seconds
- Common endpoints (default tier):
  - `/v5/order/create`: 10/s
  - `/v5/order/amend`: 10/s
  - `/v5/order/cancel`: 10/s
  - `/v5/order/cancel-all`: 1/s
  - `/v5/order/create-batch`: 10/s
  - `/v5/order/history`: 50/s
  - `/v5/order/realtime`: 50/s
  - `/v5/position/list`: 50/s
  - `/v5/account/wallet-balance`: 50/s
- Default fallback: 10/s for unknown endpoints

**Configuration**: `BybitRateLimiterConfig` in `config.rs`
- `endpoint_limits`: HashMap of endpoint -> limit per second
- `default_endpoint_limit`: Fallback limit (default: 10/s)
- `ip_limit`: IP HTTP limit (default: 600)
- `ip_window_seconds`: IP window duration (default: 5)

**Implementation Notes**:

1. **Response Header Parsing** (TODO - REST client implementation):
   - Parse `X-Bapi-Limit-*` headers from HTTP responses
   - `X-Bapi-Limit`: Current limit for endpoint
   - `X-Bapi-Limit-Status`: Remaining requests
   - `X-Bapi-Limit-Reset-Timestamp`: Reset timestamp
   - Update rate limiter via `update_endpoint_limit(endpoint, limit)`

2. **VIP Account Support** (TODO - Future enhancement):
   - VIP accounts have higher limits (2x-6x multiplier)
   - VIP 1: ~2x, VIP 2: ~4x, VIP 3+: ~6x
   - Limits should be configured based on account tier
   - Source: [Bybit VIP Rate Limits](https://bybit-exchange.github.io/docs/v5/rate-limit/rules-for-vips)

3. **PRO Account Custom Limits** (TODO - Future enhancement):
   - PRO accounts can query/set custom limits via API
   - `GET /v5/apilimit/query` - Query current limits
   - `GET /v5/apilimit/query-all` - Query all limits
   - `POST /v5/apilimit/set` - Set custom limits
   - Should periodically refresh limits from API

4. **Batch Request Handling**:
   - Batch endpoints count per order, not per request
   - Use `record_batch_endpoint(endpoint, order_count)` instead of `record_endpoint`
   - Example: Batch with 5 orders = consumes 5x rate limit capacity

5. **WebSocket Rate Limiting**:
   - IP limits don't apply to WebSocket RPC calls
   - Endpoint limits DO apply to WebSocket RPC (order placement via WS)
   - WebSocket has separate connection limits (500/5min) - not yet implemented

6. **Integration Points**:
   - REST client: Check IP + endpoint limits before request, update from headers after response
   - WebSocket RPC: Check endpoint limit before sending, record after successful send
   - Map WebSocket methods to REST endpoints for consistent rate limiting

**Sources**:
- [Bybit Rate Limit Docs](https://bybit-exchange.github.io/docs/v5/rate-limit)
- [Bybit Rate Limit Table](https://bybit-exchange.github.io/docs/v5/rate-limit#api-rate-limit-table)
- [Bybit VIP Rate Limits](https://bybit-exchange.github.io/docs/v5/rate-limit/rules-for-vips)

### 3.3 Pair and Instrument Management

**Location**: `src/exchange_engine/src/exchanges/bybit/common/pair_manager.rs`

**Status**: ✅ Basic structure implemented. See `src/exchange_engine/src/exchanges/bybit/common/pair_manager.rs`

Bybit uses the **same symbol string** (e.g., "BTCUSDT") for both spot and futures, distinguished by the `category` parameter in API calls.

**Key Design Decisions**:

1. **Bybit uses `Instrument` as `InstrumentId`**: 
   - Bybit implements `TradingClient<OrderRequest, Instrument>` (not `Pair`)
   - This allows methods like `cancel_all_orders(&self, instrument: &Instrument)` to distinguish spot vs futures
   - Binance continues using `Pair` (separate clients, no ambiguity)

2. **Symbol Format**:
   - Spot: "BTCUSDT" (baseCoin + quoteCoin)
   - Linear Futures: "BTCUSDT" (same format, different category)
   - Inverse Futures: "BTCUSD" (different quote coin)
   - Options: "BTC-27MAR26-70000-P-USDT" (different format)

3. **Category Mapping**:
   - `InstrumentType::Spot` → `category="spot"`
   - `InstrumentType::Futures` → `category="linear"` (default) or `category="inverse"`
   - `InstrumentType::Derivitives` → `category="option"`

**Current Implementation**:

- ✅ `instrument_to_bybit()`: Converts `Instrument` → `(symbol, category)`
- ✅ `bybit_to_instrument()`: Converts Bybit response → `Instrument`
- ✅ Basic caching structure for validated instruments
- ✅ Category enum for type safety

**Enhancement TODOs** (when REST client is implemented):

1. **Instrument Validation**:
   - Query `/v5/market/instruments-info` to validate instruments exist
   - Handle pagination (linear has >500 symbols)
   - Check instrument status (Trading, PreLaunch, etc.)

2. **Trading Rules Caching**:
   - Parse and cache trading rules from instrument info (price precision, lot size, min/max orders)
   - Store leverage filters for futures
   - Cache risk parameters

3. **Symbol Validation**:
   - Validate symbol format matches baseCoin + quoteCoin
   - Handle options symbol format parsing
   - Support inverse contracts (different quote coin)

4. **Status Management**:
   - Track instrument status transitions
   - Filter by active status (Trading only vs include PreLaunch)
   - Handle delisted/closed instruments

5. **Cache Refresh**:
   - Periodic refresh of instrument list
   - Handle new instruments being added
   - Handle instruments being delisted

**Sources**:
- [Bybit Instruments Info API](https://bybit-exchange.github.io/docs/v5/market/instrument)

### 3.4 Unified Account Management

**Location**: `src/exchange_engine/src/exchanges/bybit/unified/account.rs`

Bybit uses a unified account model:
- Single API key for both spot and futures
- Shared wallet balance (can transfer between spot and futures)
- Separate positions (spot holdings vs futures positions)
- Unified margin (portfolio margin mode available)

## Phase 4: Market Data Engine - Bybit Streams

### 4.1 Spot Market Data

**Location**: `src/market_data_engine/src/exchanges/bybit/spot/`

#### Streams to Implement

1. **Orderbook Stream**
   - Topic: `orderbook.{depth}.{symbol}`
   - Updates: Real-time orderbook snapshots
   - Depth options: 1, 50, 200

2. **Trade Stream**
   - Topic: `publicTrade.{symbol}`
   - Updates: Real-time public trades

#### Implementation Checklist

- [ ] Create `BybitSpotMarketDataClient` struct
- [ ] Implement `MarketDataClient<Pair>` trait
- [ ] Connect to public WebSocket stream
- [ ] Subscribe to orderbook topics
- [ ] Subscribe to trade topics
- [ ] Parse orderbook updates
- [ ] Parse trade updates
- [ ] Convert to core types
- [ ] Publish to `MarketDataStateManager`

### 4.2 Futures Market Data

**Location**: `src/market_data_engine/src/exchanges/bybit/futures/`

#### Streams to Implement

1. **Orderbook Stream**
   - Topic: `orderbook.{depth}.{symbol}`
   - Same as spot

2. **Trade Stream**
   - Topic: `publicTrade.{symbol}`
   - Same as spot

3. **Mark Price Stream** (optional)
   - Topic: `tickers.{symbol}`
   - Includes mark price in ticker data

4. **Funding Rate Stream** (optional)
   - Topic: `tickers.{symbol}`
   - Includes funding rate in ticker data

#### Implementation Checklist

- [ ] Create `BybitFuturesMarketDataClient` struct
- [ ] Implement `MarketDataClient<Pair>` trait
- [ ] Implement `FuturesMarketDataClient` trait
- [ ] Connect to public WebSocket stream
- [ ] Subscribe to futures market data topics
- [ ] Parse mark price from ticker
- [ ] Parse funding rate from ticker
- [ ] Convert to core types
- [ ] Publish to `MarketDataStateManager`

## Phase 5: Asset Engine - Bybit Trading Rules

### 5.1 Spot Trading Rules

**Location**: `src/asset_engine/src/exchanges/bybit/spot/`

#### REST Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/spot/v3/public/symbols` | GET | Get symbol information |
| `/spot/v3/public/quote/depth` | GET | Get orderbook depth (optional) |

#### Key Data to Fetch

1. **Symbol Information** (`/spot/v3/public/symbols`):
   - Symbol name
   - Base asset, quote asset
   - Price precision
   - Quantity precision
   - Min order quantity
   - Max order quantity
   - Min order value
   - Trading status

#### Implementation Checklist

- [ ] Create `BybitSpotPublicClient` struct
- [ ] Implement spot trading rules fetching
- [ ] Parse symbol information
- [ ] Convert to core trading rules type
- [ ] Cache rules
- [ ] Add periodic refresh

### 5.2 Futures Trading Rules

**Location**: `src/asset_engine/src/exchanges/bybit/futures/`

#### REST Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v5/market/instruments-info` | GET | Get instrument information |
| `/v5/market/risk-limit` | GET | Get risk limits (leverage brackets) |
| `/v5/market/funding/history` | GET | Get funding rate history |

#### Key Data to Fetch

1. **Instrument Information** (`/v5/market/instruments-info`):
   - Symbol name
   - Contract type (Linear, Inverse)
   - Base asset, quote asset
   - Price precision
   - Quantity precision
   - Min order quantity
   - Max order quantity
   - Min order value
   - Contract size
   - Leverage limits

2. **Risk Limits** (`/v5/market/risk-limit`):
   - Max leverage per symbol
   - Initial margin rate
   - Maintenance margin rate
   - Notional caps

#### Implementation Checklist

- [ ] Create `BybitFuturesPublicClient` struct
- [ ] Implement futures trading rules fetching
- [ ] Parse instrument information
- [ ] Parse risk limits
- [ ] Convert to core `FuturesTradingRules` type
- [ ] Cache rules
- [ ] Add periodic refresh

### 5.3 Asset Service Integration

**Location**: `src/asset_engine/src/service.rs`

Extend `AssetService` to support Bybit:

```rust
impl AssetService {
    // ... existing methods ...
    
    pub async fn get_trading_rules(
        &self,
        exchange: Exchange,
        pair: &Pair,
    ) -> Result<TradingRules, EngineError> {
        match exchange {
            Exchange::Bybit => {
                // Use BybitSpotPublicClient
                self.bybit_spot_client.get_trading_rules(pair).await
            }
            // ... other exchanges
        }
    }
    
    pub async fn get_futures_trading_rules(
        &self,
        exchange: Exchange,
        pair: &Pair,
    ) -> Result<FuturesTradingRules, EngineError> {
        match exchange {
            Exchange::Bybit => {
                // Use BybitFuturesPublicClient
                self.bybit_futures_client.get_futures_rules(pair).await
            }
            // ... other exchanges
        }
    }
}
```

## Phase 6: Type Definitions

### 6.1 Bybit Response Types

**Location**: `src/exchange_engine/src/exchanges/bybit/common/types.rs`

```rust
/// Bybit API response wrapper
#[derive(Debug, Deserialize, Serialize)]
pub struct BybitResponse<T> {
    pub ret_code: i32,
    pub ret_msg: String,
    pub result: Option<T>,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}

/// Bybit error response
#[derive(Debug, Deserialize, Serialize)]
pub struct BybitError {
    pub ret_code: i32,
    pub ret_msg: String,
    pub ret_ext_info: Option<serde_json::Value>,
    pub time: u64,
}
```

### 6.2 Spot Order Types

**Location**: `src/exchange_engine/src/exchanges/bybit/spot/rest/types/order.rs`

```rust
/// Bybit spot order request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitSpotOrderRequest {
    pub symbol: String,
    pub qty: String,              // Quantity as string
    pub side: String,            // "Buy" or "Sell"
    #[serde(rename = "type")]
    pub order_type: String,      // "Limit", "Market"
    pub price: Option<String>,    // Price as string (required for Limit)
    pub time_in_force: Option<String>, // "GTC", "IOC", "FOK"
    pub order_link_id: Option<String>, // Client order ID
}

/// Bybit spot order response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitSpotOrderResponse {
    pub order_id: String,
    pub order_link_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: String,
    pub qty: String,
    pub time_in_force: String,
    pub order_status: String,
    pub created_time: String,
    pub updated_time: String,
}
```

### 6.3 Futures Position Types

**Location**: `src/exchange_engine/src/exchanges/bybit/futures/rest/types/position.rs`

```rust
/// Bybit futures position
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitFuturesPosition {
    pub symbol: String,
    pub side: String,                    // "Buy" or "Sell"
    pub size: String,                    // Position size
    pub position_value: String,          // Position value
    pub entry_price: String,              // Entry price
    pub mark_price: String,               // Mark price
    pub liq_price: String,                // Liquidation price
    pub bust_price: String,               // Bust price
    pub leverage: String,                 // Leverage
    pub auto_add_margin: String,          // Auto add margin
    pub position_status: String,         // Position status
    pub position_idx: i32,                // Position index (0=one-way, 1=long, 2=short)
    pub take_profit: String,              // Take profit price
    pub stop_loss: String,                // Stop loss price
    pub trailing_stop: String,            // Trailing stop
    pub unrealised_pnl: String,           // Unrealized P&L
    pub cum_realised_pnl: String,          // Cumulative realized P&L
    pub created_time: String,             // Created time
    pub updated_time: String,             // Updated time
}
```

## Phase 7: Configuration & Runner Integration

### 7.1 Configuration Format

**Location**: `config/bot.json`

```json
{
  "exchanges": {
    "bybit": {
      "enabled": true,
      "api_key": "...",
      "api_secret": "...",
      "testnet": false,
      "spot": {
        "enabled": true
      },
      "futures": {
        "enabled": true,
        "default_position_mode": "MergedSingle",
        "default_margin_mode": "REGULAR_MARGIN",
        "max_leverage": 10
      }
    }
  }
}
```

### 7.2 Runner Updates

**Location**: `src/runner/src/lib.rs`

- Initialize `BybitSpotDriver` if spot enabled
- Initialize `BybitFuturesDriver` if futures enabled
- Pass clients to trading engine
- Support both spot and futures strategies simultaneously
- Initialize market data clients
- Initialize asset service clients

## Implementation Phases

### Phase 1: Bybit Unified REST Client (Week 1-2)
- [ ] Create `bybit` module structure
- [ ] Implement configuration
- [ ] Implement authentication (Bybit-specific)
- [ ] Create unified `BybitHttpClient` struct
- [ ] Implement `TradingClient` trait (spot methods)
- [ ] Implement `open_order()` with routing based on `InstrumentType`
- [ ] Implement spot order placement and cancellation
- [ ] Implement spot balance fetching
- [ ] Add integration tests with testnet

### Phase 2: Bybit Futures Methods (Week 2-3)
- [ ] Implement all futures methods from `TradingClient` trait
- [ ] Implement position fetching and management
- [ ] Implement futures balance fetching
- [ ] Implement leverage and margin management
- [ ] Implement position mode management
- [ ] Add integration tests

### Phase 3: Bybit Unified WebSocket Client (Week 3-4)
- [ ] Create unified `BybitWsClient` struct
- [ ] Implement `ExchangeDriver` trait
- [ ] Implement public stream connection
- [ ] Implement private stream connection (authenticated)
- [ ] Subscribe to spot topics (orderbook, trades)
- [ ] Subscribe to futures topics (positions, funding rate)
- [ ] Handle order updates (both spot and futures)
- [ ] Handle balance updates (unified account)
- [ ] Handle position updates (futures)
- [ ] Add reconnection logic
- [ ] Add integration tests

### Phase 4: Market Data Integration (Week 4-5)
- [ ] Create Bybit spot market data client
- [ ] Create Bybit futures market data client
- [ ] Implement orderbook and trade streams
- [ ] Implement mark price and funding rate streams
- [ ] Update market data state manager
- [ ] Add integration tests

### Phase 5: Asset Engine Integration (Week 5-6)
- [ ] Create Bybit spot public client
- [ ] Create Bybit futures public client
- [ ] Implement trading rules fetching
- [ ] Integrate with AssetService
- [ ] Add caching and refresh logic

### Phase 6: Testing & Documentation (Week 6-7)
- [ ] End-to-end testing with Bybit testnet
- [ ] Test all spot operations
- [ ] Test all futures operations
- [ ] Test unified account model
- [ ] Test reconciliation
- [ ] Update documentation
- [ ] Performance testing

## Testing Strategy

### Unit Tests
- Type serialization/deserialization
- Request/response parsing
- Authentication signing
- Error handling

### Integration Tests (Testnet)
- Spot order placement and cancellation
- Futures order placement and cancellation
- Position management
- Balance tracking
- Leverage and margin operations
- WebSocket stream handling
- Reconciliation

### End-to-End Tests
- Full spot trading flow
- Full futures trading flow
- Unified account operations
- Error recovery
- Rate limit handling

## Error Handling

### Common Bybit Error Codes

- `10001`: Parameter error
- `10002`: API key not found
- `10003`: Sign error
- `10004`: Permission denied
- `10005`: Too many requests (rate limit)
- `10006`: Request timeout
- `10007`: Internal error
- `10008`: Invalid request
- `10009`: Operation failed
- `10010`: Order not found
- `10011`: Insufficient balance
- `10012`: Order quantity too large
- `10013`: Order price invalid

### Handling Strategy

- Parse `retCode` from response
- Map Bybit error codes to `EngineError` variants
- Retry with exponential backoff for transient errors
- Log all errors with context
- Handle rate limits gracefully

## Rate Limiting

Bybit rate limits:
- **REST API**: 120 requests per second (shared for spot and futures)
- **WebSocket**: 20 subscriptions per connection, 10 connections per IP
- **Order Rate**: Varies by symbol and account tier

Implement rate limiter similar to Binance but with Bybit-specific limits.

## Security Considerations

1. **API Keys**: Store securely, never log
2. **Authentication**: Always use server time for timestamp
3. **Testnet**: Use testnet for all development
4. **Rate Limiting**: Respect all rate limits
5. **WebSocket**: Rotate connections if needed

## Unified Account Model Considerations

1. **Balance Sharing**: Spot and futures share wallet balance
2. **Transfer**: Can transfer funds between spot and futures
3. **Margin**: Can use portfolio margin mode (unified margin)
4. **Positions**: Separate tracking (spot holdings vs futures positions)
5. **Risk**: Unified risk management across spot and futures

## Success Criteria

- [ ] Single `BybitClient` implements both spot and futures methods
- [ ] All REST endpoints implemented and tested (spot + futures)
- [ ] WebSocket streams working (spot + futures) in single client
- [ ] Positions tracked correctly in ExchangeState
- [ ] Balance tracked correctly (unified account)
- [ ] Leverage and margin management working
- [ ] Market data streaming (spot + futures)
- [ ] Trading rules fetched and cached
- [ ] Reconciliation working for all data
- [ ] Unified account model working correctly
- [ ] Differentiation by `InstrumentType` working correctly
- [ ] No regressions in existing exchanges
- [ ] Documentation complete
- [ ] Ready for production use (after testnet validation)

## Future Enhancements

1. **Portfolio Margin**: Support for portfolio margin mode
2. **Advanced Order Types**: Conditional orders, OCO orders
3. **Options Trading**: Bybit options support (if available)
4. **Copy Trading**: Bybit copy trading API integration
5. **Unified Risk Management**: Cross-product risk limits

---

**Next Steps**: 
1. Complete Proposal 02 (Futures Integration) first
2. Review and approve this proposal
3. Begin Phase 1 implementation (Bybit Spot REST Client)

