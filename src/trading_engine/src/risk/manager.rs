//! Risk manager - orchestrates all risk checks for trading engine
//!
//! This applies to both levels-based and strategy-based trading modes.
use super::circuit_breaker::CircuitBreaker;
use super::limits::{RiskCheckResult, RiskLimits};
use super::metrics::{RiskMetrics, RiskMetricsCalculator};
use crate::cex::state::ledger::ExecutionLedger;
use crate::cex::state::TradingState;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::types::common::{Instrument, InstrumentType, Pair, Side};
use trading_core::types::exchange::{Balance, OrderRequest};

/// Risk manager - orchestrates all risk checks
pub struct RiskManager {
    limits: RiskLimits,
    circuit_breaker: Arc<CircuitBreaker>,
    /// Cached metrics (updated periodically)
    cached_metrics: Arc<RwLock<Option<RiskMetrics>>>,
    execution_ledger: Option<Arc<ExecutionLedger>>,
}

impl RiskManager {
    /// Create a new risk manager
    pub fn new(limits: RiskLimits, _symbol_registry: Arc<SymbolRegistry>) -> Self {
        Self {
            limits,
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            cached_metrics: Arc::new(RwLock::new(None)),
            execution_ledger: None,
        }
    }

    pub fn with_execution_ledger(mut self, ledger: Arc<ExecutionLedger>) -> Self {
        self.execution_ledger = Some(ledger);
        self
    }

    fn daily_loss_configured(&self) -> bool {
        self.limits.daily_loss_limit_all_pairs.is_some()
            || !self.limits.daily_loss_limit_per_pair.is_empty()
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
        let ledger_snap = match &self.execution_ledger {
            Some(ledger) => match ledger
                .snapshot(current_prices, chrono::Utc::now().timestamp_millis() as u64)
            {
                Ok(snap) => Some(snap),
                Err(err) => {
                    error!("execution ledger snapshot failed: {err}");
                    None
                }
            },
            None => None,
        };
        let metrics =
            calc.calculate_with_ledger(trading_state, current_prices, ledger_snap.as_ref());
        *self.cached_metrics.write().await = Some(metrics);
    }

    /// Get cached metrics
    pub async fn get_metrics(&self) -> Option<RiskMetrics> {
        self.cached_metrics.read().await.clone()
    }

