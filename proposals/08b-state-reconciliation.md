# Proposal 08b: State Reconciliation in Exchange Engine

## Overview

This proposal outlines the implementation of state reconciliation logic within the Exchange Engine. The reconciler ensures that the in-memory state (maintained from WebSocket streams) stays synchronized with the exchange's authoritative state by periodically fetching data via REST API and reconciling any discrepancies. This is not a separate service but a component that lives inside the Exchange Engine.

## Key Requirements

1. **Bootstrap Reconciliation**: At startup, fetch balances, open orders, and trades via REST API to initialize state
2. **Periodic Reconciliation**: Periodically (configurable interval) fetch REST data and reconcile with WebSocket state
3. **Discrepancy Detection**: Detect and log discrepancies between REST and WebSocket data
4. **State Correction**: Update in-memory state to match exchange authoritative state (REST takes priority)
5. **Configurable Interval**: Reconciliation interval configurable via system config
6. **Error Handling**: Graceful handling of reconciliation failures without disrupting trading
7. **Trait Requirements**: All exchanges must implement mandatory methods for fetching balances, open orders, and trades

## Architecture Overview

### Component Structure

```
Exchange Engine
├── State Manager
│   ├── In-Memory State (from WebSocket)
│   │   ├── Balances
│   │   ├── Open Orders
│   │   └── Trades
│   └── State Reconciler (NEW)
│       ├── Bootstrap Reconciliation
│       ├── Periodic Reconciliation
│       └── Discrepancy Resolution
└── Trading Client
    ├── fetch_balance() (mandatory)
    ├── fetch_open_orders() (mandatory)
    └── fetch_trades() (mandatory)
```

### Data Flow

```
Bootstrap
    ↓
State Reconciler
    ├── fetch_balance() → Initialize balances
    ├── fetch_open_orders() → Initialize orders
    └── fetch_trades() → Initialize trade history
        ↓
State Manager
    └── Store initial state
        ↓
WebSocket Stream
    └── Update state in real-time
        ↓
Periodic Reconciliation (every N seconds)
    ├── fetch_balance() → Compare with WS state
    ├── fetch_open_orders() → Compare with WS state
    └── fetch_trades() → Compare with WS state
        ↓
Discrepancy Detection
    └── Log discrepancies
        ↓
State Correction
    └── Update state to match REST (authoritative)
```

## Detailed Component Design

### 1. TradingClient Trait Updates (`libs/trading_core/src/traits/trading.rs`)

**Purpose**: Add mandatory methods for state reconciliation.

```rust
#[async_trait]
pub trait TradingClient<OrderRequestType = OrderRequest, InstrumentId = Pair>:
    Debug + Send + Sync + 'static
where
    OrderRequestType: Send + Sync + 'static,
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    // ... existing methods ...
    
    /// Fetches all open orders for the account.
    /// This method should be called at bootstrap and periodically for reconciliation.
    /// Returns all open orders regardless of trading pair.
    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError>;

    /// Fetches private trade history for the account.
    /// This method should be called at bootstrap and periodically for reconciliation.
    /// Optionally filters by trading pair if provided.
    /// 
    /// # Parameters
    /// - `pair`: Optional trading pair to filter trades. If None, returns all trades.
    /// 
    /// # Returns
    /// - `Ok(Vec<Trade>)`: List of trades, optionally filtered by pair
    /// - `Err(EngineError)`: If the fetch fails
    async fn fetch_trades(&self, pair: Option<&InstrumentId>) -> Result<Vec<Trade>, EngineError>;
}
```

### 2. MarketDataClient Trait Updates (`libs/trading_core/src/traits/market_data.rs`)

**Purpose**: Add mandatory method for market data snapshot fetching.

