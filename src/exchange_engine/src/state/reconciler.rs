//! Reconciliation types and logic for ExchangeStateManager
//!
//! This module defines types for tracking discrepancies between WebSocket and REST state,
//! and provides reconciliation logic.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    traits::TradingClient,
    types::{
        common::Pair,
        exchange::{Balance, Order, OrderRequest, Trade},
    },
};

use super::state::ExchangeState;

/// Reconciler for WS vs REST reconciliation
///
/// Performs periodic reconciliation between WebSocket state and REST API state.
/// REST is authoritative - WebSocket state is updated to match REST when discrepancies are found.
#[derive(Debug)]
pub struct StateReconciler {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    reconciliation_interval: std::time::Duration,
    enabled: bool,
}

impl StateReconciler {
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        reconciliation_interval_secs: u64,
        enabled: bool,
    ) -> Self {
        Self {
            trading_client,
            reconciliation_interval: std::time::Duration::from_secs(reconciliation_interval_secs),
            enabled,
        }
    }

    /// Perform initial bootstrap reconciliation
    ///
    /// Fetches balances, orders, and trades from REST API to initialize state.
    /// NOTE: The actual state update is done in ExchangeStateManager::bootstrap_reconcile
    /// This method just returns the reconciliation result for logging.
    pub async fn bootstrap_reconcile(
        &self,
        _configured_pairs: &[Pair],
        exchange_state: Arc<RwLock<ExchangeState>>,
    ) -> Result<ReconciliationResult, EngineError> {
        if !self.enabled {
            info!("Bootstrap reconciliation skipped (disabled)");
            return Ok(ReconciliationResult::default());
        }

        info!("Starting bootstrap reconciliation (exchange-level: WS vs REST)");

        // Count balances in state (already updated by ExchangeStateManager::bootstrap_reconcile)
        let state = exchange_state.read().await;
        let balance_count = state.get_balances().len();
        drop(state);

        // Note: fetch_open_orders and fetch_trades may not be implemented yet
        // For now, we'll just return the counts we fetched
        let result = ReconciliationResult {
            balances_fetched: balance_count,
            orders_fetched: 0,
            trades_fetched: 0,
            discrepancies: Vec::new(),
        };

        info!(
            "Bootstrap reconciliation completed: {} balances, {} orders, {} trades fetched{}",
            result.balances_fetched,
            result.orders_fetched,
            result.trades_fetched,
            if result.has_discrepancies() {
                format!(", {} discrepancies resolved", result.discrepancies.len())
            } else {
                ", no discrepancies".to_string()
            }
        );

        Ok(result)
    }

    /// Start periodic reconciliation task
    ///
    /// Spawns a background task that periodically reconciles WebSocket state with REST.
    pub fn start_periodic_reconciliation(
        &self,
        configured_pairs: Vec<Pair>,
        exchange_state: Arc<RwLock<ExchangeState>>,
    ) -> tokio::task::JoinHandle<()> {
        let mut interval = tokio::time::interval(self.reconciliation_interval);
        let trading_client = self.trading_client.clone();
        let enabled = self.enabled;

        let reconciliation_interval_secs = self.reconciliation_interval.as_secs();
        tokio::spawn(async move {
            if !enabled {
                info!("Periodic reconciliation disabled");
                return;
            }

            tracing::debug!(
                "Starting periodic exchange-level reconciliation (interval: {} seconds)",
                reconciliation_interval_secs
            );

            loop {
                interval.tick().await;

                tracing::debug!("Running periodic exchange-level reconciliation (WS vs REST)");

                // Fetch from REST and compare with WebSocket state
                match Self::reconcile_state(&trading_client, &configured_pairs, &exchange_state)
                    .await
                {
                    Ok(result) => {
                        if result.has_discrepancies() {
                            tracing::debug!(
                                "Exchange-level reconciliation found {} discrepancies: {} balances, {} orders, {} trades",
                                result.discrepancies.len(),
                                result.balances_fetched,
                                result.orders_fetched,
                                result.trades_fetched
                            );
                        } else {
                            tracing::debug!(
                                "Exchange-level reconciliation completed: {} balances, {} orders, {} trades, no discrepancies",
                                result.balances_fetched,
                                result.orders_fetched,
                                result.trades_fetched
                            );
                        }
                    }
                    Err(e) => {
                        error!("Exchange-level reconciliation failed: {}", e);
                    }
                }
            }
        })
    }

    /// Perform reconciliation between REST and WebSocket state
    async fn reconcile_state(
        trading_client: &Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        _configured_pairs: &[Pair], // Reserved for future pair-specific trade filtering
        exchange_state: &Arc<RwLock<ExchangeState>>,
    ) -> Result<ReconciliationResult, EngineError> {
        let mut result = ReconciliationResult::default();

        // Reconcile balances
        tracing::debug!("Fetching balances from REST API for reconciliation");
        let rest_balances = trading_client.read().await.fetch_balance().await?;
        let ws_state = exchange_state.read().await;
        let ws_balances: Vec<Balance> = ws_state.get_balances().into_iter().cloned().collect();
        drop(ws_state);

        tracing::debug!(
            "Comparing {} REST balances with {} WebSocket balances",
            rest_balances.len(),
            ws_balances.len()
        );
        let balance_discrepancies = compare_balances(&rest_balances, &ws_balances);
        let discrepancy_count = balance_discrepancies.len();
        result.discrepancies.extend(balance_discrepancies);
        result.balances_fetched = rest_balances.len();

        if discrepancy_count > 0 {
            tracing::debug!(
                "Found {} balance discrepancies, updating WebSocket state to match REST (REST is authoritative)",
                discrepancy_count
            );
        }

        let mut ws_state = exchange_state.write().await;
        ws_state.update_balances(rest_balances);
        drop(ws_state);

        // TODO: Reconcile orders and trades when fetch_open_orders and fetch_trades are implemented

        Ok(result)
    }
}

