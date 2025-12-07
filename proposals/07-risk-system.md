# Proposal 07: Risk System

## Overview

This proposal outlines the implementation of a comprehensive risk management system within the trading engine. The risk system prevents wipeouts and bad trading decisions through a set of rules that check position sizes, daily loss limits, balance thresholds, and exposure limits. It performs periodic, pre-trade, and post-trade checks, with circuit breakers for critical breaches and graceful degradation for non-critical issues.

## Key Requirements

1. **Risk Rules**: Position size limits (total), daily loss limits (total), balance thresholds, exposure limits
2. **Risk Actions**: Critical breaches stop trading and cancel orders; non-critical breaches log warnings and stop posting
3. **Risk Metrics**: Real-time P&L (unrealized), realized P&L, exposure
4. **Risk Checks**: Periodic checks, pre-trade checks, post-trade checks
5. **Configuration**: Global limits (in code), dynamic limits (based on conditions)
6. **Circuit Breakers**: Emergency stops, graceful degradation, recovery procedures

## Architecture Overview

### Risk System Flow

```
Trading Decision
    ↓
Pre-Trade Risk Check
    ├── Position Limits ✓
    ├── Daily Loss Limits ✓
    ├── Balance Thresholds ✓
    └── Exposure Limits ✓
    ↓
[Pass] → Place Order
[Fail] → Block Order + Action (warn/cancel/stop)

Post-Trade Risk Check
    ├── Update Metrics
    ├── Check Limits
    └── Trigger Actions if Needed

Periodic Risk Check
    ├── Calculate Metrics
    ├── Check All Limits
    └── Trigger Circuit Breakers if Needed
```

### Component Structure

```
trading_engine/
├── risk/
│   ├── mod.rs
│   ├── manager.rs              # RiskManager - orchestrates all checks
│   ├── rules/
│   │   ├── mod.rs
│   │   ├── position_limits.rs   # Position size limits (total)
│   │   ├── loss_limits.rs       # Daily loss limits (total)
│   │   ├── balance_limits.rs    # Balance thresholds
│   │   └── exposure_limits.rs   # Exposure/notional limits
│   ├── metrics/
│   │   ├── mod.rs
│   │   ├── pnl_calculator.rs    # Real-time & realized P&L
│   │   └── exposure_calculator.rs # Exposure calculations
│   ├── actions/
│   │   ├── mod.rs
│   │   ├── blocker.rs           # Block order placement
│   │   ├── canceller.rs         # Cancel all orders
│   │   └── stopper.rs           # Stop trading (circuit breaker)
│   ├── circuit_breaker.rs       # Circuit breaker logic
│   ├── config.rs                # Risk configuration
│   ├── state.rs                 # Risk state tracking
│   └── checker.rs               # Pre/post/periodic checks
```

## Detailed Component Design

### 1. Risk Manager (`risk/manager.rs`)

**Purpose**: Main orchestrator for all risk checks and actions.

