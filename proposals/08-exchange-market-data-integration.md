# Proposal 08: Exchange Market Data Integration

## Overview

This proposal outlines the integration of exchange market data (symbol info, trading rules, prices, etc.) into the level creation and management system. Market data is fetched via REST at bootstrap alongside other REST calls (balances, trades, open orders). Exchange values always take priority over config values, with config/UI as fallback. This is implemented as part of the `TradingClient` trait, requiring all exchanges to provide this functionality.

## Key Requirements

1. **Market Data Fetching**: Fetch all market data (symbol info, trading rules, prices, orderbook snapshot, exchange limits) at bootstrap
2. **Trait Requirement**: Part of `TradingClient` trait - all exchanges must implement
3. **Priority System**: Exchange values always take priority over config values
4. **Bootstrap Integration**: Fetch alongside balances, trades, open orders
5. **TTL-Based Caching**: Cache market data with TTL (e.g., 5-10 minutes), refresh on-demand when stale
6. **Fallback Strategy**: Fallback to JSON config or UI if exchange data unavailable

## Architecture Overview

### Data Flow

```
Bootstrap
    ↓
Exchange REST Client
    ├── fetch_balance()
    ├── fetch_open_orders()
    ├── fetch_trades()
    └── fetch_market_data() (NEW)
        ↓
ExchangeStateManager
    └── Store market data
        ↓
Level Manager / Level Resolver
    ├── Check Exchange Market Data (priority)
    └── Fallback to Config/UI
```

### Component Structure

```
trading_system/
├── src/
│   ├── libs/
│   │   ├── exchange/
│   │   │   ├── core/
│   │   │   │   └── traits.rs
│   │   │   │       └── TradingClient trait
│   │   │   │           └── fetch_market_data() (NEW)
│   │   │   └── binance/
│   │   │       ├── rest_client.rs
│   │   │       │   └── fetch_market_data() (NEW implementation)
│   │   │       └── state/
│   │   │           └── manager.rs
│   │   │               └── fetch_market_data() (bootstrap call)
│   │   │
│   │   └── shared/
│   │       └── data_types.rs
│   │           └── ExchangeMarketData types (NEW)
│   │
│   └── trading_engine/
│       └── level/
│           ├── resolver.rs
│           │   └── Use exchange market data (priority)
│           └── config.rs
│               └── Resolve with priority system
```

## Detailed Component Design

### 1. TradingClient Trait Update (`exchange/core/traits.rs`)

**Purpose**: Add market data fetching to the trait contract.

```rust
#[async_trait]
pub trait TradingClient<OrderRequest, InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    OrderRequest: Send + Sync + 'static,
    InstrumentId: Clone + Debug + Send + 'static,
{
    // ... existing methods ...
    
    /// Fetches market data for configured pairs
    ///
    /// This method must be implemented by all exchanges and should return
    /// comprehensive market data including symbol info, trading rules, prices, etc.
    ///
    /// # Parameters
    /// - `pairs`: List of trading pairs to fetch market data for
    ///
    /// # Returns
    /// - `Ok(ExchangeMarketData)`: Market data for the requested pairs
    /// - `Err(EngineError)`: If the fetch fails
    async fn fetch_market_data(
        &self,
        pairs: &[InstrumentId],
    ) -> Result<ExchangeMarketData, EngineError>;
}
```

### 2. Exchange Market Data Types (`shared/data_types.rs`)

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

### 3. Binance REST Client Implementation (`exchange/binance/rest_client.rs`)

**Purpose**: Binance-specific implementation of market data fetching.

