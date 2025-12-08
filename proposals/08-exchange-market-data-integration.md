# Proposal 08: Exchange Market Data Integration

## Overview

This proposal outlines the integration of exchange market data (symbol info, trading rules, prices, etc.) into the level creation and management system. Market data is fetched via **public REST calls** at bootstrap. Exchange values always take priority over config values, with config/UI as fallback.

**Key Architectural Decision**: Market data fetching (symbol info, trading rules, exchange limits) is handled by a **separate Asset Engine service**, not by the Trading Engine or Exchange Engine. This separation ensures:
- Clear responsibility: Asset Engine = public asset data, Exchange Engine = trading operations
- Reusability: Asset data can be used by config validation, level creation, and other services
- Consistency: Follows the pattern of Market Data Engine and Exchange Engine as separate services

**Note**: This proposal focuses on the Asset Engine design. The actual implementation will be done as part of future proposals. Redis to Channels migration is complete.

## Key Requirements

1. **Asset Engine Service**: New service dedicated to fetching public asset data via REST (unsigned calls)
2. **Market Data Fetching**: Fetch all market data (symbol info, trading rules, prices, orderbook snapshot, exchange limits) at bootstrap
3. **Public REST Client**: Asset Engine uses unsigned REST calls (no API keys required)
4. **Priority System**: Exchange values always take priority over config values
5. **Bootstrap Integration**: Fetch at bootstrap, cache with TTL
6. **TTL-Based Caching**: Cache market data with TTL (e.g., 5-10 minutes), refresh on-demand when stale
7. **Fallback Strategy**: Fallback to JSON config or UI if exchange data unavailable
8. **Exchange-Specific Implementations**: Each exchange (Binance, etc.) has its own asset data fetcher

## Architecture Overview

### Data Flow

```
Bootstrap
    ↓
Asset Engine (NEW SERVICE)
    ├── Public REST Client (unsigned)
    │   ├── fetch_exchange_info() → Symbol info, trading rules
    │   ├── fetch_ticker_prices() → Current prices
    │   ├── fetch_orderbook_snapshot() → Orderbook depth
    │   └── fetch_exchange_limits() → Rate limits, position limits
    │
    └── Asset Data Service
        ├── Cache asset data (TTL-based)
        ├── Provide symbol validation
        └── Publish asset updates (optional, via channel)
        ↓
Level Manager / Level Resolver
    ├── Check Asset Engine Data (priority)
    └── Fallback to Config/UI
```

### Component Separation

```
┌─────────────────────────────────────────────────────────┐
│                    Asset Engine                          │
│  (Public REST calls only - no authentication)          │
├─────────────────────────────────────────────────────────┤
│  exchanges/                                             │
│    └── binance/                                         │
│        ├── public_client.rs      (REST client, unsigned)│
│        └── asset_service.rs      (caches, validates)     │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│                    Exchange Engine                       │
│  (Trading operations - requires authentication)         │
├─────────────────────────────────────────────────────────┤
│  - WebSocket: Execution reports, balances, orders      │
│  - REST: Signed calls for trading                      │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│                    Trading Engine                        │
│  (Uses Asset Engine data for validation/resolution)     │
└─────────────────────────────────────────────────────────┘
```

### Component Structure

```
trading_system/
├── src/
│   ├── asset_engine/              (NEW SERVICE)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── service.rs         # AssetService - main service
│   │   │   ├── cache.rs           # TTL-based caching
│   │   │   └── exchanges/
│   │   │       └── binance/
│   │   │           ├── public_client.rs  # Public REST client (unsigned)
│   │   │           └── asset_service.rs  # Binance-specific asset data
│   │   └── Cargo.toml
│   │
│   ├── exchange_engine/           (EXISTING - trading operations)
│   │   └── exchanges/
│   │       └── binance/
│   │           ├── ws_client.rs   # WebSocket (execution reports, balances)
│   │           └── rest_client.rs  # Signed REST (trading operations)
│   │
│   └── trading_engine/
│       └── level/
│           ├── resolver.rs
│           │   └── Use Asset Engine data (priority)
│           └── config.rs
│               └── Resolve with priority system
│
└── libs/
    └── trading_core/
        └── types/
            └── asset.rs           # Asset data types (NEW)
```