```rust
pub struct RiskManager {
    rules: RiskRules,
    metrics: RiskMetrics,
    metrics_calculator: RiskMetricsCalculator,
    metrics_cache: Arc<RwLock<CachedMetrics>>,  // Cached metrics
    actions: RiskActions,
    circuit_breaker: CircuitBreaker,
    state: Arc<RwLock<RiskState>>,
    config: RiskConfig,
}

/// Cached metrics to avoid full recalculation
pub struct CachedMetrics {
    metrics: RiskMetrics,
    last_calculated: Instant,
    cache_ttl: Duration,
    last_trading_state_hash: u64,  // Hash to detect state changes
}

impl RiskManager {
    /// Pre-trade risk check (uses cached metrics if available)
    pub async fn check_pre_trade(
        &self,
        order_request: &OrderRequest,
        trading_state: &TradingState,
        balances: &[Balance],
    ) -> RiskCheckResult {
        // Get cached metrics or calculate if stale
        let metrics = self.get_or_calculate_metrics(trading_state, balances).await;
        
        // Perform risk checks with cached metrics
        // ...
    }
    
    /// Post-trade risk check (incremental update)
    pub async fn check_post_trade(
        &self,
        order: &Order,
        trading_state: &TradingState,
    ) -> RiskCheckResult {
        // Incrementally update metrics (don't recalculate everything)
        self.update_metrics_incremental(order, trading_state).await;
        
        // Perform risk checks
        // ...
    }
    
    /// Periodic risk check (uses cached metrics)
    pub async fn check_periodic(
        &self,
        trading_state: &TradingState,
        balances: &[Balance],
    ) -> RiskCheckResult {
        // Get cached metrics
        let metrics = self.get_or_calculate_metrics(trading_state, balances).await;
        
        // Perform risk checks
        // ...
    }
    
    /// Get cached metrics or calculate if stale
    async fn get_or_calculate_metrics(
        &self,
        trading_state: &TradingState,
        balances: &[Balance],
    ) -> RiskMetrics {
        let cache = self.metrics_cache.read().unwrap();
        
        // Check if cache is valid
        if cache.is_valid(trading_state) {
            return cache.metrics.clone();
        }
        
        drop(cache);
        
        // Cache is stale, recalculate
        let metrics = self.metrics_calculator.calculate(
            trading_state,
            balances,
            // ... current prices
        );
        
        // Update cache
        let mut cache = self.metrics_cache.write().unwrap();
        cache.update(metrics.clone(), trading_state);
        
        metrics
    }
    
    /// Incrementally update metrics (faster than full recalculation)
    async fn update_metrics_incremental(
        &self,
        order: &Order,
        trading_state: &TradingState,
    ) {
        let mut cache = self.metrics_calculator.write().unwrap();
        
        // Update only affected metrics
        match order.side {
            Side::Buy => {
                // Increment exposure, update P&L, etc.
                cache.metrics.total_exposure += order.amount * order.price;
            }
            Side::Sell => {
                // Decrement exposure, update P&L, etc.
                cache.metrics.total_exposure -= order.amount * order.price;
            }
        }
        
        cache.last_calculated = Instant::now();
    }
    
    /// Get current risk state
    pub fn get_state(&self) -> Arc<RwLock<RiskState>>;
    
    /// Update dynamic limits based on conditions
    pub fn update_dynamic_limits(&self, conditions: &RiskConditions);
}
```

### 2. Risk Check Result (`risk/manager.rs`)

```rust
#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    /// Whether the check passed
    pub allowed: bool,
    
    /// Severity of the breach (if any)
    pub severity: RiskSeverity,
    
    /// Reason for rejection (if any)
    pub reason: Option<String>,
    
    /// Action to take
    pub action: RiskAction,
    
    /// Warnings (non-blocking issues)
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskSeverity {
    /// No issue
    None,
    
    /// Warning only (non-critical)
    Warning,
    
    /// Critical breach - block trading
    Critical,
    
    /// Emergency - stop everything
    Emergency,
}

#[derive(Debug, Clone)]
pub enum RiskAction {
    /// Allow trading to continue
    Allow,
    
    /// Block this specific order
    BlockOrder,
    
    /// Log warning and stop posting new orders
    WarnAndStopPosting,
    
    /// Cancel all orders and stop trading
    CancelAllAndStop,
    
    /// Emergency stop - circuit breaker
    EmergencyStop,
}
```

### 3. Risk Rules (`risk/rules/mod.rs`)

**Purpose**: Individual risk rule implementations.

```rust
pub trait RiskRule: Send + Sync {
    /// Check if rule is satisfied
    fn check(
        &self,
        context: &RiskCheckContext,
    ) -> RuleResult;
    
    /// Get rule name
    fn name(&self) -> &str;
    
    /// Check if rule is enabled
    fn is_enabled(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct RuleResult {
    pub passed: bool,
    pub severity: RiskSeverity,
    pub message: Option<String>,
    pub current_value: Option<Decimal>,
    pub limit_value: Option<Decimal>,
}

pub struct RiskRules {
    position_limits: PositionLimitsRule,
    loss_limits: LossLimitsRule,
    balance_limits: BalanceLimitsRule,
    exposure_limits: ExposureLimitsRule,
}

impl RiskRules {
    /// Check all rules
    pub fn check_all(&self, context: &RiskCheckContext) -> Vec<RuleResult>;
}
```

### 4. Position Limits Rule (`risk/rules/position_limits.rs`)

**Purpose**: Enforces total position size limits.