    /// Check level before posting (for level-based trading)
    pub async fn check_level(
        &self,
        resolved_level: &crate::cex::levels::ResolvedLevel,
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

        // Construct OrderRequest from ResolvedLevel using the level's instrument
        let order_request = OrderRequest {
            instrument: Instrument::new(
                resolved_level.level.identity.instrument.clone(),
                resolved_level.level.pair.clone(),
            ),
            side: resolved_level.level.config.side.clone(),
            order_type: resolved_level.resolved_order_type.clone(),
            amount: resolved_level.resolved_quantity,
            price: resolved_level.resolved_price,
            timestamp: None,
            exchange_params: std::collections::HashMap::new(),
        };

        if resolved_level.resolved_price <= Decimal::ZERO
            || resolved_level.resolved_quantity <= Decimal::ZERO
        {
            return RiskCheckResult::blocked(SmolStr::new(
                "price and quantity must be strictly positive",
            ));
        }

        let risk = &resolved_level.level.config.risk;
        if risk.daily_loss_limit.is_some() {
            tracing::debug!(
                "ignoring per-level daily_loss_limit on {}; PnL attribution is not implemented",
                resolved_level.level.identity.instance_id
            );
        }
        if let Some(max_qty) = risk.max_order_qty.filter(|v| *v > Decimal::ZERO) {
            if order_request.amount > max_qty {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Level order qty {} exceeds per-level max_order_qty {}",
                    order_request.amount, max_qty
                )));
            }
        }
        if let Some(max_notional) = risk.max_notional.filter(|v| *v > Decimal::ZERO) {
            let notional = order_request.price * order_request.amount;
            if notional > max_notional {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Level order notional {} exceeds per-level max_notional {}",
                    notional, max_notional
                )));
            }
        }
        if let Some(max_orders) = risk.max_orders.filter(|n| *n > 0) {
            if resolved_level.level.active_orders.len() >= max_orders {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Level working order count {} exceeds per-level max_orders {}",
                    resolved_level.level.active_orders.len(),
                    max_orders
                )));
            }
        }
        if let Some(max_position) = risk.max_position.filter(|v| *v > Decimal::ZERO) {
            let signed = match order_request.side {
                Side::Buy => order_request.amount,
                Side::Sell => -order_request.amount,
            };
            let owned_working: Decimal = resolved_level
                .level
                .active_orders
                .iter()
                .filter_map(|oid| trading_state.active_orders.get(oid))
                .map(|order| order.amount - order.filled_amount)
                .sum();
            let prospective = owned_working + order_request.amount;
            if prospective.abs() > max_position || signed.abs() > max_position {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Level exposure {} exceeds per-level max_position {}",
                    prospective, max_position
                )));
            }
        }

        let position_qty = trading_state.effective_position_qty(&order_request.instrument.pair);
        self.check_pre_trade(
            &order_request,
            trading_state,
            balances,
            current_prices,
            position_qty,
            None,
        )
        .await
    }

    /// Pre-trade risk check (for any order request).
    ///
    /// `position_qty` is the filled position. Futures orders that reduce it are not
    /// blocked for lack of the base asset. `leverage` scales the quote margin
    /// required to increase a futures position.
    pub async fn check_pre_trade(
        &self,
        order_request: &OrderRequest,
        trading_state: &TradingState,
        balances: &[Balance],
        current_prices: Option<&HashMap<Pair, Decimal>>,
        position_qty: Decimal,
        leverage: Option<u32>,
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

        let daily_loss_check = self
            .enforce_daily_loss_limits(
                trading_state,
                Some(&order_request.instrument.pair),
                current_prices,
            )
            .await;
        if !daily_loss_check.allowed {
            return daily_loss_check;
        }

        // Check balance. Futures reductions are allowed through; increases use quote margin.
        let balance_check = check_order_balance(order_request, balances, position_qty, leverage);
        if !balance_check.allowed {
            return balance_check;
        }

        let increasing = order_increases_exposure(order_request.side.clone(), position_qty);
        if increasing {
            let total_position_check = self.check_total_position_limits(trading_state);
            if !total_position_check.allowed {
                return total_position_check;
            }
            let order_count_check = self.check_order_count(trading_state);
            if !order_count_check.allowed {
                return order_count_check;
            }
        }

        if let Some(max_qty) = self.limits.max_order_qty.filter(|v| *v > Decimal::ZERO) {
            if order_request.amount > max_qty {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Order qty {} exceeds max_order_qty {}",
                    order_request.amount, max_qty
                )));
            }
        }
        if let Some(max_notional) = self.limits.max_notional.filter(|v| *v > Decimal::ZERO) {
            let notional = order_request.price * order_request.amount;
            if notional > max_notional {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Order notional {} exceeds max_notional {}",
                    notional, max_notional
                )));
            }
        }
        if let Some(max_total_position) = self
            .limits
            .total_max_position
            .filter(|v| *v > Decimal::ZERO)
        {
            let signed = match order_request.side {
                Side::Buy => order_request.amount,
                Side::Sell => -order_request.amount,
            };
            let pair = &order_request.instrument.pair;
            let prospective = trading_state.effective_position_qty(pair) + signed;
            if prospective.abs() > max_total_position {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "Prospective effective exposure {} exceeds max_position {}",
                    prospective, max_total_position
                )));
            }
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

        let daily_loss_check = self
            .enforce_daily_loss_limits(trading_state, None, Some(current_prices))
            .await;
        if !daily_loss_check.allowed {
            return daily_loss_check;
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

        let daily_loss_check = self
            .enforce_daily_loss_limits(trading_state, None, Some(current_prices))
            .await;
        if !daily_loss_check.allowed {
            return daily_loss_check;
        }

        RiskCheckResult::allowed()
    }

    async fn enforce_daily_loss_limits(
        &self,
        trading_state: &TradingState,
        pair: Option<&Pair>,
        current_prices: Option<&HashMap<Pair, Decimal>>,
    ) -> RiskCheckResult {
        if !self.daily_loss_configured() {
            return RiskCheckResult::allowed();
        }
        let Some(ledger) = &self.execution_ledger else {
            return RiskCheckResult::blocked(SmolStr::new(
                "daily-loss limit requires execution ledger",
            ));
        };
        let empty = HashMap::new();
        let prices = current_prices.unwrap_or(&empty);
        let snap = match ledger.snapshot(prices, chrono::Utc::now().timestamp_millis() as u64) {
            Ok(snap) => snap,
            Err(err) => {
                return RiskCheckResult::blocked(SmolStr::new(format!(
                    "execution ledger snapshot failed: {err}"
                )));
            }
        };
        let mut calc = RiskMetricsCalculator::new();
        let metrics = calc.calculate_with_ledger(trading_state, prices, Some(&snap));
        *self.cached_metrics.write().await = Some(metrics.clone());
        match pair {
            Some(pair) => self.check_daily_loss_limits(&metrics, pair),
            None => self.check_daily_loss_limits_global(&metrics),
        }
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
        // Use pair from instrument
        let pair = order_request.instrument.pair.clone();

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

    /// Check total position limits per pair. Never sums BTC and ETH base quantities.
    pub fn check_total_position_limits(&self, state: &TradingState) -> RiskCheckResult {
        if let Some(max_total_position) = self
            .limits
            .total_max_position
            .filter(|v| *v > Decimal::ZERO)
        {
            let mut pairs: Vec<Pair> = state.positions.keys().cloned().collect();
            for order in state.active_orders.values() {
                if !pairs.contains(&order.instrument.pair) {
                    pairs.push(order.instrument.pair.clone());
                }
            }
            for submission in state.pending_submissions().values() {
                if !pairs.contains(&submission.symbol) {
                    pairs.push(submission.symbol.clone());
                }
            }
            for pair in pairs {
                let qty = state.effective_position_qty(&pair).abs();
                if qty > max_total_position {
                    return RiskCheckResult::blocked(SmolStr::new(format!(
                        "Per-pair position limit exceeded for {}: {} > {}",
                        pair, qty, max_total_position
                    )));
                }
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

/// Buy increases exposure when the position is flat or long. Sell increases it when flat or short.
pub fn order_increases_exposure(side: Side, position_qty: Decimal) -> bool {
    match side {
        Side::Buy => position_qty >= Decimal::ZERO,
        Side::Sell => position_qty <= Decimal::ZERO,
    }
}

/// Spot checks inventory. Futures increases require quote margin. Futures reductions do not
/// require the base asset, so a long can be sold and a short can be bought back.
pub fn check_order_balance(
    order_request: &OrderRequest,
    balances: &[Balance],
    position_qty: Decimal,
    leverage: Option<u32>,
) -> RiskCheckResult {
    let pair = order_request.instrument.pair.clone();
    if matches!(
        order_request.instrument.instrument_type(),
        InstrumentType::Futures | InstrumentType::Derivitives
    ) {
        if !order_increases_exposure(order_request.side.clone(), position_qty) {
            return RiskCheckResult::allowed();
        }
        let mut required = order_request.price * order_request.amount;
        if let Some(lev) = leverage {
            if lev > 0 {
                required /= Decimal::from(lev);
            }
        }
        let available = balances
            .iter()
            .find(|b| b.symbol.as_ref() == pair.quote.as_str())
            .map(|b| b.free)
            .unwrap_or(Decimal::ZERO);
        if available < required {
            return RiskCheckResult::blocked(SmolStr::new(format!(
                "Insufficient futures margin: need {} {}, have {}",
                required, pair.quote, available
            )));
        }
        return RiskCheckResult::allowed();
    }

    match order_request.side {
        Side::Buy => {
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
        Side::Sell => {
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

#[cfg(test)]
mod balance_tests {
    use super::*;
    use rust_decimal_macros::dec;
    use trading_core::types::common::{Instrument, OrderType};

    fn req(instrument_type: InstrumentType, side: Side) -> OrderRequest {
        OrderRequest {
            instrument: Instrument::new(instrument_type, Pair::new("BTC", "USDT")),
            side,
            order_type: OrderType::Limit,
            price: dec!(100),
            amount: dec!(1),
            timestamp: None,
            exchange_params: Default::default(),
        }
    }

    fn usdt(free: Decimal) -> Balance {
        Balance {
            symbol: "USDT".into(),
            free,
            used: Decimal::ZERO,
            total: free,
        }
    }

    fn btc(free: Decimal) -> Balance {
        Balance {
            symbol: "BTC".into(),
            free,
            used: Decimal::ZERO,
            total: free,
        }
    }

    #[test]
    fn futures_sell_to_close_does_not_require_base_inventory() {
        let result = check_order_balance(
            &req(InstrumentType::Futures, Side::Sell),
            &[],
            dec!(1),
            Some(5),
        );
        assert!(result.allowed);
    }

    #[test]
    fn futures_short_open_uses_quote_margin() {
        let blocked = check_order_balance(
            &req(InstrumentType::Futures, Side::Sell),
            &[usdt(dec!(10))],
            Decimal::ZERO,
            Some(5),
        );
        assert!(!blocked.allowed);
        let allowed = check_order_balance(
            &req(InstrumentType::Futures, Side::Sell),
            &[usdt(dec!(20))],
            Decimal::ZERO,
            Some(5),
        );
        assert!(allowed.allowed);
    }

    #[tokio::test]
    async fn prospective_effective_exposure_blocks_increase() {
        let limits = RiskLimits {
            total_max_position: Some(dec!(1)),
            ..RiskLimits::default()
        };
        let risk = RiskManager::new(
            limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                vec![Pair::new("BTC", "USDT")],
            )),
        );
        let mut state = TradingState::new();
        state.set_position(Pair::new("BTC", "USDT"), dec!(1), dec!(100));
        let blocked = risk
            .check_pre_trade(
                &req(InstrumentType::Spot, Side::Buy),
                &state,
                &[usdt(dec!(1000))],
                None,
                dec!(1),
                None,
            )
            .await;
        assert!(!blocked.allowed);
        let reduce = risk
            .check_pre_trade(
                &req(InstrumentType::Spot, Side::Sell),
                &state,
                &[usdt(dec!(1000)), btc(dec!(1))],
                None,
                dec!(1),
                None,
            )
            .await;
        assert!(reduce.allowed);
    }

    #[tokio::test]
    async fn daily_loss_without_ledger_is_blocked() {
        let limits = RiskLimits {
            daily_loss_limit_all_pairs: Some(dec!(1)),
            max_order_qty: Some(dec!(10)),
            ..RiskLimits::default()
        };
        let risk = RiskManager::new(
            limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                vec![Pair::new("BTC", "USDT")],
            )),
        );
        let blocked = risk
            .check_pre_trade(
                &req(InstrumentType::Spot, Side::Buy),
                &TradingState::new(),
                &[usdt(dec!(1000))],
                None,
                Decimal::ZERO,
                None,
            )
            .await;
        assert!(!blocked.allowed);
        assert!(blocked
            .reason
            .unwrap()
            .contains("daily-loss limit requires execution ledger"));
    }

    #[tokio::test]
    async fn daily_loss_uses_ledger_realized_pnl() {
        let dir = std::env::temp_dir().join(format!(
            "inc9-daily-loss-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = crate::cex::state::ExecutionLedger::open(dir.join("ledger.jsonl")).unwrap();
        ledger
            .record_fill(crate::cex::state::FillRecord {
                execution_id: "b".into(),
                order_id: "o1".into(),
                client_order_id: None,
                strategy_id: None,
                symbol_base: "BTC".into(),
                symbol_quote: "USDT".into(),
                side: "Buy".into(),
                quantity: "1".into(),
                price: "100".into(),
                fee: "0".into(),
                fee_currency: "USDT".into(),
                exchange_ts_ms: chrono::Utc::now().timestamp_millis() as u64,
                receive_ts_ms: chrono::Utc::now().timestamp_millis() as u64,
            })
            .unwrap();
        ledger
            .record_fill(crate::cex::state::FillRecord {
                execution_id: "s".into(),
                order_id: "o2".into(),
                client_order_id: None,
                strategy_id: None,
                symbol_base: "BTC".into(),
                symbol_quote: "USDT".into(),
                side: "Sell".into(),
                quantity: "1".into(),
                price: "90".into(),
                fee: "0".into(),
                fee_currency: "USDT".into(),
                exchange_ts_ms: chrono::Utc::now().timestamp_millis() as u64,
                receive_ts_ms: chrono::Utc::now().timestamp_millis() as u64,
            })
            .unwrap();
        let limits = RiskLimits {
            daily_loss_limit_all_pairs: Some(dec!(5)),
            max_order_qty: Some(dec!(10)),
            ..RiskLimits::default()
        };
        let risk = RiskManager::new(
            limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                vec![Pair::new("BTC", "USDT")],
            )),
        )
        .with_execution_ledger(Arc::new(ledger));
        let blocked = risk
            .check_pre_trade(
                &req(InstrumentType::Spot, Side::Buy),
                &TradingState::new(),
                &[usdt(dec!(1000))],
                None,
                Decimal::ZERO,
                None,
            )
            .await;
        assert!(!blocked.allowed);
        assert!(blocked
            .reason
            .unwrap()
            .contains("Daily loss limit exceeded"));
    }

    #[test]
    fn btc_and_eth_are_not_summed_as_one_qty_cap() {
        let limits = RiskLimits {
            total_max_position: Some(dec!(0.1)),
            ..RiskLimits::default()
        };
        let risk = RiskManager::new(
            limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                vec![Pair::new("BTC", "USDT"), Pair::new("ETH", "USDT")],
            )),
        );
        let mut state = TradingState::new();
        state.set_position(Pair::new("BTC", "USDT"), dec!(0.08), dec!(50000));
        state.set_position(Pair::new("ETH", "USDT"), dec!(0.08), dec!(3000));
        assert!(risk.check_total_position_limits(&state).allowed);
    }

    #[tokio::test]
    async fn two_orders_pass_alone_but_combined_exceed_per_pair_cap() {
        let limits = RiskLimits {
            total_max_position: Some(dec!(0.0015)),
            max_order_qty: Some(dec!(0.001)),
            ..RiskLimits::default()
        };
        let risk = RiskManager::new(
            limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                vec![Pair::new("BTC", "USDT")],
            )),
        );
        let mut order = req(InstrumentType::Spot, Side::Buy);
        order.amount = dec!(0.001);
        order.price = dec!(50000);
        let mut state = TradingState::new();
        let first = risk
            .check_pre_trade(
                &order,
                &state,
                &[usdt(dec!(1000))],
                None,
                Decimal::ZERO,
                None,
            )
            .await;
        assert!(first.allowed);
        state.begin_submission(crate::cex::state::PendingSubmission {
            client_order_id: smol_str::SmolStr::new("a"),
            symbol: Pair::new("BTC", "USDT"),
            side: Side::Buy,
            amount: dec!(0.001),
            price: dec!(50000),
            strategy_id: None,
            level_id: Some("buy_level_1_BTCUSDT".into()),
        });
        let second = risk
            .check_pre_trade(
                &order,
                &state,
                &[usdt(dec!(1000))],
                None,
                state.effective_position_qty(&Pair::new("BTC", "USDT")),
                None,
            )
            .await;
        assert!(!second.allowed);
    }

    #[tokio::test]
    async fn zero_configured_caps_are_not_enforced() {
        let limits = RiskLimits {
            total_max_position: Some(Decimal::ZERO),
            max_notional: Some(Decimal::ZERO),
            max_order_qty: Some(Decimal::ZERO),
            ..RiskLimits::default()
        };
        let risk = RiskManager::new(
            limits,
            Arc::new(trading_core::registry::symbol::SymbolRegistry::from_pairs(
                vec![Pair::new("BTC", "USDT")],
            )),
        );
        let allowed = risk
            .check_pre_trade(
                &req(InstrumentType::Spot, Side::Buy),
                &TradingState::new(),
                &[usdt(dec!(1000))],
                None,
                Decimal::ZERO,
                None,
            )
            .await;
        assert!(allowed.allowed);
    }
}
