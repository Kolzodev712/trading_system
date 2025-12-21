//! Risk manager - orchestrates all risk checks for trading engine
//!
//! This applies to both levels-based and strategy-based trading modes.
use super::circuit_breaker::CircuitBreaker;
use super::limits::{RiskCheckResult, RiskLimits};
use super::metrics::{RiskMetrics, RiskMetricsCalculator};
use crate::state::TradingState;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::{Pair, Symbol};
use trading_core::types::exchange::{Balance, OrderRequest};

/// Risk manager - orchestrates all risk checks
pub struct RiskManager {
    limits: RiskLimits,
    #[allow(dead_code)]
    metrics_calculator: RiskMetricsCalculator,
    circuit_breaker: Arc<CircuitBreaker>,
    /// Cached metrics (updated periodically)
    cached_metrics: Arc<RwLock<Option<RiskMetrics>>>,
    /// Symbol registry for Symbol -> Pair conversion
    symbol_registry: Arc<SymbolRegistry>,
}

impl RiskManager {
    /// Create a new risk manager
    pub fn new(limits: RiskLimits, symbol_registry: Arc<SymbolRegistry>) -> Self {
        Self {
            limits,
            metrics_calculator: RiskMetricsCalculator::new(),
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            cached_metrics: Arc::new(RwLock::new(None)),
            symbol_registry,
        }
    }

    /// Get current risk limits
    pub fn get_limits(&self) -> &RiskLimits {
        &self.limits
    }

    /// Update risk limits
    pub fn update_limits(&mut self, limits: RiskLimits) {
        self.limits = limits;
    }