## Detailed Component Design

### 1. Asset Engine Service (`asset_engine/src/service.rs`)

**Purpose**: Main service for fetching and caching asset data.

```rust
pub struct AssetService {
    cache: Arc<RwLock<AssetDataCache>>,
    exchange_clients: HashMap<Exchange, Box<dyn AssetDataClient>>,
}

#[async_trait]
pub trait AssetDataClient: Send + Sync + Debug {
    /// Fetch exchange info (symbols, trading rules)
    async fn fetch_exchange_info(&self) -> Result<ExchangeInfo, EngineError>;
    
    /// Fetch current ticker prices
    async fn fetch_ticker_prices(&self, pairs: &[Pair]) -> Result<HashMap<Pair, Decimal>, EngineError>;
    
    /// Fetch orderbook snapshot
    async fn fetch_orderbook_snapshot(&self, pair: &Pair) -> Result<Orderbook, EngineError>;
    
    /// Fetch exchange limits (rate limits, position limits)
    async fn fetch_exchange_limits(&self) -> Result<ExchangeLimits, EngineError>;
}

impl AssetService {
    /// Fetch comprehensive market data for pairs
    pub async fn fetch_market_data(
        &self,
        exchange: Exchange,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError> {
        // Check cache first
        if let Some(cached) = self.get_cached_market_data(exchange, pairs) {
            if !cached.is_stale() {
                return Ok(cached.data);
            }
        }
        
        // Fetch fresh data
        let client = self.exchange_clients.get(&exchange)
            .ok_or_else(|| EngineError::UnsupportedExchange)?;
        
        let exchange_info = client.fetch_exchange_info().await?;
        let prices = client.fetch_ticker_prices(pairs).await?;
        let limits = client.fetch_exchange_limits().await?;
        
        // Build market data
        let market_data = ExchangeMarketData {
            symbol_info: self.parse_symbol_info(&exchange_info, pairs)?,
            trading_rules: self.parse_trading_rules(&exchange_info, pairs)?,
            current_prices: prices,
            exchange_limits: limits,
            last_updated: current_timestamp(),
        };
        
        // Update cache
        self.update_cache(exchange, market_data.clone()).await;
        
        Ok(market_data)
    }
}
```

### 2. Asset Engine Binance Implementation (`asset_engine/src/exchanges/binance/public_client.rs`)

**Purpose**: Binance-specific public REST client (unsigned calls only).

```rust
pub struct BinancePublicClient {
    base_url: String,
    http_client: Client,
}

impl BinancePublicClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http_client: Client::new(),
        }
    }
    
    /// Fetch exchange info (public endpoint, no auth)
    pub async fn fetch_exchange_info(&self) -> Result<BinanceExchangeInfo, EngineError> {
        let url = format!("{}/api/v3/exchangeInfo", self.base_url);
        let response = self.http_client
            .get(&url)
            .send()
            .await?;
        
        let exchange_info: BinanceExchangeInfo = response.json().await?;
        Ok(exchange_info)
    }
    
    /// Fetch ticker prices (public endpoint, no auth)
    pub async fn fetch_ticker_prices(&self, pairs: &[Pair]) -> Result<HashMap<Pair, Decimal>, EngineError> {
        // Implementation
    }
    
    /// Fetch orderbook snapshot (public endpoint, no auth)
    pub async fn fetch_orderbook_snapshot(&self, pair: &Pair) -> Result<Orderbook, EngineError> {
        // Implementation
    }
}
```