```rust
#[async_trait]
pub trait MarketDataClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    // ... existing methods ...
    
    /// Fetches market data snapshot (orderbook and recent trades) for a trading pair.
    /// This method should be called at bootstrap and periodically for reconciliation.
    /// Returns both orderbook and recent trades in a single call for efficiency.
    /// 
    /// # Parameters
    /// - `pair`: Trading pair to fetch market data for
    /// 
    /// # Returns
    /// - `Ok((Orderbook, Vec<PublicTrade>))`: Orderbook snapshot and recent trades
    /// - `Err(EngineError)`: If the fetch fails
    async fn fetch_market_data_snapshot(
        &self,
        pair: InstrumentId,
    ) -> Result<(Orderbook, Vec<PublicTrade>), EngineError>;
}
```

### 3. Configuration Updates (`libs/trading_core/src/config.rs`)

**Purpose**: Add reconciliation configuration to system config.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    // ... existing fields ...
    
    /// Reconciliation interval in seconds
    /// How often to reconcile state with exchange REST API
    /// Default: 300 (5 minutes)
    #[serde(default = "default_reconciliation_interval")]
    pub reconciliation_interval_secs: u64,
    
    /// Enable state reconciliation
    /// Default: true
    #[serde(default = "default_reconciliation_enabled")]
    pub reconciliation_enabled: bool,
}

fn default_reconciliation_interval() -> u64 {
    300 // 5 minutes
}