```rust
pub struct PositionLimitsRule {
    /// Maximum total position size (in base currency)
    max_total_position: Decimal,
    
    /// Dynamic limit (can be adjusted)
    dynamic_limit: Arc<RwLock<Option<Decimal>>>,
    
    enabled: bool,
}

impl RiskRule for PositionLimitsRule {
    fn check(&self, context: &RiskCheckContext) -> RuleResult {
        let current_total = self.calculate_total_position(context.trading_state);
        let limit = self.dynamic_limit.read().unwrap()
            .unwrap_or(self.max_total_position);
        
        if current_total >= limit {
            RuleResult {
                passed: false,
                severity: RiskSeverity::Critical,
                message: Some(format!(
                    "Total position {} exceeds limit {}",
                    current_total, limit
                )),
                current_value: Some(current_total),
                limit_value: Some(limit),
            }
        } else {
            RuleResult {
                passed: true,
                severity: RiskSeverity::None,
                message: None,
                current_value: Some(current_total),
                limit_value: Some(limit),
            }
        }
    }
}

impl PositionLimitsRule {
    fn calculate_total_position(&self, state: &TradingState) -> Decimal {
        state.positions.values()
            .map(|p| p.quantity)
            .sum()
    }
}
```

### 5. Loss Limits Rule (`risk/rules/loss_limits.rs`)

**Purpose**: Enforces daily loss limits.

```rust
pub struct LossLimitsRule {
    /// Maximum daily loss (in quote currency)
    max_daily_loss: Decimal,
    
    /// Dynamic limit (can be adjusted)
    dynamic_limit: Arc<RwLock<Option<Decimal>>>,
    
    enabled: bool,
}

impl RiskRule for LossLimitsRule {
    fn check(&self, context: &RiskCheckContext) -> RuleResult {
        let daily_pnl = context.risk_metrics.daily_pnl;
        
        // Daily loss is negative P&L
        let daily_loss = if daily_pnl < Decimal::ZERO {
            -daily_pnl
        } else {
            Decimal::ZERO
        };
        
        let limit = self.dynamic_limit.read().unwrap()
            .unwrap_or(self.max_daily_loss);
        
        if daily_loss >= limit {
            RuleResult {
                passed: false,
                severity: RiskSeverity::Emergency,  // Critical - stop everything
                message: Some(format!(
                    "Daily loss {} exceeds limit {}",
                    daily_loss, limit
                )),
                current_value: Some(daily_loss),
                limit_value: Some(limit),
            }
        } else {
            RuleResult {
                passed: true,
                severity: RiskSeverity::None,
                message: None,
                current_value: Some(daily_loss),
                limit_value: Some(limit),
            }
        }
    }
}
```

### 6. Balance Limits Rule (`risk/rules/balance_limits.rs`)

**Purpose**: Enforces balance thresholds.

```rust
pub struct BalanceLimitsRule {
    /// Minimum balance required (in quote currency)
    min_balance: Decimal,
    
    /// Reserve balance (cannot be used for trading)
    reserve_balance: Decimal,
    
    enabled: bool,
}

impl RiskRule for BalanceLimitsRule {
    fn check(&self, context: &RiskCheckContext) -> RuleResult {
        let available_balance = self.calculate_available_balance(context.balances);
        let required = self.min_balance + self.reserve_balance;
        
        if available_balance < required {
            RuleResult {
                passed: false,
                severity: RiskSeverity::Warning,  // Non-critical - just stop posting
                message: Some(format!(
                    "Available balance {} is below threshold {}",
                    available_balance, required
                )),
                current_value: Some(available_balance),
                limit_value: Some(required),
            }
        } else {
            RuleResult {
                passed: true,
                severity: RiskSeverity::None,
                message: None,
                current_value: Some(available_balance),
                limit_value: Some(required),
            }
        }
    }
}

impl BalanceLimitsRule {
    fn calculate_available_balance(&self, balances: &[Balance]) -> Decimal {
        balances.iter()
            .filter(|b| b.symbol.as_ref() == "USDT" || b.symbol.as_ref().ends_with("USDT"))
            .map(|b| b.free)
            .sum()
    }
}
```

### 7. Exposure Limits Rule (`risk/rules/exposure_limits.rs`)

**Purpose**: Enforces exposure/notional value limits.