### 3. Exchange Market Data Types (`libs/trading_core/src/types/asset.rs`)

**Purpose**: Common types for exchange market data.

```rust
use std::collections::HashMap;

/// Complete market data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeMarketData {
    /// Symbol information by pair
    pub symbol_info: HashMap<Pair, SymbolInfo>,
    
    /// Trading rules by pair
    pub trading_rules: HashMap<Pair, TradingRules>,
    
    /// Current market prices by pair
    pub current_prices: HashMap<Pair, Decimal>,
    
    /// Orderbook snapshots by pair (optional, for depth validation)
    pub orderbook_snapshots: HashMap<Pair, Orderbook>,
    
    /// Exchange limits (rate limits, position limits, etc.)
    pub exchange_limits: ExchangeLimits,
    
    /// Last updated timestamp
    pub last_updated: u64,
}

/// Symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Trading pair
    pub pair: Pair,
    
    /// Symbol status
    pub status: SymbolStatus,
    
    /// Base asset
    pub base_asset: String,
    
    /// Quote asset
    pub quote_asset: String,
    
    /// Symbol name on exchange (e.g., "BTCUSDT")
    pub exchange_symbol: String,
    
    /// Whether trading is allowed
    pub trading_allowed: bool,
    
    /// Whether the symbol is active
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SymbolStatus {
    PreTrading,
    Trading,
    PostTrading,
    EndOfDay,
    Halt,
    AuctionMatch,
    Break,
    PendingTrading,
}

/// Trading rules for a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingRules {
    /// Trading pair
    pub pair: Pair,
    
    /// Minimum order size (in base currency)
    pub min_order_size: Decimal,
    
    /// Maximum order size (in base currency, if applicable)
    pub max_order_size: Option<Decimal>,
    
    /// Step size for order quantities (LOT_SIZE filter)
    /// Quantities must be rounded to a multiple of this
    pub step_size: Decimal,
    
    /// Minimum price
    pub min_price: Option<Decimal>,
    
    /// Maximum price
    pub max_price: Option<Decimal>,
    
    /// Tick size for order prices (PRICE_FILTER)
    /// Prices must be rounded to a multiple of this
    pub tick_size: Decimal,
    
    /// Minimum notional value (price * quantity) in quote currency
    pub min_notional: Decimal,
    
    /// Maximum notional value (if applicable)
    pub max_notional: Option<Decimal>,
    
    /// Price precision (decimal places)
    pub price_precision: u8,
    
    /// Quantity precision (decimal places)
    pub quantity_precision: u8,
}

/// Exchange limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeLimits {
    /// Rate limits
    pub rate_limits: Vec<RateLimit>,
    
    /// Maximum position size (if applicable)
    pub max_position_size: Option<Decimal>,
    
    /// Maximum number of open orders
    pub max_open_orders: Option<usize>,
    
    /// Other exchange-specific limits
    pub other_limits: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub rate_limit_type: RateLimitType,
    pub interval: String,  // e.g., "MINUTE", "SECOND"
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitType {
    RequestWeight,
    Orders,
    RawRequests,
}
```

### 4. Asset Engine Integration in Runner (`runner/src/main.rs`)

**Purpose**: Initialize Asset Engine at bootstrap.

```rust
// At bootstrap
let asset_service = AssetService::new()?;
asset_service.register_exchange(
    Exchange::Binance,
    Box::new(BinancePublicClient::new(binance_base_url)),
)?;

// Fetch market data at bootstrap
let market_data = asset_service
    .fetch_market_data(Exchange::Binance, &configured_pairs)
    .await?;

// Pass to level manager/resolver
let level_manager = LevelManager::new(
    asset_service.clone(),  // Pass asset service for market data access
    // ... other params
);
```

### 5. Level Resolver Integration (`trading_engine/src/levels/resolver.rs`)

**Purpose**: Use Asset Engine data for level resolution.