    /// Get circuit breaker
    pub fn get_circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        &self.circuit_breaker
    }

    /// Update cached metrics (called periodically or after trades)
    pub async fn update_metrics(
        &self,
        trading_state: &TradingState,
        current_prices: &HashMap<Pair, Decimal>,
    ) {
        // Note: metrics_calculator needs &mut self, but we can't mutate it here
        // For now, we'll create a new calculator each time. In the future, we should
        // wrap it in RwLock or Mutex if we need to maintain state.
        let mut calc = RiskMetricsCalculator::new();
        let metrics = calc.calculate(trading_state, current_prices);
        *self.cached_metrics.write().await = Some(metrics);
    }

    /// Get cached metrics
    pub async fn get_metrics(&self) -> Option<RiskMetrics> {
        self.cached_metrics.read().await.clone()
    }

    /// Check level before posting (for level-based trading)
    pub async fn check_level(
        &self,
        resolved_level: &crate::levels::ResolvedLevel,
        trading_state: &TradingState,
        balances: &[Balance],
        current_prices: Option<&HashMap<Pair, Decimal>>,
    ) -> RiskCheckResult {
        // Check circuit breaker first (highest priority)
        if !self.circuit_breaker.is_allowed().await {
            let reason = self
                .circuit_breaker
                .get_open_reason()
                .await
                .unwrap_or_else(|| SmolStr::new("Circuit breaker is open"));
            error!("Risk check blocked: {}", reason);
            return RiskCheckResult::blocked(SmolStr::new(format!(
                "Circuit breaker open: {}",
                reason
            )));
        }

        // Construct OrderRequest from ResolvedLevel for balance check
        let symbol = self
            .symbol_registry
            .get_symbol(&resolved_level.level.pair)
            .cloned()
            .unwrap_or_else(|| Symbol::from(resolved_level.level.pair.convert_to_smolstr()));

        let order_request = OrderRequest {
            symbol,
            side: resolved_level.level.config.side.clone(),
            order_type: resolved_level.resolved_order_type.clone(),
            amount: resolved_level.resolved_quantity,
            price: resolved_level.resolved_price,
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        };

        // Check daily loss limits (if metrics available)
        if let Some(prices) = current_prices {
            // Update metrics if needed
            self.update_metrics(trading_state, prices).await;

            if let Some(metrics) = self.get_metrics().await {
                // Convert Symbol to Pair using symbol registry
                let pair_opt = self
                    .symbol_registry
                    .get_pair(&order_request.symbol)
                    .cloned();

                let daily_loss_check = if let Some(pair) = pair_opt {
                    self.check_daily_loss_limits(&metrics, &pair)
                } else {
                    // Symbol not in registry - check global limit only
                    self.check_daily_loss_limits_global(&metrics)
                };

                if !daily_loss_check.allowed {
                    return daily_loss_check;
                }
            }
        }

        // Check balance
        let balance_check = self.check_balance(&order_request, balances);
        if !balance_check.allowed {
            return balance_check;
        }

        // Check total position limits
        let total_position_check = self.check_total_position_limits(trading_state);
        if !total_position_check.allowed {
            return total_position_check;
        }

        // Check total order count
        let order_count_check = self.check_order_count(trading_state);
        if !order_count_check.allowed {
            return order_count_check;
        }

        // All checks passed
        RiskCheckResult::allowed()
    }

    /// Pre-trade risk check (for any order request)
    pub async fn check_pre_trade(
        &self,
        order_request: &OrderRequest,
        trading_state: &TradingState,
        balances: &[Balance],
        current_prices: Option<&HashMap<Pair, Decimal>>,
    ) -> RiskCheckResult {
        // Check circuit breaker first (highest priority)
        if !self.circuit_breaker.is_allowed().await {
            let reason = self
                .circuit_breaker
                .get_open_reason()
                .await
                .unwrap_or_else(|| SmolStr::new("Circuit breaker is open"));
            error!("Risk check blocked: {}", reason);
            return RiskCheckResult::blocked(SmolStr::new(format!(
                "Circuit breaker open: {}",
                reason
            )));
        }

        // Check daily loss limits (if metrics available)
        if let Some(prices) = current_prices {
            // Update metrics if needed
            self.update_metrics(trading_state, prices).await;

            if let Some(metrics) = self.get_metrics().await {
                // Convert Symbol to Pair using symbol registry
                let pair_opt = self
                    .symbol_registry
                    .get_pair(&order_request.symbol)
                    .cloned();

                let daily_loss_check = if let Some(pair) = pair_opt {
                    self.check_daily_loss_limits(&metrics, &pair)
                } else {
                    // Symbol not in registry - check global limit only
                    self.check_daily_loss_limits_global(&metrics)
                };

                if !daily_loss_check.allowed {
                    return daily_loss_check;
                }
            }
        }

        // Check balance
        let balance_check = self.check_balance(order_request, balances);
        if !balance_check.allowed {
            return balance_check;
        }

        // Check total position limits
        let total_position_check = self.check_total_position_limits(trading_state);
        if !total_position_check.allowed {
            return total_position_check;
        }

        // Check total order count
        let order_count_check = self.check_order_count(trading_state);
        if !order_count_check.allowed {
            return order_count_check;
        }

        // All checks passed
        RiskCheckResult::allowed()
    }

    /// Post-trade risk check (after order execution)
    pub async fn check_post_trade(
        &self,
        trading_state: &TradingState,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> RiskCheckResult {
        // Update metrics after trade
        self.update_metrics(trading_state, current_prices).await;

        // Check circuit breaker
        if !self.circuit_breaker.is_allowed().await {
            let reason = self
                .circuit_breaker
                .get_open_reason()
                .await
                .unwrap_or_else(|| SmolStr::new("Circuit breaker is open"));
            error!("Post-trade risk check: {}", reason);
            return RiskCheckResult::blocked(SmolStr::new(format!(
                "Circuit breaker open: {}",
                reason.as_str()
            )));
        }

        if let Some(metrics) = self.get_metrics().await {
            // Check daily loss limits (global only for post-trade, pair-specific would need order info)
            let daily_loss_check = self.check_daily_loss_limits_global(&metrics);
            if !daily_loss_check.allowed {
                return daily_loss_check;
            }
        }

        RiskCheckResult::allowed()
    }

    /// Periodic risk check (runs in background)
    pub async fn check_periodic(
        &self,
        trading_state: &TradingState,
        current_prices: &HashMap<Pair, Decimal>,
    ) -> RiskCheckResult {
        // Update metrics
        self.update_metrics(trading_state, current_prices).await;

        // Check circuit breaker
        if !self.circuit_breaker.is_allowed().await {
            let reason = self
                .circuit_breaker
                .get_open_reason()
                .await
                .unwrap_or_else(|| SmolStr::new("Circuit breaker is open"));
            warn!("Periodic risk check: {}", reason);
            return RiskCheckResult::blocked(SmolStr::new(format!(
                "Circuit breaker open: {}",
                reason.as_str()
            )));
        }

        if let Some(metrics) = self.get_metrics().await {
            // Check daily loss limits (global only for periodic check)
            let daily_loss_check = self.check_daily_loss_limits_global(&metrics);
            if !daily_loss_check.allowed {
                return daily_loss_check;
            }
        }

        RiskCheckResult::allowed()
    }

    /// Check daily loss limits (with pair context)
    pub fn check_daily_loss_limits(&self, metrics: &RiskMetrics, pair: &Pair) -> RiskCheckResult {
        // Daily loss is negative daily P&L
        let daily_loss = if metrics.daily_pnl < Decimal::ZERO {
            -metrics.daily_pnl
        } else {
            Decimal::ZERO
        };

        // Check per-pair daily loss limit
        if let Some(pair_limit) = self
            .limits
            .daily_loss_limit_per_pair
            .get(&pair.convert_to_smolstr())
        {
            if daily_loss >= *pair_limit {
                error!(
                    "Daily loss limit exceeded for {}: {} >= {}",
                    pair, daily_loss, pair_limit
                );
                // Trigger circuit breaker
                let reason = format!(
                    "Daily loss limit exceeded for {}: {} >= {}",
                    pair, daily_loss, pair_limit
                );
                let cb = self.circuit_breaker.clone();
                let reason_clone = reason.clone();
                tokio::spawn(async move {
                    cb.open(reason_clone.as_str()).await;
                });

                return RiskCheckResult::blocked(SmolStr::new(reason));
            }
        }

        // Check global daily loss limit
        self.check_daily_loss_limits_global(metrics)
    }

    /// Check global daily loss limits only
    pub fn check_daily_loss_limits_global(&self, metrics: &RiskMetrics) -> RiskCheckResult {
        // Daily loss is negative daily P&L
        let daily_loss = if metrics.daily_pnl < Decimal::ZERO {
            -metrics.daily_pnl
        } else {
            Decimal::ZERO
        };

        // Check global daily loss limit
        if let Some(max_daily_loss) = self.limits.daily_loss_limit_all_pairs {
            if daily_loss >= max_daily_loss {
                error!(
                    "Daily loss limit exceeded: {} >= {}",
                    daily_loss, max_daily_loss
                );
                // Trigger circuit breaker
                let reason = format!(
                    "Daily loss limit exceeded: {} >= {}",
                    daily_loss, max_daily_loss
                );
                let cb = self.circuit_breaker.clone();
                let reason_clone = reason.clone();
                tokio::spawn(async move {
                    cb.open(reason_clone.as_str()).await;
                });

                return RiskCheckResult::blocked(SmolStr::new(reason));
            }
        }

        RiskCheckResult::allowed()
    }

    /// Check balance for order request
    pub fn check_balance(
        &self,
        order_request: &OrderRequest,
        balances: &[Balance],
    ) -> RiskCheckResult {
        // Convert Symbol to Pair using symbol registry
        let pair = match self.symbol_registry.get_pair(&order_request.symbol) {
            Some(p) => p.clone(),
            None => {
                // Symbol not in registry - skip balance check (better to allow than block incorrectly)
                return RiskCheckResult::allowed();
            }
        };

        match order_request.side {
            trading_core::types::common::Side::Buy => {
                // Need quote currency balance
                let required = order_request.price * order_request.amount;
                let available = balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == pair.quote.as_str())
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                if available < required {
                    return RiskCheckResult::blocked(SmolStr::new(format!(
                        "Insufficient balance: need {} {}, have {}",
                        required, pair.quote, available
                    )));
                }
            }
            trading_core::types::common::Side::Sell => {
                // Need base currency balance
                let required = order_request.amount;
                let available = balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == pair.base.as_str())
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                if available < required {
                    return RiskCheckResult::blocked(SmolStr::new(format!(
                        "Insufficient balance: need {} {}, have {}",
                        required, pair.base, available
                    )));
                }
            }
        }

        RiskCheckResult::allowed()
    }

    /// Check total position limits
    pub fn check_total_position_limits(&self, state: &TradingState) -> RiskCheckResult {
        if let Some(max_total_position) = self.limits.total_max_position {
            // Calculate total position across all pairs
            let total_position: Decimal = state.positions.values().map(|p| p.quantity.abs()).sum();

            // Block only if position exceeds the limit (not at exactly the limit)
            if total_position > max_total_position {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Total position limit exceeded: {} > {}",
                    total_position, max_total_position
                )));
            }
        }
        RiskCheckResult::allowed()
    }

    /// Check order count limits
    pub fn check_order_count(&self, state: &TradingState) -> RiskCheckResult {
        // Check total order count
        if let Some(max_total_orders) = self.limits.max_total_orders {
            let total_active_orders = state.active_orders.len();
            // Block at or above the limit (order limit is stricter than position limit)
            if total_active_orders >= max_total_orders {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Total order limit reached: {} >= {}",
                    total_active_orders, max_total_orders
                )));
            }
        }

        RiskCheckResult::allowed()
    }
}