```rust
pub struct ExposureLimitsRule {
    /// Maximum total exposure (notional value)
    max_exposure: Decimal,
    
    /// Dynamic limit (can be adjusted)
    dynamic_limit: Arc<RwLock<Option<Decimal>>>,
    
    enabled: bool,
}

impl RiskRule for ExposureLimitsRule {
    fn check(&self, context: &RiskCheckContext) -> RuleResult {
        let current_exposure = context.risk_metrics.total_exposure;
        let limit = self.dynamic_limit.read().unwrap()
            .unwrap_or(self.max_exposure);
        
        if current_exposure >= limit {
            RuleResult {
                passed: false,
                severity: RiskSeverity::Critical,
                message: Some(format!(
                    "Total exposure {} exceeds limit {}",
                    current_exposure, limit
                )),
                current_value: Some(current_exposure),
                limit_value: Some(limit),
            }
        } else {
            RuleResult {
                passed: true,
                severity: RiskSeverity::None,
                message: None,
                current_value: Some(current_exposure),
                limit_value: Some(limit),
            }
        }
    }
}
```

### 8. Risk Metrics (`risk/metrics/mod.rs`)

**Purpose**: Calculates and tracks risk metrics with caching.

```rust
pub struct RiskMetrics {
    /// Real-time P&L (unrealized)
    pub unrealized_pnl: Decimal,
    
    /// Realized P&L (from closed positions)
    pub realized_pnl: Decimal,
    
    /// Total P&L (unrealized + realized)
    pub total_pnl: Decimal,
    
    /// Daily P&L (tracked per day)
    pub daily_pnl: Decimal,
    
    /// Total exposure (notional value of all positions)
    pub total_exposure: Decimal,
    
    /// Total position size
    pub total_position: Decimal,
    
    /// Last calculated timestamp
    pub timestamp: u64,
}

pub struct RiskMetricsCalculator {
    cache: Arc<RwLock<CachedMetrics>>,
    cache_ttl: Duration,
}

impl RiskMetricsCalculator {
    /// Calculate all risk metrics (with caching)
    pub fn calculate(
        &self,
        trading_state: &TradingState,
        balances: &[Balance],
        current_prices: &HashMap<Pair, Decimal>,
        daily_pnl_tracker: &DailyPnLTracker,
    ) -> RiskMetrics {
        // Check cache first
        let cache = self.cache.read().unwrap();
        if cache.is_valid(trading_state) {
            return cache.metrics.clone();
        }
        drop(cache);
        
        // Cache miss or stale - calculate
        let metrics = self.calculate_full(trading_state, balances, current_prices, daily_pnl_tracker);
        
        // Update cache
        let mut cache = self.cache.write().unwrap();
        cache.update(metrics.clone(), trading_state);
        
        metrics
    }
    
    /// Full calculation (expensive)
    fn calculate_full(
        &self,
        trading_state: &TradingState,
        balances: &[Balance],
        current_prices: &HashMap<Pair, Decimal>,
        daily_pnl_tracker: &DailyPnLTracker,
    ) -> RiskMetrics;
    
    /// Incremental update (fast)
    pub fn update_incremental(
        &self,
        delta: &MetricsDelta,
    ) -> Result<(), RiskError>;
}

/// Delta for incremental updates
pub struct MetricsDelta {
    pub exposure_delta: Decimal,
    pub position_delta: Decimal,
    pub pnl_delta: Decimal,
}
    
    /// Calculate unrealized P&L
    fn calculate_unrealized_pnl(
        &self,
        positions: &HashMap<Pair, Position>,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> Decimal;
    
    /// Calculate total exposure
    fn calculate_exposure(
        &self,
        positions: &HashMap<Pair, Position>,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> Decimal;
    
    /// Calculate total position size
    fn calculate_total_position(
        &self,
        positions: &HashMap<Pair, Position>,
    ) -> Decimal;
}
```

### 9. Daily P&L Tracker (`risk/metrics/pnl_calculator.rs`)

**Purpose**: Tracks daily P&L separately.