```rust
pub struct LevelResolver {
    asset_service: Option<Arc<AssetService>>,  // Asset Engine reference
}

impl LevelResolver {
    /// Get trading rules for a pair (from Asset Engine)
    pub async fn get_trading_rules(&self, pair: &Pair) -> Option<TradingRules> {
        if let Some(service) = &self.asset_service {
            service.get_trading_rules(pair).await
        } else {
            None  // Fallback to config
        }
    }
    
    /// Get current price for a pair (from Asset Engine)
    pub async fn get_current_price(&self, pair: &Pair) -> Option<Decimal> {
        if let Some(service) = &self.asset_service {
            service.get_current_price(pair).await
        } else {
            None  // Fallback to config
        }
    }
}
```

### 6. Asset Engine Caching (`asset_engine/src/cache.rs`)

**Purpose**: TTL-based caching for asset data.

```rust
pub struct AssetDataCache {
    data: HashMap<Exchange, ExchangeMarketData>,
    cache_metadata: HashMap<Exchange, CacheMetadata>,
    default_ttl: Duration,  // e.g., 5-10 minutes
}

pub struct CacheMetadata {
    cached_at: Instant,
    ttl: Duration,
}

impl AssetDataCache {
    /// Get cached data if available and not stale
    pub fn get(&self, exchange: Exchange) -> Option<&ExchangeMarketData> {
        if let Some(metadata) = self.cache_metadata.get(&exchange) {
            if metadata.cached_at.elapsed() < metadata.ttl {
                return self.data.get(&exchange);
            }
        }
        None
    }
    
    /// Update cache
    pub fn update(&mut self, exchange: Exchange, data: ExchangeMarketData) {
        self.data.insert(exchange.clone(), data);
        self.cache_metadata.insert(exchange, CacheMetadata {
            cached_at: Instant::now(),
            ttl: self.default_ttl,
        });
    }
}
```

### 7. Level Resolver Update (`trading_engine/level/resolver.rs`)

**Purpose**: Use exchange market data with priority over config.