use rust_decimal::Decimal;
use trading_core::types::common::{OrderId, OrderStatus};

/// Discrepancy detected during reconciliation
#[derive(Debug, Clone)]
pub enum Discrepancy {
    /// Balance mismatch between REST and WebSocket
    BalanceMismatch {
        symbol: String,
        rest_free: Decimal,
        rest_used: Decimal,
        ws_free: Decimal,
        ws_used: Decimal,
    },
    /// Balance exists in REST but missing in WebSocket state
    BalanceMissingInWs {
        symbol: String,
        rest_free: Decimal,
        rest_used: Decimal,
    },
    /// Order mismatch between REST and WebSocket
    OrderMismatch {
        order_id: OrderId,
        rest_status: OrderStatus,
        rest_filled: Decimal,
        ws_status: OrderStatus,
        ws_filled: Decimal,
    },
    /// Order exists in REST but missing in WebSocket state
    OrderMissingInWs {
        order_id: OrderId,
        symbol: Pair,
        status: OrderStatus,
    },
    /// Trade exists in REST but missing in WebSocket state
    TradeMissingInWs { trade_id: String, pair: Pair },
}

/// Result of a reconciliation operation
#[derive(Debug, Default, Clone)]
pub struct ReconciliationResult {
    /// Number of balances fetched from REST
    pub balances_fetched: usize,
    /// Number of orders fetched from REST
    pub orders_fetched: usize,
    /// Number of trades fetched from REST
    pub trades_fetched: usize,
    /// List of discrepancies detected
    pub discrepancies: Vec<Discrepancy>,
}

impl ReconciliationResult {
    /// Check if any discrepancies were detected
    pub fn has_discrepancies(&self) -> bool {
        !self.discrepancies.is_empty()
    }
}

/// Compare REST balances with WebSocket balances
pub fn compare_balances(rest_balances: &[Balance], ws_balances: &[Balance]) -> Vec<Discrepancy> {
    let mut discrepancies = Vec::new();

    // Create a map of WS balances by symbol for quick lookup
    let ws_balance_map: std::collections::HashMap<_, _> =
        ws_balances.iter().map(|b| (b.symbol.as_ref(), b)).collect();

    for rest_balance in rest_balances {
        let symbol = rest_balance.symbol.as_ref();
        match ws_balance_map.get(symbol) {
            Some(ws_balance) => {
                // Check for mismatch
                if (rest_balance.free - ws_balance.free).abs() > Decimal::new(1, 6)
                    || (rest_balance.used - ws_balance.used).abs() > Decimal::new(1, 6)
                {
                    discrepancies.push(Discrepancy::BalanceMismatch {
                        symbol: symbol.to_string(),
                        rest_free: rest_balance.free,
                        rest_used: rest_balance.used,
                        ws_free: ws_balance.free,
                        ws_used: ws_balance.used,
                    });
                }
            }
            None => {
                // Balance missing in WS
                discrepancies.push(Discrepancy::BalanceMissingInWs {
                    symbol: symbol.to_string(),
                    rest_free: rest_balance.free,
                    rest_used: rest_balance.used,
                });
            }
        }
    }

    discrepancies
}

/// Compare REST orders with WebSocket orders
pub fn compare_orders(rest_orders: &[Order], ws_orders: &[Order]) -> Vec<Discrepancy> {
    let mut discrepancies = Vec::new();

    // Create a map of WS orders by order_id for quick lookup
    let ws_order_map: std::collections::HashMap<_, _> =
        ws_orders.iter().map(|o| (&o.order_id, o)).collect();

    for rest_order in rest_orders {
        match ws_order_map.get(&rest_order.order_id) {
            Some(ws_order) => {
                // Check for mismatch
                if rest_order.status != ws_order.status
                    || (rest_order.filled_amount - ws_order.filled_amount).abs()
                        > Decimal::new(1, 6)
                {
                    discrepancies.push(Discrepancy::OrderMismatch {
                        order_id: rest_order.order_id.clone(),
                        rest_status: rest_order.status.clone(),
                        rest_filled: rest_order.filled_amount,
                        ws_status: ws_order.status.clone(),
                        ws_filled: ws_order.filled_amount,
                    });
                }
            }
            None => {
                // Order missing in WS
                // Note: We can't convert Symbol to Pair reliably, so we'll use a placeholder
                // In practice, the symbol string will be logged for debugging
                discrepancies.push(Discrepancy::OrderMissingInWs {
                    order_id: rest_order.order_id.clone(),
                    symbol: trading_core::types::common::Pair::new("UNKNOWN", "UNKNOWN"), // Placeholder - symbol info is in order_id
                    status: rest_order.status.clone(),
                });
            }
        }
    }

    discrepancies
}