fn default_reconciliation_enabled() -> bool {
    true
}
```

### 4. State Reconciler (`src/exchange_engine/src/reconciler.rs`)

**Purpose**: Core reconciliation logic.

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Interval};
use trading_core::{
    error::EngineError,
    traits::TradingClient,
    types::{
        common::Pair,
        exchange::{Balance, Order, Trade},
    },
};
use tracing::{debug, error, info, warn};

/// State reconciler for synchronizing WebSocket state with REST API
pub struct StateReconciler<TC>
where
    TC: TradingClient,
{
    trading_client: Arc<RwLock<TC>>,
    reconciliation_interval: Duration,
    enabled: bool,
}

impl<TC> StateReconciler<TC>
where
    TC: TradingClient,
{
    pub fn new(
        trading_client: Arc<RwLock<TC>>,
        reconciliation_interval_secs: u64,
        enabled: bool,
    ) -> Self {
        Self {
            trading_client,
            reconciliation_interval: Duration::from_secs(reconciliation_interval_secs),
            enabled,
        }
    }

    /// Perform initial bootstrap reconciliation
    /// Fetches all state from REST API to initialize in-memory state
    pub async fn bootstrap_reconcile(
        &self,
        configured_pairs: &[Pair],
    ) -> Result<ReconciliationResult, EngineError> {
        if !self.enabled {
            info!("State reconciliation disabled, skipping bootstrap");
            return Ok(ReconciliationResult::default());
        }

        info!("Starting bootstrap reconciliation...");

        // Fetch balances
        let balances = self.fetch_balances().await?;
        info!("Bootstrap: Fetched {} balances", balances.len());

        // Fetch open orders
        let open_orders = self.fetch_open_orders().await?;
        info!("Bootstrap: Fetched {} open orders", open_orders.len());

        // Fetch trades for each configured pair
        let mut all_trades = Vec::new();
        for pair in configured_pairs {
            match self.fetch_trades(Some(pair)).await {
                Ok(trades) => {
                    info!("Bootstrap: Fetched {} trades for {}", trades.len(), pair);
                    all_trades.extend(trades);
                }
                Err(e) => {
                    warn!("Bootstrap: Failed to fetch trades for {}: {}", pair, e);
                    // Continue with other pairs
                }
            }
        }

        Ok(ReconciliationResult {
            balances_fetched: balances.len(),
            orders_fetched: open_orders.len(),
            trades_fetched: all_trades.len(),
            discrepancies: Vec::new(),
        })
    }

    /// Start periodic reconciliation task
    /// Runs reconciliation at configured intervals
    pub fn start_periodic_reconciliation(
        &self,
        configured_pairs: Vec<Pair>,
        state_manager: Arc<dyn StateManager>,
    ) -> tokio::task::JoinHandle<()> {
        let mut interval = tokio::time::interval(self.reconciliation_interval);
        let trading_client = self.trading_client.clone();
        let enabled = self.enabled;
        let configured_pairs = configured_pairs.clone();

        tokio::spawn(async move {
            if !enabled {
                debug!("Periodic reconciliation disabled");
                return;
            }

            loop {
                interval.tick().await;

                info!("Starting periodic reconciliation...");

                match Self::reconcile_state(
                    &trading_client,
                    &configured_pairs,
                    &state_manager,
                ).await {
                    Ok(result) => {
                        if result.has_discrepancies() {
                            warn!(
                                "Reconciliation found {} discrepancies",
                                result.discrepancies.len()
                            );
                            for discrepancy in &result.discrepancies {
                                warn!("Discrepancy: {:?}", discrepancy);
                            }
                        } else {
                            debug!("Reconciliation completed with no discrepancies");
                        }
                    }
                    Err(e) => {
                        error!("Reconciliation failed: {}", e);
                        // Continue - don't stop reconciliation on error
                    }
                }
            }
        })
    }

    /// Perform reconciliation between REST and WebSocket state
    async fn reconcile_state(
        trading_client: &Arc<RwLock<TC>>,
        configured_pairs: &[Pair],
        state_manager: &Arc<dyn StateManager>,
    ) -> Result<ReconciliationResult, EngineError> {
        let mut result = ReconciliationResult::default();

        // Fetch balances from REST
        let rest_balances = trading_client.read().await.fetch_balance().await?;
        result.balances_fetched = rest_balances.len();

        // Get WebSocket balances from state manager
        let ws_balances = state_manager.get_balances().await;

        // Compare and detect discrepancies
        let balance_discrepancies = Self::compare_balances(&rest_balances, &ws_balances);
        result.discrepancies.extend(balance_discrepancies);

        // Update state with REST data (authoritative)
        state_manager.update_balances(rest_balances).await?;

        // Fetch open orders from REST
        let rest_orders = trading_client.read().await.fetch_open_orders().await?;
        result.orders_fetched = rest_orders.len();

        // Get WebSocket orders from state manager
        let ws_orders = state_manager.get_open_orders().await;

        // Compare and detect discrepancies
        let order_discrepancies = Self::compare_orders(&rest_orders, &ws_orders);
        result.discrepancies.extend(order_discrepancies);

        // Update state with REST data (authoritative)
        state_manager.update_open_orders(rest_orders).await?;

        // Fetch trades from REST for each configured pair
        for pair in configured_pairs {
            match trading_client.read().await.fetch_trades(Some(pair)).await {
                Ok(rest_trades) => {
                    result.trades_fetched += rest_trades.len();

                    // Get WebSocket trades from state manager
                    let ws_trades = state_manager.get_trades(pair).await;

                    // Compare and detect discrepancies
                    let trade_discrepancies = Self::compare_trades(&rest_trades, &ws_trades);
                    result.discrepancies.extend(trade_discrepancies);

                    // Update state with REST data (authoritative)
                    state_manager.update_trades(pair, rest_trades).await?;
                }
                Err(e) => {
                    warn!("Failed to fetch trades for {}: {}", pair, e);
                    // Continue with other pairs
                }
            }
        }

        Ok(result)
    }

    /// Fetch balances from REST API
    async fn fetch_balances(&self) -> Result<Vec<Balance>, EngineError> {
        self.trading_client.read().await.fetch_balance().await
    }

    /// Fetch open orders from REST API
    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        self.trading_client.read().await.fetch_open_orders().await
    }

    /// Fetch trades from REST API
    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        self.trading_client.read().await.fetch_trades(pair).await
    }

    /// Compare REST balances with WebSocket balances
    fn compare_balances(
        rest_balances: &[Balance],
        ws_balances: &[Balance],
    ) -> Vec<Discrepancy> {
        let mut discrepancies = Vec::new();

        // Create maps for easy lookup
        let rest_map: std::collections::HashMap<_, _> = rest_balances
            .iter()
            .map(|b| (b.symbol.clone(), b))
            .collect();
        let ws_map: std::collections::HashMap<_, _> = ws_balances
            .iter()
            .map(|b| (b.symbol.clone(), b))
            .collect();

        // Check for balances in REST but not in WS
        for (symbol, rest_balance) in &rest_map {
            if let Some(ws_balance) = ws_map.get(symbol) {
                // Compare values
                if (rest_balance.free - ws_balance.free).abs() > Decimal::new(1, 8)
                    || (rest_balance.used - ws_balance.used).abs() > Decimal::new(1, 8)
                {
                    discrepancies.push(Discrepancy::BalanceMismatch {
                        symbol: symbol.clone(),
                        rest_free: rest_balance.free,
                        rest_used: rest_balance.used,
                        ws_free: ws_balance.free,
                        ws_used: ws_balance.used,
                    });
                }
            } else {
                discrepancies.push(Discrepancy::BalanceMissingInWs {
                    symbol: symbol.clone(),
                    rest_free: rest_balance.free,
                    rest_used: rest_balance.used,
                });
            }
        }

        // Check for balances in WS but not in REST (shouldn't happen, but log it)
        for (symbol, ws_balance) in &ws_map {
            if !rest_map.contains_key(symbol) {
                discrepancies.push(Discrepancy::BalanceMissingInRest {
                    symbol: symbol.clone(),
                    ws_free: ws_balance.free,
                    ws_used: ws_balance.used,
                });
            }
        }

        discrepancies
    }

    /// Compare REST orders with WebSocket orders
    fn compare_orders(rest_orders: &[Order], ws_orders: &[Order]) -> Vec<Discrepancy> {
        let mut discrepancies = Vec::new();

        // Create maps for easy lookup
        let rest_map: std::collections::HashMap<_, _> = rest_orders
            .iter()
            .map(|o| (o.order_id, o))
            .collect();
        let ws_map: std::collections::HashMap<_, _> = ws_orders
            .iter()
            .map(|o| (o.order_id, o))
            .collect();

        // Check for orders in REST but not in WS
        for (order_id, rest_order) in &rest_map {
            if let Some(ws_order) = ws_map.get(order_id) {
                // Compare order status and filled amount
                if rest_order.status != ws_order.status
                    || (rest_order.filled_amount - ws_order.filled_amount).abs()
                        > Decimal::new(1, 8)
                {
                    discrepancies.push(Discrepancy::OrderMismatch {
                        order_id: *order_id,
                        rest_status: rest_order.status.clone(),
                        rest_filled: rest_order.filled_amount,
                        ws_status: ws_order.status.clone(),
                        ws_filled: ws_order.filled_amount,
                    });
                }
            } else {
                discrepancies.push(Discrepancy::OrderMissingInWs {
                    order_id: *order_id,
                    symbol: rest_order.symbol.clone(),
                    status: rest_order.status.clone(),
                });
            }
        }

        // Check for orders in WS but not in REST (order was filled/cancelled)
        for (order_id, ws_order) in &ws_map {
            if !rest_map.contains_key(order_id) {
                discrepancies.push(Discrepancy::OrderMissingInRest {
                    order_id: *order_id,
                    symbol: ws_order.symbol.clone(),
                    ws_status: ws_order.status.clone(),
                });
            }
        }

        discrepancies
    }

    /// Compare REST trades with WebSocket trades
    fn compare_trades(rest_trades: &[Trade], ws_trades: &[Trade]) -> Vec<Discrepancy> {
        let mut discrepancies = Vec::new();

        // Create sets of trade IDs for comparison
        let rest_trade_ids: std::collections::HashSet<_> = rest_trades
            .iter()
            .map(|t| &t.trade_id)
            .collect();
        let ws_trade_ids: std::collections::HashSet<_> = ws_trades
            .iter()
            .map(|t| &t.trade_id)
            .collect();

        // Check for trades in REST but not in WS
        for trade in rest_trades {
            if !ws_trade_ids.contains(&trade.trade_id) {
                discrepancies.push(Discrepancy::TradeMissingInWs {
                    trade_id: trade.trade_id.clone(),
                    pair: trade.pair.clone(),
                });
            }
        }

        // Note: We don't check for trades in WS but not in REST because
        // REST might only return recent trades, while WS has all trades

        discrepancies
    }
}

/// Result of a reconciliation operation
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    pub balances_fetched: usize,
    pub orders_fetched: usize,
    pub trades_fetched: usize,
    pub discrepancies: Vec<Discrepancy>,
}

impl ReconciliationResult {
    pub fn has_discrepancies(&self) -> bool {
        !self.discrepancies.is_empty()
    }
}

/// Discrepancy detected during reconciliation
#[derive(Debug, Clone)]
pub enum Discrepancy {
    BalanceMismatch {
        symbol: Symbol,
        rest_free: Decimal,
        rest_used: Decimal,
        ws_free: Decimal,
        ws_used: Decimal,
    },
    BalanceMissingInWs {
        symbol: Symbol,
        rest_free: Decimal,
        rest_used: Decimal,
    },
    BalanceMissingInRest {
        symbol: Symbol,
        ws_free: Decimal,
        ws_used: Decimal,
    },
    OrderMismatch {
        order_id: OrderId,
        rest_status: OrderStatus,
        rest_filled: Decimal,
        ws_status: OrderStatus,
        ws_filled: Decimal,
    },
    OrderMissingInWs {
        order_id: OrderId,
        symbol: Symbol,
        status: OrderStatus,
    },
    OrderMissingInRest {
        order_id: OrderId,
        symbol: Symbol,
        ws_status: OrderStatus,
    },
    TradeMissingInWs {
        trade_id: String,
        pair: Pair,
    },
}

/// Trait for state managers that can be reconciled
#[async_trait]
pub trait StateManager: Send + Sync {
    async fn get_balances(&self) -> Vec<Balance>;
    async fn get_open_orders(&self) -> Vec<Order>;
    async fn get_trades(&self, pair: &Pair) -> Vec<Trade>;
    
    async fn update_balances(&self, balances: Vec<Balance>) -> Result<(), EngineError>;
    async fn update_open_orders(&self, orders: Vec<Order>) -> Result<(), EngineError>;
    async fn update_trades(&self, pair: &Pair, trades: Vec<Trade>) -> Result<(), EngineError>;
}
```