```rust
pub struct IndicatorResolver {
    exchange_state_manager: Option<Arc<ExchangeStateManager>>,
}

impl IndicatorResolver {
    /// Resolve price with priority: Exchange Data > Config
    pub fn resolve_price(
        &self,
        config: &PriceConfig,
        pair: &Pair,
        indicators: &HashMap<String, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<Decimal, EngineError> {
        // First, try to get exchange trading rules
        let trading_rules = self.get_trading_rules(pair);
        
        match config {
            PriceConfig::Fixed(price) => {
                // Validate against exchange limits if available
                if let Some(rules) = &trading_rules {
                    // Round to tick size
                    let rounded = self.round_to_tick_size(*price, rules.tick_size);
                    // Validate min/max price
                    if let Some(min) = rules.min_price {
                        if rounded < min {
                            return Err(EngineError::CustomError(
                                anyhow::anyhow!("Price {} below minimum {}", rounded, min)
                            ));
                        }
                    }
                    if let Some(max) = rules.max_price {
                        if rounded > max {
                            return Err(EngineError::CustomError(
                                anyhow::anyhow!("Price {} above maximum {}", rounded, max)
                            ));
                        }
                    }
                    Ok(rounded)
                } else {
                    // Fallback to config value
                    Ok(*price)
                }
            }
            PriceConfig::Indicator { indicator_name, offset_type, offset_value } => {
                // Resolve from indicator (existing logic)
                let indicator_value = indicators.get(indicator_name)
                    .ok_or_else(|| EngineError::CustomError(
                        anyhow::anyhow!("Indicator {} not found", indicator_name)
                    ))?;
                
                let base_price = indicator_value.value;
                let price = self.apply_offset(base_price, *offset_type, *offset_value);
                
                // Round to tick size if exchange rules available
                if let Some(rules) = &trading_rules {
                    Ok(self.round_to_tick_size(price, rules.tick_size))
                } else {
                    Ok(price)
                }
            }
            // ... other price config types
        }
    }
    
    /// Resolve quantity with priority: Exchange Data > Config
    pub fn resolve_quantity(
        &self,
        config: &QuantityConfig,
        pair: &Pair,
        balance: Decimal,
        current_price: Decimal,
    ) -> Result<Decimal, EngineError> {
        // Get exchange trading rules
        let trading_rules = self.get_trading_rules(pair);
        
        match config {
            QuantityConfig::Fixed(quantity) => {
                // Validate and round using exchange rules if available
                if let Some(rules) = &trading_rules {
                    // Round to step size
                    let rounded = self.round_to_step_size(*quantity, rules.step_size);
                    
                    // Validate min order size
                    if rounded < rules.min_order_size {
                        return Err(EngineError::CustomError(
                            anyhow::anyhow!(
                                "Quantity {} below minimum {}",
                                rounded,
                                rules.min_order_size
                            )
                        ));
                    }
                    
                    // Validate max order size
                    if let Some(max) = rules.max_order_size {
                        if rounded > max {
                            return Err(EngineError::CustomError(
                                anyhow::anyhow!(
                                    "Quantity {} above maximum {}",
                                    rounded,
                                    max
                                )
                            ));
                        }
                    }
                    
                    // Validate min notional
                    let notional = rounded * current_price;
                    if notional < rules.min_notional {
                        return Err(EngineError::CustomError(
                            anyhow::anyhow!(
                                "Notional {} below minimum {}",
                                notional,
                                rules.min_notional
                            )
                        ));
                    }
                    
                    Ok(rounded)
                } else {
                    // Fallback to config value
                    Ok(*quantity)
                }
            }
            QuantityConfig::BalancePercent(percent) => {
                let quantity = balance * (*percent / Decimal::from(100));
                
                // Round and validate using exchange rules if available
                if let Some(rules) = &trading_rules {
                    let rounded = self.round_to_step_size(quantity, rules.step_size);
                    // Validate min/max
                    self.validate_quantity(rounded, &rules, current_price)?;
                    Ok(rounded)
                } else {
                    Ok(quantity)
                }
            }
            QuantityConfig::Notional(notional) => {
                let quantity = notional / current_price;
                
                // Round and validate using exchange rules if available
                if let Some(rules) = &trading_rules {
                    let rounded = self.round_to_step_size(quantity, rules.step_size);
                    // Validate min/max
                    self.validate_quantity(rounded, &rules, current_price)?;
                    Ok(rounded)
                } else {
                    Ok(quantity)
                }
            }
            // ... other quantity config types
        }
    }
    
    /// Get trading rules for a pair (with fallback)
    fn get_trading_rules(&self, pair: &Pair) -> Option<TradingRules> {
        // Priority: Exchange Data > Config (if config has rules)
        if let Some(manager) = &self.exchange_state_manager {
            manager.get_trading_rules(pair)
        } else {
            None  // Fallback to config
        }
    }
    
    /// Round to tick size
    fn round_to_tick_size(&self, price: Decimal, tick_size: Decimal) -> Decimal {
        (price / tick_size).floor() * tick_size
    }
    
    /// Round to step size
    fn round_to_step_size(&self, quantity: Decimal, step_size: Decimal) -> Decimal {
        (quantity / step_size).floor() * step_size
    }
    
    /// Validate quantity against trading rules
    fn validate_quantity(
        &self,
        quantity: Decimal,
        rules: &TradingRules,
        current_price: Decimal,
    ) -> Result<(), EngineError> {
        // Validate min order size
        if quantity < rules.min_order_size {
            return Err(EngineError::CustomError(
                anyhow::anyhow!(
                    "Quantity {} below minimum {}",
                    quantity,
                    rules.min_order_size
                )
            ));
        }
        
        // Validate max order size
        if let Some(max) = rules.max_order_size {
            if quantity > max {
                return Err(EngineError::CustomError(
                    anyhow::anyhow!(
                        "Quantity {} above maximum {}",
                        quantity,
                        max
                    )
                ));
            }
        }
        
        // Validate min notional
        let notional = quantity * current_price;
        if notional < rules.min_notional {
            return Err(EngineError::CustomError(
                anyhow::anyhow!(
                    "Notional {} below minimum {}",
                    notional,
                    rules.min_notional
                )
            ));
        }
        
        // Validate max notional
        if let Some(max_notional) = rules.max_notional {
            if notional > max_notional {
                return Err(EngineError::CustomError(
                    anyhow::anyhow!(
                        "Notional {} above maximum {}",
                        notional,
                        max_notional
                    )
                ));
            }
        }
        
        Ok(())
    }
}
```