```rust
pub struct DailyPnLTracker {
    /// Current day's P&L
    daily_pnl: Decimal,
    
    /// Last reset date
    last_reset_date: String,  // YYYY-MM-DD format
    
    /// Daily P&L history
    daily_history: Vec<DailyPnLEntry>,
}

#[derive(Debug, Clone)]
pub struct DailyPnLEntry {
    pub date: String,
    pub pnl: Decimal,
    pub realized: Decimal,
    pub unrealized: Decimal,
}

impl DailyPnLTracker {
    /// Update daily P&L
    pub fn update(&mut self, realized_pnl: Decimal, unrealized_pnl: Decimal);
    
    /// Reset for new day
    pub fn reset_if_new_day(&mut self);
    
    /// Get current daily P&L
    pub fn get_daily_pnl(&self) -> Decimal;
    
    /// Get daily history
    pub fn get_history(&self) -> &[DailyPnLEntry];
}
```

### 10. Exposure Calculator (`risk/metrics/exposure_calculator.rs`)

**Purpose**: Calculates exposure/notional value.

```rust
pub struct ExposureCalculator;

impl ExposureCalculator {
    /// Calculate total exposure
    pub fn calculate_total(
        &self,
        positions: &HashMap<Pair, Position>,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> Decimal {
        positions.iter()
            .filter_map(|(pair, position)| {
                current_prices.get(pair).map(|price| {
                    position.quantity * *price
                })
            })
            .sum()
    }
    
    /// Calculate exposure for a specific pair
    pub fn calculate_for_pair(
        &self,
        pair: &Pair,
        position: &Position,
        current_price: Decimal,
    ) -> Decimal {
        position.quantity * current_price
    }
}
```

### 11. Risk Actions (`risk/actions/mod.rs`)

**Purpose**: Executes actions based on risk check results.

```rust
pub struct RiskActions {
    blocker: OrderBlocker,
    canceller: OrderCanceller,
    stopper: TradingStopper,
}

impl RiskActions {
    /// Execute action based on risk check result
    pub async fn execute(
        &self,
        action: &RiskAction,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        level_manager: Arc<RwLock<LevelManager>>,
    ) -> Result<(), RiskError>;
}

pub struct OrderBlocker;

impl OrderBlocker {
    /// Block order placement
    pub fn block(&self, reason: &str) -> Result<(), RiskError>;
}

pub struct OrderCanceller {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
}

impl OrderCanceller {
    /// Cancel all orders
    pub async fn cancel_all(
        &self,
        pairs: &[Pair],
    ) -> Result<(), RiskError>;
}

pub struct TradingStopper {
    level_manager: Arc<RwLock<LevelManager>>,
}

impl TradingStopper {
    /// Stop all trading (pause all levels)
    pub async fn stop_all(&self) -> Result<(), RiskError>;
    
    /// Resume trading (enable all levels)
    pub async fn resume_all(&self) -> Result<(), RiskError>;
}
```

### 12. Circuit Breaker (`risk/circuit_breaker.rs`)

**Purpose**: Emergency stops and recovery.

```rust
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    conditions: Vec<CircuitBreakerCondition>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    /// Normal operation
    Closed,
    
    /// Circuit open - trading stopped
    Open,
    
    /// Testing recovery
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerCondition {
    /// Condition name
    pub name: String,
    
    /// Check function
    pub check: Box<dyn Fn(&RiskMetrics) -> bool + Send + Sync>,
    
    /// Action to take when triggered
    pub action: RiskAction,
    
    /// Recovery condition
    pub recovery_check: Option<Box<dyn Fn(&RiskMetrics) -> bool + Send + Sync>>,
}

impl CircuitBreaker {
    /// Check circuit breaker conditions
    pub async fn check(
        &self,
        metrics: &RiskMetrics,
    ) -> Option<RiskAction>;
    
    /// Open circuit breaker
    pub fn open(&self, reason: &str);
    
    /// Close circuit breaker
    pub fn close(&self);
    
    /// Get current state
    pub fn get_state(&self) -> CircuitBreakerState;
}
```

### 13. Risk State (`risk/state.rs`)

**Purpose**: Tracks current risk state.

```rust
pub struct RiskState {
    /// Current risk metrics
    pub metrics: RiskMetrics,
    
    /// Circuit breaker state
    pub circuit_breaker_state: CircuitBreakerState,
    
    /// Last check timestamp
    pub last_check: u64,
    
    /// Active breaches
    pub active_breaches: Vec<BreachRecord>,
    
    /// Risk check history
    pub check_history: Vec<RiskCheckRecord>,
}

#[derive(Debug, Clone)]
pub struct BreachRecord {
    pub rule_name: String,
    pub severity: RiskSeverity,
    pub message: String,
    pub timestamp: u64,
    pub resolved: bool,
}

#[derive(Debug, Clone)]
pub struct RiskCheckRecord {
    pub timestamp: u64,
    pub check_type: CheckType,
    pub result: RiskCheckResult,
}

#[derive(Debug, Clone)]
pub enum CheckType {
    PreTrade,
    PostTrade,
    Periodic,
}
```