```rust
impl TradingClient<OrderRequest> for BinanceClient {
    async fn fetch_market_data(
        &self,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError> {
        // 1. Fetch exchange info (symbols, trading rules)
        let exchange_info = self.fetch_exchange_info().await?;
        
        // 2. Fetch current prices (ticker)
        let prices = self.fetch_ticker_prices(pairs).await?;
        
        // 3. Fetch orderbook snapshots (optional, for depth)
        let orderbooks = self.fetch_orderbook_snapshots(pairs).await?;
        
        // 4. Parse and build ExchangeMarketData
        let market_data = self.build_market_data(
            exchange_info,
            prices,
            orderbooks,
            pairs,
        )?;
        
        Ok(market_data)
    }
}

impl BinanceClient {
    /// Fetch exchange info from Binance API
    async fn fetch_exchange_info(&self) -> Result<BinanceExchangeInfo, EngineError>;
    
    /// Fetch ticker prices for pairs
    async fn fetch_ticker_prices(
        &self,
        pairs: &[Pair],
    ) -> Result<HashMap<Pair, Decimal>, EngineError>;
    
    /// Fetch orderbook snapshots
    async fn fetch_orderbook_snapshots(
        &self,
        pairs: &[Pair],
    ) -> Result<HashMap<Pair, Orderbook>, EngineError>;
    
    /// Build ExchangeMarketData from Binance responses
    fn build_market_data(
        &self,
        exchange_info: BinanceExchangeInfo,
        prices: HashMap<Pair, Decimal>,
        orderbooks: HashMap<Pair, Orderbook>,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError>;
    
    /// Parse trading rules from Binance symbol filters
    fn parse_trading_rules(
        &self,
        symbol: &BinanceSymbol,
    ) -> Result<TradingRules, EngineError>;
}
```

### 4. Exchange State Manager Integration (`exchange/binance/state/manager.rs`)

**Purpose**: Fetch and store market data at bootstrap.

```rust
impl ExchangeStateManager {
    /// Bootstrap: Fetch all initial data including market data
    pub async fn bootstrap(
        &mut self,
        configured_pairs: &[Pair],
    ) -> Result<(), EngineError> {
        // 1. Fetch balances
        let balances = self.trading_client.read().await.fetch_balance().await?;
        
        // 2. Fetch open orders
        let orders = self.fetch_open_orders(configured_pairs).await?;
        
        // 3. Fetch trades (last 24h)
        let trades = self.fetch_trades(configured_pairs).await?;
        
        // 4. Fetch market data (NEW) - with caching
        let market_data = self.fetch_market_data_cached(configured_pairs).await?;
        
        // 5. Store in state
        self.state.write().await.market_data = market_data;
        
        Ok(())
    }
    
    /// Fetch market data with TTL-based caching
    async fn fetch_market_data_cached(
        &self,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError> {
        // Check cache first
        if let Some(cached) = self.get_cached_market_data(pairs) {
            if !cached.is_stale() {
                return Ok(cached.data);
            }
        }
        
        // Cache miss or stale - fetch fresh
        let market_data = self.trading_client.read().await
            .fetch_market_data(pairs).await?;
        
        // Update cache
        self.update_market_data_cache(market_data.clone()).await;
        
        Ok(market_data)
    }
    
    /// Get cached market data if available and not stale
    fn get_cached_market_data(&self, pairs: &[Pair]) -> Option<CachedMarketData>;
    
    /// Update market data cache
    async fn update_market_data_cache(&self, data: ExchangeMarketData);
    
    /// Get trading rules for a pair
    pub fn get_trading_rules(&self, pair: &Pair) -> Option<TradingRules> {
        self.state.read().await
            .market_data
            .trading_rules
            .get(pair)
            .cloned()
    }
    
    /// Get current price for a pair
    pub fn get_current_price(&self, pair: &Pair) -> Option<Decimal> {
        self.state.read().await
            .market_data
            .current_prices
            .get(pair)
            .cloned()
    }
    
    /// Get symbol info for a pair
    pub fn get_symbol_info(&self, pair: &Pair) -> Option<SymbolInfo> {
        self.state.read().await
            .market_data
            .symbol_info
            .get(pair)
            .cloned()
    }
}
```

### 5. Exchange State Update (`exchange/binance/state/state.rs`)

**Purpose**: Store market data in state with caching.

```rust
pub struct ExchangeState {
    // ... existing fields ...
    
    /// Market data (fetched at bootstrap, cached with TTL)
    pub market_data: ExchangeMarketData,
    
    /// Market data cache metadata
    pub market_data_cache: MarketDataCache,
}

pub struct MarketDataCache {
    data: ExchangeMarketData,
    cached_at: Instant,
    ttl: Duration,  // e.g., 5-10 minutes
}

impl MarketDataCache {
    /// Check if cache is still valid
    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }
    
    /// Check if cache is stale
    pub fn is_stale(&self) -> bool {
        !self.is_valid()
    }
}
```

### 6. Level Resolver Update (`trading_engine/level/resolver.rs`)

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

### 7. Level Config Priority System (`trading_engine/level/config.rs`)

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
// In runner/main.rs or exchange engine bootstrap
let mut exchange_state_manager = ExchangeStateManager::new(...);

// Bootstrap: Fetch all data including market data (with caching)
exchange_state_manager.bootstrap(&configured_pairs).await?;

