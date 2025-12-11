# Asset Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Asset Engine fetches and caches static asset metadata (symbols, trading rules, exchange limits) from the exchange via public REST API endpoints. It is a **caching-only** service (not a state manager) that provides fast lookup for order validation and rounding.

## Key Principle

Asset Engine provides **static asset metadata** with TTL-based refresh. Exchange values take priority over config values, with graceful config fallback.

## Architecture

### Structure

```
src/asset_engine/
├── src/
│   ├── service.rs                # AssetService - orchestrates asset engine
│   ├── cache.rs                  # AssetDataCache - TTL-based cache
│   └── exchanges/
│       └── binance/
│           └── public_client.rs  # BinancePublicClient - REST client
```

### Components

#### 1. AssetService (`service.rs`)

**Purpose**: Orchestrates the asset engine.

**Key Responsibilities**:
- Manages exchange registration
- Fetches market data at bootstrap
- Provides API for trading rules and symbol info
- TTL-based caching with automatic refresh

**Methods**:
- `new()` - Create new AssetService
- `register_exchange(exchange: Exchange, client: Arc<dyn PublicClient>)` - Register exchange
- `fetch_market_data(exchange: Exchange, pairs: &[Pair]) -> Result<()>` - Fetch market data
- `get_trading_rules(exchange: Exchange, pair: &Pair) -> Option<TradingRules>` - Get trading rules
- `get_symbol_info(exchange: Exchange, pair: &Pair) -> Option<SymbolInfo>` - Get symbol info

#### 2. AssetDataCache (`cache.rs`)

**Purpose**: TTL-based cache for asset data.

**Data Structures**:
- `data: HashMap<Exchange, ExchangeInfo>` - Cached exchange info by exchange
- `metadata: HashMap<Exchange, CacheMetadata>` - Cache metadata (TTL, timestamps)

**CacheMetadata**:
- `created_at: u64` - Cache creation timestamp
- `ttl_secs: u64` - Time-to-live in seconds (default: 300 = 5 minutes)

**Methods**:
- `new(default_ttl: u64)` - Create new cache
- `get(exchange: &Exchange) -> Option<ExchangeInfo>` - Get cached data
- `is_stale(exchange: &Exchange) -> bool` - Check if cache is stale
- `update(exchange: Exchange, data: ExchangeInfo)` - Update cache

#### 3. Exchange-Specific Clients (`exchanges/{exchange}/public_client.rs`)

**Purpose**: REST clients for fetching public exchange data.

**BinancePublicClient**:
- Fetches exchange info (symbols, trading rules)
- Fetches exchange limits
- No authentication required (public endpoints)

**Methods**:
- `new(base_url: String)` - Create new client
- `fetch_exchange_info() -> Result<ExchangeInfo>` - Fetch exchange info
- `fetch_exchange_limits() -> Result<ExchangeLimits>` - Fetch exchange limits

## Data Flow

```
Bootstrap → AssetService
    ├── Fetch market data from exchange (REST API)
    ├── Parse exchange info (symbols, trading rules, limits)
    ├── Store in cache (TTL-based)
    └── Provide to consumers
        ├──→ Level Resolver (for order validation)
        └──→ Strategy Executor (for order validation)
```

## Communication

### Input
- **REST API**: Public exchange endpoints (no authentication)

### Output
- **Trading Rules**: Price/quantity validation rules
- **Symbol Info**: Symbol metadata
- **Exchange Limits**: Exchange-specific limits

## Caching Strategy

### TTL-Based Cache

- **Default TTL**: 5 minutes (300 seconds)
- **Automatic Refresh**: Fetches new data when cache is stale
- **Stale Check**: `is_stale()` checks if cache needs refresh

### Priority System

1. **Exchange Values** (highest priority): Values from exchange REST API
2. **Config Values** (fallback): Values from configuration files

**Graceful Degradation**: System works with or without Asset Engine data (falls back to config values).

## Data Structures

### TradingRules

- `tick_size: Decimal` - Minimum price increment
- `step_size: Decimal` - Minimum quantity increment
- `min_quantity: Decimal` - Minimum order quantity
- `max_quantity: Decimal` - Maximum order quantity
- `min_price: Decimal` - Minimum order price
- `max_price: Decimal` - Maximum order price
- `min_notional: Decimal` - Minimum order notional value

### SymbolInfo

- `symbol: Symbol` - Symbol string
- `pair: Pair` - Base/quote pair
- `status: SymbolStatus` - Trading status
- `base_asset: String` - Base asset
- `quote_asset: String` - Quote asset

### ExchangeLimits

- `rate_limits: Vec<RateLimit>` - Rate limit rules
- `order_limits: OrderLimits` - Order-specific limits

## Usage

### Order Validation

Asset Engine is used in:
- **Level Resolver** (`IndicatorResolver`): For price/quantity rounding
- **Strategy Executor**: For order validation

**Example**:
```rust
// Round price to tick size
let tick_size = asset_service.get_trading_rules(exchange, pair)
    .map(|rules| rules.tick_size)
    .unwrap_or(config_tick_size);
let rounded_price = round_to_tick(price, tick_size);

// Round quantity to step size
let step_size = asset_service.get_trading_rules(exchange, pair)
    .map(|rules| rules.step_size)
    .unwrap_or(config_step_size);
let rounded_quantity = round_to_step(quantity, step_size);
```

## Performance Characteristics

- **Lookup Latency**: < 0.1ms (in-memory cache)
- **Cache TTL**: 5 minutes (configurable)
- **Memory**: Efficient HashMap storage
- **Refresh**: Non-blocking, runs in background

## Dependencies

- `trading_core`: Types, Exchange enum
- `tokio`: Async runtime
- `reqwest`: HTTP client for REST API

## Testing

- Unit tests for cache
- Unit tests for service
- Integration tests for exchange data fetching

## Configuration

### Cache Configuration

```json
{
  "asset_engine": {
    "cache_ttl_secs": 300
  }
}
```

## Error Handling

- **REST API Failures**: Logged, falls back to config values
- **Parse Errors**: Logged, falls back to config values
- **Stale Cache**: Automatically refreshed on next access

## Security

- **Public Endpoints**: No authentication required
- **TLS/SSL**: All connections use TLS
- **Rate Limiting**: Handled automatically by exchange clients

## Bootstrap Integration

Asset Engine fetches market data at application startup:
1. System starts
2. AssetService initialized
3. Market data fetched for configured pairs
4. Data cached with TTL
5. Available for order validation throughout runtime