### 14. Risk Checker (`risk/checker.rs`)

**Purpose**: Orchestrates pre-trade, post-trade, and periodic checks with optimizations.

```rust
pub struct RiskChecker {
    risk_manager: Arc<RiskManager>,
    check_cache: Arc<RwLock<CheckCache>>,  // Cache recent check results
}

/// Cache for recent risk check results (avoid redundant checks)
pub struct CheckCache {
    recent_checks: HashMap<String, (RiskCheckResult, Instant)>,  // Key: order_request_hash
    cache_ttl: Duration,
}

impl RiskChecker {
    /// Pre-trade check (with caching for identical requests)
    pub async fn check_pre_trade(
        &self,
        order_request: &OrderRequest,
        trading_state: &TradingState,
        balances: &[Balance],
    ) -> RiskCheckResult {
        // Check cache for identical requests
        let cache_key = self.hash_order_request(order_request);
        if let Some((cached_result, timestamp)) = self.check_cache.read().unwrap().get(&cache_key) {
            if timestamp.elapsed() < Duration::from_millis(100) {
                // Return cached result if recent
                return cached_result.clone();
            }
        }
        
        // Perform check (uses cached metrics)
        let result = self.risk_manager.check_pre_trade(
            order_request,
            trading_state,
            balances,
        ).await;
        
        // Cache result
        {
            let mut cache = self.check_cache.write().unwrap();
            cache.insert(cache_key, result.clone());
        }
        
        // Execute action if needed
        if !result.allowed {
            self.risk_manager.actions.execute(
                &result.action,
                // ... trading client, level manager
            ).await?;
        }
        
        result
    }
    
    /// Post-trade check (incremental update)
    pub async fn check_post_trade(
        &self,
        order: &Order,
        trading_state: &TradingState,
    ) -> RiskCheckResult {
        // Incrementally update metrics (fast)
        self.risk_manager.update_metrics_incremental(order, trading_state).await;
        
        // Perform check with updated metrics
        self.risk_manager.check_post_trade(order, trading_state).await
    }
    
    /// Periodic check (runs in background, uses cached metrics)
    pub async fn start_periodic_checks(
        &self,
        interval_secs: u64,
    ) -> Result<(), RiskError> {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        
        loop {
            interval.tick().await;
            
            // Get cached metrics (fast)
            let metrics = self.risk_manager.get_cached_metrics().await;
            
            // Perform checks with cached metrics
            let result = self.risk_manager.check_periodic(
                // ... trading_state, balances
            ).await;
            
            // Handle result
            if !result.allowed {
                self.risk_manager.actions.execute(&result.action, ...).await?;
            }
        }
    }
}
```

**Performance Optimizations**:
- **Metrics caching**: Cache metrics with TTL (e.g., 1 second)
- **Incremental updates**: Update only changed metrics (post-trade)
- **Check result caching**: Cache identical check requests
- **State change detection**: Only recalculate if trading state changed
- **Async processing**: Non-blocking risk checks
```

### 15. Risk Configuration (`risk/config.rs`)

**Purpose**: Risk system configuration.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Global position limits
    pub position_limits: PositionLimitsConfig,
    
    /// Global loss limits
    pub loss_limits: LossLimitsConfig,
    
    /// Balance thresholds
    pub balance_limits: BalanceLimitsConfig,
    
    /// Exposure limits
    pub exposure_limits: ExposureLimitsConfig,
    
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
    
    /// Check intervals
    pub check_intervals: CheckIntervalsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionLimitsConfig {
    /// Maximum total position size
    pub max_total_position: Decimal,
    
    /// Enable dynamic limits
    pub enable_dynamic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossLimitsConfig {
    /// Maximum daily loss
    pub max_daily_loss: Decimal,
    
    /// Enable dynamic limits
    pub enable_dynamic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceLimitsConfig {
    /// Minimum balance required
    pub min_balance: Decimal,
    
    /// Reserve balance (cannot be used)
    pub reserve_balance: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureLimitsConfig {
    /// Maximum total exposure
    pub max_exposure: Decimal,
    
    /// Enable dynamic limits
    pub enable_dynamic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Enable circuit breaker
    pub enabled: bool,
    
    /// Conditions that trigger circuit breaker
    pub conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckIntervalsConfig {
    /// Periodic check interval (seconds)
    pub periodic_interval_secs: u64,
}
```