// Now market data is available for level creation
let level_manager = LevelManager::new(
    exchange_state_manager.clone(),  // Pass for market data access
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
        
        // Check if cache is stale
        if exchange_state_manager.is_market_data_stale() {
            // Refresh market data
            let market_data = exchange_state_manager
                .fetch_market_data_cached(&configured_pairs)
                .await?;
            
            // Update state
            exchange_state_manager.update_market_data(market_data).await?;
        }
    }
});
```

### 2. Level Creation Integration

```rust
// In level manager
impl LevelManager {
    pub fn create_level(
        &mut self,
        config: LevelConfig,
        exchange_data: Option<&ExchangeMarketData>,
    ) -> Result<Level, EngineError> {
        // Resolve level config with priority
        let resolved = config.resolve_with_priority(exchange_data, &config.symbol);
        
        // Create level with resolved values
        Level::new(resolved)
    }
}
```

### 3. Level Execution Integration

```rust
// In level executor
impl LevelExecutor {
    pub async fn resolve_level(
        &self,
        level: &Level,
        indicators: &HashMap<String, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<ResolvedLevel, EngineError> {
        // Get exchange market data
        let market_data = self.exchange_state_manager
            .get_market_data(&level.config.symbol);
        
        // Resolve with priority
        let price = self.resolver.resolve_price(
            &level.config.price,
            &level.config.symbol,
            indicators,
            orderbook,
        )?;
        
        let quantity = self.resolver.resolve_quantity(
            &level.config.quantity,
            &level.config.symbol,
            balance,
            price,
        )?;
        
        Ok(ResolvedLevel { price, quantity, ... })
    }
}
```

## Error Handling

### 1. Exchange Data Unavailable

```rust
// In level resolver
if let Some(rules) = self.get_trading_rules(pair) {
    // Use exchange data
} else {
    // Fallback to config
    warn!("Exchange market data unavailable for {}, using config values", pair);
    // Use config values
}
```

### 2. Exchange Data Fetch Failure

```rust
// In bootstrap
match trading_client.fetch_market_data(&pairs).await {
    Ok(market_data) => {
        // Store in state with cache metadata
        state.market_data = market_data;
        state.market_data_cache.update(Instant::now());
    }
    Err(e) => {
        warn!("Failed to fetch market data: {}. Levels will use config values.", e);
        // Continue with empty market data - will fallback to config
        // Try to use cached data if available (even if stale)
        if let Some(cached) = state.market_data_cache.get_stale() {
            warn!("Using stale cached market data");
            state.market_data = cached;
        }
    }
}
```

### 3. Invalid Exchange Data

```rust
// Validate exchange data
if let Some(rules) = &trading_rules {
    if rules.min_order_size <= Decimal::ZERO {
        warn!("Invalid min_order_size from exchange, using config");
        // Fallback to config
    }
}
```

### 4. Cache Stale

```rust
// When cache is stale, refresh in background
if market_data_cache.is_stale() {
    // Use stale data for now (better than nothing)
    // Trigger background refresh
    tokio::spawn(async move {
        if let Ok(fresh_data) = fetch_market_data().await {
            update_cache(fresh_data).await;
        }
    });
}
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

- [ ] `fetch_market_data()` added to `TradingClient` trait
- [ ] All exchanges implement `fetch_market_data()`
- [ ] Market data fetched at bootstrap
- [ ] Exchange values take priority over config
- [ ] Fallback to config when exchange data unavailable
- [ ] Level resolver uses exchange trading rules
- [ ] Quantities rounded to step size (from exchange)
- [ ] Prices rounded to tick size (from exchange)
- [ ] Min/max validations use exchange limits
- [ ] System works with or without exchange data

## Future Enhancements (Out of Scope)

- Caching market data (for now, fetch fresh each time)
- Periodic refresh of market data
- Market data versioning
- Historical market data storage

## Notes

- **Part of TradingClient trait** - all exchanges must implement
- **Fetched at bootstrap** - alongside balances, trades, orders
- **Exchange values always priority** - most accurate source
- **Config/UI as fallback** - when exchange data unavailable
- **TTL-based caching** - Cache for 5-10 minutes, refresh on-demand
- **Integrated in exchange engine** - not a separate system
- **Used in level resolver** - for price/quantity resolution and validation
- **Background refresh** - Refresh cache before expiration
- **Stale data fallback** - Use stale cache if fresh fetch fails