### 5. Exchange State Manager Integration (`src/exchange_engine/src/state/manager.rs`)

**Purpose**: Integrate reconciler with state manager.

```rust
impl ExchangeStateManager {
    pub fn new(
        trading_client: Arc<RwLock<TC>>,
        config: &TradingSystemConfig,
    ) -> Self {
        let reconciler = StateReconciler::new(
            trading_client.clone(),
            config.system.reconciliation_interval_secs,
            config.system.reconciliation_enabled,
        );

        Self {
            trading_client,
            state: Arc::new(RwLock::new(ExchangeState::default())),
            reconciler,
            // ... other fields
        }
    }

    /// Bootstrap: Initialize state with REST data
    pub async fn bootstrap(&mut self, configured_pairs: &[Pair]) -> Result<(), EngineError> {
        // Perform bootstrap reconciliation
        let result = self.reconciler.bootstrap_reconcile(configured_pairs).await?;
        
        info!(
            "Bootstrap reconciliation complete: {} balances, {} orders, {} trades",
            result.balances_fetched,
            result.orders_fetched,
            result.trades_fetched
        );

        // Start periodic reconciliation
        let configured_pairs = configured_pairs.to_vec();
        let state_manager = Arc::new(self.clone()) as Arc<dyn StateManager>;
        self.reconciler.start_periodic_reconciliation(configured_pairs, state_manager);

        Ok(())
    }
}

impl StateManager for ExchangeStateManager {
    async fn get_balances(&self) -> Vec<Balance> {
        self.state.read().await.balances.clone()
    }

    async fn get_open_orders(&self) -> Vec<Order> {
        self.state.read().await.open_orders.values().cloned().collect()
    }

    async fn get_trades(&self, pair: &Pair) -> Vec<Trade> {
        self.state.read().await.trades.get(pair).cloned().unwrap_or_default()
    }

    async fn update_balances(&self, balances: Vec<Balance>) -> Result<(), EngineError> {
        self.state.write().await.balances = balances;
        Ok(())
    }

    async fn update_open_orders(&self, orders: Vec<Order>) -> Result<(), EngineError> {
        let mut state = self.state.write().await;
        state.open_orders.clear();
        for order in orders {
            state.open_orders.insert(order.order_id, order);
        }
        Ok(())
    }

    async fn update_trades(&self, pair: &Pair, trades: Vec<Trade>) -> Result<(), EngineError> {
        self.state.write().await.trades.insert(pair.clone(), trades);
        Ok(())
    }
}
```