## Integration Points

### 1. Pre-Trade Integration

```rust
// In level executor or trading engine
let risk_check = risk_checker.check_pre_trade(
    &order_request,
    &trading_state,
    &balances,
).await;

if !risk_check.allowed {
    match risk_check.severity {
        RiskSeverity::Warning => {
            warn!("Risk check failed: {}", risk_check.reason.unwrap());
            // Stop posting this order
        }
        RiskSeverity::Critical | RiskSeverity::Emergency => {
            error!("Critical risk breach: {}", risk_check.reason.unwrap());
            // Cancel all orders and stop trading
        }
        _ => {}
    }
    return Err(EngineError::RiskCheckFailed);
}
```

### 2. Post-Trade Integration

```rust
// After order execution
let risk_check = risk_checker.check_post_trade(
    &order,
    &trading_state,
).await;

if !risk_check.allowed {
    // Execute appropriate action
    risk_manager.actions.execute(&risk_check.action, ...).await?;
}
```

### 3. Periodic Check Integration

```rust
// Start periodic risk checks
risk_checker.start_periodic_checks(60).await?;  // Every 60 seconds

// In background task
loop {
    let metrics = risk_manager.calculate_metrics(...).await;
    let result = risk_manager.check_periodic(&trading_state, &balances).await;
    
    // Check circuit breaker
    if let Some(action) = circuit_breaker.check(&metrics).await {
        risk_manager.actions.execute(&action, ...).await?;
    }
    
    tokio::time::sleep(Duration::from_secs(60)).await;
}
```

## Risk Check Flow

### Pre-Trade Check

```
Order Request
    ↓
RiskManager.check_pre_trade()
    ├── Calculate current metrics
    ├── Check Position Limits
    ├── Check Loss Limits
    ├── Check Balance Limits
    └── Check Exposure Limits
    ↓
[All Pass] → Allow Order
[Any Fail] → Determine Severity
    ├── Warning → Log + Block Order
    ├── Critical → Cancel All + Stop Trading
    └── Emergency → Circuit Breaker
```

### Post-Trade Check

```
Order Executed
    ↓
Update Trading State
    ↓
RiskManager.check_post_trade()
    ├── Recalculate Metrics
    ├── Check All Limits
    └── Trigger Actions if Needed
```

### Periodic Check

```
Every N seconds
    ↓
RiskManager.check_periodic()
    ├── Calculate Metrics
    ├── Check All Limits
    ├── Check Circuit Breaker
    └── Update Risk State
```

## Dynamic Limits

### Conditions-Based Limits

```rust
pub struct RiskConditions {
    /// Market volatility (high/low)
    pub volatility: VolatilityLevel,
    
    /// Current drawdown
    pub drawdown: Decimal,
    
    /// Time of day
    pub time_of_day: TimeOfDay,
    
    /// Market conditions
    pub market_conditions: MarketConditions,
}

impl RiskManager {
    /// Update dynamic limits based on conditions
    pub fn update_dynamic_limits(&self, conditions: &RiskConditions) {
        // Reduce limits in high volatility
        if conditions.volatility == VolatilityLevel::High {
            // Reduce position limits by 50%
            // Reduce exposure limits by 50%
        }
        
        // Reduce limits during drawdown
        if conditions.drawdown > Decimal::new(5, 0) {  // 5%
            // Reduce all limits by 25%
        }
    }
}
```

## Circuit Breaker Conditions

### Default Conditions