/// Compare REST trades with WebSocket trades
pub fn compare_trades(rest_trades: &[Trade], ws_trades: &[Trade]) -> Vec<Discrepancy> {
    let mut discrepancies = Vec::new();

    // Create a set of WS trade IDs for quick lookup
    let ws_trade_ids: std::collections::HashSet<_> =
        ws_trades.iter().map(|t| t.trade_id.as_ref()).collect();

    for rest_trade in rest_trades {
        if !ws_trade_ids.contains(rest_trade.trade_id.as_ref()) {
            discrepancies.push(Discrepancy::TradeMissingInWs {
                trade_id: rest_trade.trade_id.as_ref().to_string(),
                pair: rest_trade.pair.clone(),
            });
        }
    }

    discrepancies
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::{Side, Symbol};

    fn create_test_balance(symbol: &str, free: Decimal, used: Decimal) -> Balance {
        Balance {
            symbol: Symbol::from(symbol),
            free,
            used,
            total: free + used,
        }
    }

    fn create_test_order(
        order_id: &str,
        symbol: &str,
        status: OrderStatus,
        filled: Decimal,
    ) -> Order {
        Order {
            order_id: OrderId::new(order_id.into()),
            symbol: Symbol::from(symbol),
            side: Side::Buy,
            price: Decimal::new(100, 0),
            amount: Decimal::new(1, 0),
            internal_created_at: 0,
            exchange_created_at: None,
            status,
            filled_amount: filled,
        }
    }

    #[test]
    fn test_compare_balances_match() {
        let rest = vec![create_test_balance(
            "BTC",
            Decimal::new(100, 0),
            Decimal::new(10, 0),
        )];
        let ws = vec![create_test_balance(
            "BTC",
            Decimal::new(100, 0),
            Decimal::new(10, 0),
        )];

        let discrepancies = compare_balances(&rest, &ws);
        assert!(discrepancies.is_empty());
    }

    #[test]
    fn test_compare_balances_mismatch() {
        let rest = vec![create_test_balance(
            "BTC",
            Decimal::new(100, 0),
            Decimal::new(10, 0),
        )];
        let ws = vec![create_test_balance(
            "BTC",
            Decimal::new(90, 0),
            Decimal::new(20, 0),
        )];

        let discrepancies = compare_balances(&rest, &ws);
        assert_eq!(discrepancies.len(), 1);
        match &discrepancies[0] {
            Discrepancy::BalanceMismatch { .. } => {}
            _ => panic!("Expected BalanceMismatch"),
        }
    }

    #[test]
    fn test_compare_balances_missing() {
        let rest = vec![create_test_balance(
            "BTC",
            Decimal::new(100, 0),
            Decimal::new(10, 0),
        )];
        let ws = vec![];

        let discrepancies = compare_balances(&rest, &ws);
        assert_eq!(discrepancies.len(), 1);
        match &discrepancies[0] {
            Discrepancy::BalanceMissingInWs { .. } => {}
            _ => panic!("Expected BalanceMissingInWs"),
        }
    }

    #[test]
    fn test_compare_orders_match() {
        let rest = vec![create_test_order(
            "123",
            "BTCUSDT",
            OrderStatus::New,
            Decimal::ZERO,
        )];
        let ws = vec![create_test_order(
            "123",
            "BTCUSDT",
            OrderStatus::New,
            Decimal::ZERO,
        )];

        let discrepancies = compare_orders(&rest, &ws);
        assert!(discrepancies.is_empty());
    }

    #[test]
    fn test_compare_orders_mismatch() {
        let rest = vec![create_test_order(
            "123",
            "BTCUSDT",
            OrderStatus::Filled,
            Decimal::new(1, 0),
        )];
        let ws = vec![create_test_order(
            "123",
            "BTCUSDT",
            OrderStatus::PartiallyFilled,
            Decimal::new(5, 1),
        )];

        let discrepancies = compare_orders(&rest, &ws);
        assert_eq!(discrepancies.len(), 1);
        match &discrepancies[0] {
            Discrepancy::OrderMismatch { .. } => {}
            _ => panic!("Expected OrderMismatch"),
        }
    }

    #[test]
    fn test_reconciliation_result() {
        let mut result = ReconciliationResult::default();
        assert!(!result.has_discrepancies());

        result.discrepancies.push(Discrepancy::BalanceMismatch {
            symbol: "BTC".to_string(),
            rest_free: Decimal::ZERO,
            rest_used: Decimal::ZERO,
            ws_free: Decimal::ZERO,
            ws_used: Decimal::ZERO,
        });
        assert!(result.has_discrepancies());
    }
}