## Integration Points

### 1. Bootstrap Integration

```rust
// In exchange engine initialization
let mut state_manager = ExchangeStateManager::new(trading_client, &config);

// Bootstrap: Fetch initial state and start reconciliation
state_manager.bootstrap(&configured_pairs).await?;
```

### 2. Configuration

```json
{
  "system": {
    "reconciliation_enabled": true,
    "reconciliation_interval_secs": 300
  }
}
```

## Error Handling

### 1. Reconciliation Failure

```rust
// If reconciliation fails, log error but continue operation
match reconciler.reconcile().await {
    Ok(result) => {
        if result.has_discrepancies() {
            warn!("Reconciliation found discrepancies: {:?}", result.discrepancies);
        }
    }
    Err(e) => {
        error!("Reconciliation failed: {}", e);
        // Continue - don't stop trading on reconciliation failure
    }
}
```

### 2. REST API Failure

```rust
// If REST API fails, use WebSocket state (may be stale)
match trading_client.fetch_balance().await {
    Ok(balances) => {
        // Update state
    }
    Err(e) => {
        warn!("Failed to fetch balances from REST: {}. Using WebSocket state.", e);
        // Continue with WebSocket state
    }
}
```

## Performance Considerations

1. **Reconciliation Interval**: Default 5 minutes, configurable
2. **Non-Blocking**: Reconciliation runs in background, doesn't block trading
3. **Efficient Comparison**: Use hash maps for O(1) lookups
4. **Selective Updates**: Only update state when discrepancies found
5. **Rate Limiting**: Respect exchange rate limits when fetching REST data