### 8. Level Config Priority System (`trading_engine/level/config.rs`)

**Purpose**: Document and enforce priority system.

```rust
impl LevelConfig {
    /// Resolve level configuration with priority:
    /// 1. Exchange Market Data (highest priority - most accurate)
    /// 2. Config values (fallback)
    /// 3. UI values (fallback, if available)
    ///
    /// This ensures levels always use the most up-to-date and accurate
    /// values from the exchange, falling back to config/UI only when
    /// exchange data is unavailable.
    pub fn resolve_with_priority(
        &self,
        exchange_data: Option<&ExchangeMarketData>,
        pair: &Pair,
    ) -> ResolvedLevelConfig {
        // Implementation uses exchange data when available
        // Falls back to self (config) when not available
    }
}
```

## Integration Points

### 1. Bootstrap Integration

```rust
// In runner/main.rs
// 1. Initialize Asset Engine
let asset_service = AssetService::new()?;
asset_service.register_exchange(
    Exchange::Binance,
    Box::new(BinancePublicClient::new(binance_base_url)),
)?;

// 2. Fetch market data at bootstrap
let market_data = asset_service
    .fetch_market_data(Exchange::Binance, &configured_pairs)
    .await?;

// 3. Initialize Level Manager with Asset Engine
let level_manager = LevelManager::new(
    asset_service.clone(),  // Pass asset service for market data access
    // ... other params
);
```

### 2. Market Data Refresh

```rust
// Periodic refresh of market data (if cache is stale)
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300)); // Every 5 minutes
    
    loop {
        interval.tick().await;
        
        // Refresh market data (Asset Engine handles caching internally)
        if let Err(e) = asset_service
            .fetch_market_data(Exchange::Binance, &configured_pairs)
            .await
        {
            warn!("Failed to refresh market data: {}", e);
        }
    }
});
```

### 3. Level Creation Integration

```rust
// In level manager
impl LevelManager {
    pub async fn create_level(
        &mut self,
        config: LevelConfig,
    ) -> Result<Level, EngineError> {
        // Get market data from Asset Engine
        let market_data = if let Some(service) = &self.asset_service {
            service.get_market_data(&config.symbol).await
        } else {
            None
        };
        
        // Resolve level config with priority
        let resolved = config.resolve_with_priority(market_data.as_ref(), &config.symbol);
        
        // Create level with resolved values
        Level::new(resolved)
    }
}
```

### 4. Level Execution Integration

```rust
// In level executor
impl LevelExecutor {
    pub async fn resolve_level(
        &self,
        level: &Level,
        indicators: &HashMap<String, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<ResolvedLevel, EngineError> {
        // Get market data from Asset Engine (via resolver)
        // Resolver already has Asset Engine reference
        
        // Resolve with priority
        let price = self.resolver.resolve_price(
            &level.config.price,
            &level.config.symbol,
            indicators,
            orderbook,
        ).await?;
        
        let quantity = self.resolver.resolve_quantity(
            &level.config.quantity,
            &level.config.symbol,
            balance,
            price,
        ).await?;
        
        Ok(ResolvedLevel { price, quantity, ... })
    }
}
```