```rust
let conditions = vec![
    CircuitBreakerCondition {
        name: "daily_loss_limit".to_string(),
        check: Box::new(|metrics| metrics.daily_pnl < -Decimal::new(500, 0)),
        action: RiskAction::EmergencyStop,
        recovery_check: Some(Box::new(|metrics| metrics.daily_pnl > -Decimal::new(400, 0))),
    },
    CircuitBreakerCondition {
        name: "exposure_limit".to_string(),
        check: Box::new(|metrics| metrics.total_exposure > Decimal::new(10000, 0)),
        action: RiskAction::CancelAllAndStop,
        recovery_check: Some(Box::new(|metrics| metrics.total_exposure < Decimal::new(8000, 0))),
    },
];
```

## Graceful Degradation

### Degradation Levels

```rust
pub enum DegradationLevel {
    /// Normal operation
    Normal,
    
    /// Reduce position sizes by 25%
    Reduced,
    
    /// Reduce position sizes by 50%
    Conservative,
    
    /// Stop posting new orders
    StopPosting,
    
    /// Emergency stop
    Emergency,
}

impl RiskManager {
    /// Apply graceful degradation
    pub fn apply_degradation(&self, level: DegradationLevel) {
        match level {
            DegradationLevel::Reduced => {
                // Reduce all limits by 25%
            }
            DegradationLevel::Conservative => {
                // Reduce all limits by 50%
            }
            DegradationLevel::StopPosting => {
                // Pause all levels
            }
            DegradationLevel::Emergency => {
                // Cancel all orders and stop trading
            }
            _ => {}
        }
    }
}
```

## Recovery Procedures

### Recovery Conditions

```rust
impl CircuitBreaker {
    /// Check if recovery conditions are met
    pub fn check_recovery(&self, metrics: &RiskMetrics) -> bool {
        // Check all recovery conditions
        self.conditions.iter().any(|condition| {
            if let Some(recovery_check) = &condition.recovery_check {
                recovery_check(metrics)
            } else {
                false
            }
        })
    }
    
    /// Attempt recovery
    pub async fn attempt_recovery(
        &self,
        metrics: &RiskMetrics,
        risk_manager: &RiskManager,
    ) -> Result<bool, RiskError> {
        if self.check_recovery(metrics) {
            // Move to half-open state
            self.set_state(CircuitBreakerState::HalfOpen);
            
            // Test with small position
            // If successful, close circuit breaker
            // If failed, open again
            
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
```

## Error Handling

1. **Risk Check Failures**
   - Log error
   - Default to blocking order (fail-safe)
   - Alert on critical errors

2. **Metrics Calculation Failures**
   - Use last known metrics
   - Log error
   - Continue with degraded mode

3. **Action Execution Failures**
   - Retry with exponential backoff
   - Log error
   - Escalate if critical

## Testing Strategy

1. **Unit Tests**
   - Individual risk rules
   - Metrics calculations
   - Action execution

2. **Integration Tests**
   - Pre-trade checks
   - Post-trade checks
   - Periodic checks
   - Circuit breaker

3. **Scenario Tests**
   - Daily loss limit breach
   - Position limit breach
   - Balance threshold breach
   - Exposure limit breach
   - Recovery scenarios

## Success Criteria

- [ ] Pre-trade risk checks prevent bad orders
- [ ] Post-trade risk checks monitor after execution
- [ ] Periodic risk checks run continuously
- [ ] Position limits enforced (total)
- [ ] Daily loss limits enforced (total)
- [ ] Balance thresholds enforced
- [ ] Exposure limits enforced
- [ ] Real-time P&L calculated
- [ ] Realized P&L tracked
- [ ] Exposure calculated
- [ ] Circuit breakers trigger on critical breaches
- [ ] Graceful degradation works
- [ ] Recovery procedures function
- [ ] Dynamic limits adjust based on conditions
- [ ] Actions execute correctly (warn/cancel/stop)

## Future Enhancements (Out of Scope)

- Advanced risk models (VaR, CVaR)
- Portfolio-level risk
- Correlation analysis
- Stress testing
- Risk reporting and analytics
- Machine learning-based risk prediction

## Notes

- **Risk system is part of trading_engine** - not a separate service
- **Pre-trade, post-trade, and periodic checks** - comprehensive coverage
- **Critical breaches stop trading** - cancel all orders
- **Non-critical breaches log warnings** - stop posting
- **Dynamic limits** - adjust based on market conditions
- **Circuit breakers** - emergency stops with recovery
- **Graceful degradation** - reduce activity before full stop
- **Real-time metrics** - P&L, exposure calculated continuously