## Testing Strategy

1. **Unit Tests**
   - Reconciliation logic
   - Discrepancy detection
   - State comparison functions

2. **Integration Tests**
   - Bootstrap reconciliation
   - Periodic reconciliation
   - Discrepancy resolution
   - Error handling

3. **Mock Tests**
   - Mock REST responses
   - Mock WebSocket state
   - Test discrepancy scenarios

## Success Criteria

- [ ] `fetch_open_orders()` and `fetch_trades()` added to `TradingClient` trait
- [ ] `fetch_market_data_snapshot()` added to `MarketDataClient` trait
- [ ] All exchanges implement mandatory reconciliation methods
- [ ] State reconciler implemented in exchange engine
- [ ] Bootstrap reconciliation fetches initial state
- [ ] Periodic reconciliation runs at configured intervals
- [ ] Discrepancies detected and logged
- [ ] State corrected to match REST (authoritative source)
- [ ] Reconciliation configurable via system config
- [ ] Error handling doesn't disrupt trading

## Notes

- **Not a separate service**: Reconciler lives inside Exchange Engine
- **REST is authoritative**: REST data takes priority over WebSocket state
- **Periodic reconciliation**: Configurable interval (default 5 minutes)
- **Bootstrap reconciliation**: Fetches initial state at startup
- **Discrepancy logging**: All discrepancies logged for monitoring
- **Non-blocking**: Reconciliation runs in background
- **Graceful degradation**: Continues operation if reconciliation fails