## Error Handling

### 1. Asset Engine Data Unavailable

```rust
// In level resolver
if let Some(rules) = self.get_trading_rules(pair).await {
    // Use Asset Engine data
} else {
    // Fallback to config
    warn!("Asset Engine data unavailable for {}, using config values", pair);
    // Use config values
}
```

### 2. Asset Engine Fetch Failure

```rust
// In bootstrap
match asset_service.fetch_market_data(Exchange::Binance, &pairs).await {
    Ok(market_data) => {
        // Asset Engine handles caching internally
        info!("Market data fetched successfully");
    }
    Err(e) => {
        warn!("Failed to fetch market data: {}. Levels will use config values.", e);
        // Continue - Asset Engine may have stale cache
        // Level resolver will fallback to config if Asset Engine unavailable
    }
}
```

### 3. Invalid Asset Data

```rust
// Validate asset data
if let Some(rules) = &trading_rules {
    if rules.min_order_size <= Decimal::ZERO {
        warn!("Invalid min_order_size from Asset Engine, using config");
        // Fallback to config
    }
}
```

### 4. Cache Stale

```rust
// Asset Engine handles cache refresh internally
// When cache is stale, Asset Engine can:
// 1. Use stale data (if available)
// 2. Trigger background refresh
// 3. Fallback to config if refresh fails
```

## Performance Optimizations

1. **TTL-Based Caching**
   - Cache market data for 5-10 minutes
   - Avoid repeated API calls
   - Refresh on-demand when stale

2. **Background Refresh**
   - Refresh cache before expiration
   - Non-blocking refresh
   - Use stale data if refresh fails

3. **Lazy Fetching**
   - Fetch only when needed
   - Cache results
   - Reuse cached data

4. **Connection Pooling**
   - Reuse HTTP connections
   - Connection pool for REST client
   - Efficient request handling

## Testing Strategy

1. **Unit Tests**
   - TradingClient trait implementation
   - Market data parsing
   - Priority resolution logic
   - Fallback behavior

2. **Integration Tests**
   - Bootstrap with market data
   - Level creation with exchange data
   - Level creation with fallback to config
   - Level execution with exchange rules

3. **Mock Tests**
   - Mock exchange REST responses
   - Test priority system
   - Test fallback scenarios

## Success Criteria

- [ ] Asset Engine service created
- [ ] Asset Engine uses public REST client (unsigned calls only)
- [ ] Binance implementation in Asset Engine
- [ ] Asset Engine caches data with TTL
- [ ] Market data fetched at bootstrap via Asset Engine
- [ ] Exchange values take priority over config
- [ ] Fallback to config when Asset Engine data unavailable
- [ ] Level resolver uses Asset Engine trading rules
- [ ] Quantities rounded to step size (from Asset Engine)
- [ ] Prices rounded to tick size (from Asset Engine)
- [ ] Min/max validations use Asset Engine limits
- [ ] System works with or without Asset Engine data

## Future Enhancements (Out of Scope)

- Caching market data (for now, fetch fresh each time)
- Periodic refresh of market data
- Market data versioning
- Historical market data storage

## Notes

- **Separate Asset Engine service** - dedicated to public asset data
- **Public REST calls only** - no authentication required
- **Fetched at bootstrap** - via Asset Engine
- **Exchange values always priority** - most accurate source
- **Config/UI as fallback** - when Asset Engine data unavailable
- **TTL-based caching** - Cache for 5-10 minutes, refresh on-demand
- **Used in level resolver** - for price/quantity resolution and validation
- **Background refresh** - Asset Engine handles cache refresh
- **Stale data fallback** - Use stale cache if fresh fetch fails
- **Follows service pattern** - Similar to Market Data Engine and Exchange Engine
- **Reusable** - Asset data can be used by config validation, level creation, etc.

