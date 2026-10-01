//! Trading state management
//!
//! This module provides TradingState (data structure) and TradingStateManager
//! (state manager that subscribes to ExchangeStateManager and manages trading state).

use async_trait::async_trait;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use trading_core::{
    error::EngineError,
    types::{
        common::{InstrumentType, OrderId, OrderStatus, Pair, Side},
        exchange::{Order, Trade},
    },
};

pub mod ledger;
pub mod manager;
mod operator_flatten;
mod reconciler;
pub mod submission;
mod subscriber;

pub use ledger::{
    economic_source, ExecutionLedger, FillRecord, FundingRecord, LedgerRecord, LedgerSnapshot,
};
pub use operator_flatten::{
    cancel_working_unverified, flatten_cancel_working_failed, flatten_position_unverified,
    is_operator_flatten, merge_rest_working_orders, settle_operator_flatten_exits,
    settle_operator_flatten_on_order, settle_operator_flatten_when_local_flat,
    submit_operator_reduce_only_exits, verify_rest_exposure_flat,
    verify_rest_working_orders_cleared, OperatorFlattenReport, OPERATOR_FLATTEN_INTENT,
};
pub use submission::{
    recover_unknown_submissions, RecoveryReport, SubmissionJournal, SubmissionRecord,
    SubmissionState,
};

/// Represents a position in a trading pair
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: Pair,
    pub quantity: Decimal,
    pub avg_price: Decimal,
}

impl Position {
    /// Average entry used for P&L. `None` means the cost basis is unknown
    /// (wallet bootstrap without a venue entry), not a real zero price.
    pub fn cost_basis(&self) -> Option<Decimal> {
        if self.avg_price.abs() <= Decimal::new(1, 6) {
            None
        } else {
            Some(self.avg_price)
        }
    }
}

/// A submission whose exchange outcome is not yet known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSubmission {
    pub client_order_id: SmolStr,
    pub symbol: Pair,
    pub side: Side,
    pub amount: Decimal,
    pub price: Decimal,
    pub strategy_id: Option<String>,
    /// Expanded level instance id when this submission belongs to a level.
    pub level_id: Option<String>,
}

/// Manages active orders and positions
#[derive(Debug, Clone, Default)]
pub struct TradingState {
    pub positions: HashMap<Pair, Position>,
    pub active_orders: HashMap<OrderId, Order>,
    /// Orders that are pending (not yet filled or cancelled)
    pub pending_orders: HashMap<OrderId, u64>, // order_id -> timestamp when order was placed
    /// Last applied fill qty per order (idempotent fill accounting).
    fill_applied: HashMap<OrderId, Decimal>,
    /// Spot private-trade ids already applied to position (unique ledger key).
    applied_trade_ids: HashSet<String>,
    /// Intended orders recorded before the exchange acknowledges them.
    submissions: HashMap<SmolStr, PendingSubmission>,
    /// Journal or recovery failure: block further sends until the process is repaired.
    exposure_halt: Option<String>,
    last_block_reason_code: Option<trading_core::diagnostics::ReasonCode>,
    instrument_type: InstrumentType,
    /// Futures pairs whose REST/WS position snapshot has not been applied yet.
    position_sync_pending: HashSet<Pair>,
    /// WS position disagrees with the last REST snapshot; block all new orders.
    position_disagreement: HashSet<Pair>,
}

impl TradingState {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            active_orders: HashMap::new(),
            pending_orders: HashMap::new(),
            fill_applied: HashMap::new(),
            applied_trade_ids: HashSet::new(),
            submissions: HashMap::new(),
            exposure_halt: None,
            last_block_reason_code: None,
            instrument_type: InstrumentType::Spot,
            position_sync_pending: HashSet::new(),
            position_disagreement: HashSet::new(),
        }
    }

    pub fn with_instrument_type(mut self, instrument_type: InstrumentType) -> Self {
        self.instrument_type = instrument_type;
        self
    }

    pub fn instrument_type(&self) -> &InstrumentType {
        &self.instrument_type
    }

    fn is_futures(&self) -> bool {
        matches!(
            self.instrument_type,
            InstrumentType::Futures | InstrumentType::Derivitives
        )
    }

    pub fn mark_position_sync_pending(&mut self, pair: Pair) {
        if self.is_futures() {
            self.position_sync_pending.insert(pair);
        }
    }

    pub fn mark_position_synced(&mut self, pair: &Pair) {
        self.position_sync_pending.remove(pair);
    }

    pub fn position_sync_pending(&self, pair: &Pair) -> bool {
        self.position_sync_pending.contains(pair)
    }

    pub fn position_disagreement(&self, pair: &Pair) -> bool {
        self.position_disagreement.contains(pair)
    }

    pub fn note_position_disagreement(&mut self, pair: Pair) {
        self.position_disagreement.insert(pair.clone());
        self.last_block_reason_code =
            Some(trading_core::diagnostics::ReasonCode::ReconcileDisagreement);
        trading_core::diagnostics::emit_decision(
            "halt",
            trading_core::diagnostics::ReasonCode::ReconcileDisagreement,
            "REST/WS position disagreement",
        );
        self.mark_position_sync_pending(pair);
    }

    pub fn add_order(&mut self, order: Order) {
        let order_id = order.order_id.clone();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Track pending orders separately
        if matches!(
            order.status,
            OrderStatus::New | OrderStatus::PartiallyFilled
        ) {
            self.pending_orders.insert(order_id.clone(), timestamp);
        }

        self.active_orders.insert(order_id, order);
    }

    pub fn remove_order(&mut self, order_id: &OrderId) {
        self.active_orders.remove(order_id);
        self.pending_orders.remove(order_id);
    }

    /// Update order status (e.g., when filled or cancelled)
    pub fn update_order_status(
        &mut self,
        order_id: &OrderId,
        status: OrderStatus,
        filled_amount: Option<Decimal>,
    ) {
        if let Some(order) = self.active_orders.get_mut(order_id) {
            order.status = status.clone();
            if let Some(amount) = filled_amount {
                order.filled_amount = amount;
            }

            // Remove from pending if filled or cancelled
            if matches!(
                status,
                OrderStatus::Filled
                    | OrderStatus::Canceled
                    | OrderStatus::Rejected
                    | OrderStatus::Expired
            ) {
                self.pending_orders.remove(order_id);
            }
        }
    }

    /// Get stale orders (pending for more than timeout_secs)
    pub fn get_stale_orders(&self, timeout_secs: u64) -> Vec<OrderId> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.pending_orders
            .iter()
            .filter(|(_, &timestamp)| now.saturating_sub(timestamp) > timeout_secs)
            .map(|(order_id, _)| order_id.clone())
            .collect()
    }

    pub fn update_position(&mut self, symbol: Pair, quantity: Decimal, price: Decimal) {
        let position = self.positions.entry(symbol.clone()).or_insert(Position {
            symbol: symbol.clone(),
            quantity: Decimal::ZERO,
            avg_price: Decimal::ZERO,
        });

        if quantity > Decimal::ZERO {
            // Adding to position
            let total_cost = position.avg_price * position.quantity + price * quantity;
            position.quantity += quantity;
            if position.quantity > Decimal::ZERO {
                position.avg_price = total_cost / position.quantity;
            }
        } else {
            // Reducing position
            let symbol_to_remove = position.symbol.clone();
            position.quantity += quantity; // quantity is negative for sells
            if position.quantity <= Decimal::ZERO {
                self.positions.remove(&symbol_to_remove);
            }
        }
    }

    pub fn get_position(&self, symbol: &Pair) -> Option<&Position> {
        self.positions.get(symbol)
    }

    /// Replace the position for a pair with an absolute quantity (exchange truth).
    /// Zero (or near-zero) quantity removes the position. REST/recon path.
    pub fn set_position(&mut self, symbol: Pair, quantity: Decimal, avg_price: Decimal) {
        self.write_position(symbol.clone(), quantity, avg_price);
        self.mark_position_synced(&symbol);
        self.position_disagreement.remove(&symbol);
    }

    /// Apply a private-stream position. A jump vs the current qty is unverified
    /// until REST recon confirms it; new exposure stays blocked in the meantime.
    pub fn apply_stream_position(&mut self, symbol: Pair, quantity: Decimal, avg_price: Decimal) {
        let previous = self.position_qty(&symbol);
        self.write_position(symbol.clone(), quantity, avg_price);
        if (quantity - previous).abs() > Decimal::new(1, 6) {
            tracing::warn!(
                pair = %symbol,
                stream = %quantity,
                previous = %previous,
                "REST/WS position disagreement blocks new exposure"
            );
            self.note_position_disagreement(symbol);
        }
    }

    fn write_position(&mut self, symbol: Pair, quantity: Decimal, avg_price: Decimal) {
        if quantity.abs() <= Decimal::new(1, 6) {
            self.positions.remove(&symbol);
            return;
        }
        self.positions.insert(
            symbol.clone(),
            Position {
                symbol,
                quantity,
                avg_price,
            },
        );
    }

    pub fn get_active_order_count(&self, symbol: &Pair) -> usize {
        self.active_orders
            .values()
            .filter(|o| &o.instrument.pair == symbol)
            .count()
    }

    pub fn position_qty(&self, symbol: &Pair) -> Decimal {
        self.positions
            .get(symbol)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO)
    }

    /// Signed remaining qty on working orders (buy +, sell −).
    pub fn working_qty(&self, symbol: &Pair) -> Decimal {
        self.active_orders
            .values()
            .filter(|o| {
                o.instrument.pair == *symbol
                    && matches!(o.status, OrderStatus::New | OrderStatus::PartiallyFilled)
            })
            .map(|o| {
                let remaining = (o.amount - o.filled_amount).max(Decimal::ZERO);
                match o.side {
                    Side::Buy => remaining,
                    Side::Sell => -remaining,
                }
            })
            .sum()
    }

    /// Remaining working + reserved qty on `side` (adverse fill does not credit the opposite side).
    pub fn unfilled_same_side_qty(&self, symbol: &Pair, side: Side) -> Decimal {
        let working: Decimal = self
            .active_orders
            .values()
            .filter(|order| {
                order.instrument.pair == *symbol
                    && order.side == side
                    && matches!(
                        order.status,
                        OrderStatus::New | OrderStatus::PartiallyFilled
                    )
            })
            .map(|order| (order.amount - order.filled_amount).max(Decimal::ZERO))
            .sum();
        let reserved: Decimal = self
            .submissions
            .values()
            .filter(|submission| &submission.symbol == symbol && submission.side == side)
            .map(|submission| submission.amount)
            .sum();
        working + reserved
    }

    /// Position plus working quantity plus submissions whose ack is still unknown.
    pub fn effective_position_qty(&self, symbol: &Pair) -> Decimal {
        self.position_qty(symbol) + self.working_qty(symbol) + self.reserved_submission_qty(symbol)
    }

    /// Signed quantity reserved by submissions recorded before an ack.
    pub fn reserved_submission_qty(&self, symbol: &Pair) -> Decimal {
        self.submissions
            .values()
            .filter(|submission| &submission.symbol == symbol)
            .map(|submission| match submission.side {
                Side::Buy => submission.amount,
                Side::Sell => -submission.amount,
            })
            .sum()
    }

    /// Apply an incremental fill once per order id. Safe to call twice with the same fill.
    ///
    /// Spot position quantity is updated only by [`Self::apply_unique_trade`] so an
    /// order update plus its trade cannot double-count. Futures quantity stays REST/WS
    /// truth; this only advances the fill cursor.
    pub fn apply_fill(&mut self, order: &Order) {
        let prev = self
            .fill_applied
            .get(&order.order_id)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let delta = (order.filled_amount - prev).max(Decimal::ZERO);
        if delta <= Decimal::ZERO {
            return;
        }
        self.fill_applied
            .insert(order.order_id.clone(), order.filled_amount);
    }

    /// Apply a private-stream trade once. Returns whether position quantity changed.
    /// Uses the trade's execution price. Repeated messages with the same id are no-ops.
    pub fn apply_unique_trade(&mut self, trade: &Trade) -> bool {
        if self.is_futures() {
            return false;
        }
        if trade.quantity <= Decimal::ZERO {
            return false;
        }
        let id = if trade.trade_id.is_empty() {
            format!(
                "order:{}:{}:{}",
                trade.order_id.as_str(),
                trade.quantity,
                trade.timestamp
            )
        } else {
            trade.trade_id.to_string()
        };
        if !self.applied_trade_ids.insert(id.clone()) {
            tracing::debug!(
                event = "state_transition",
                reason_code = %trading_core::diagnostics::ReasonCode::FillDuplicate,
                execution_id = %id,
                "duplicate fill ignored"
            );
            return false;
        }
        let signed = match trade.side {
            Side::Buy => trade.quantity,
            Side::Sell => -trade.quantity,
        };
        self.update_position(trade.pair.clone(), signed, trade.price);
        trading_core::diagnostics::emit_state_transition(
            "fill",
            trading_core::diagnostics::ReasonCode::FillApplied,
            &id,
        );
        true
    }

    /// Restart recovery for a terminal order whose fills are not yet in position
    /// and have no live trade id. No-op when wallet recon already seeded quantity.
    pub fn apply_recovered_fill(&mut self, order: &Order) {
        self.apply_fill(order);
        if self.is_futures() || order.filled_amount <= Decimal::ZERO {
            return;
        }
        if self.position_qty(&order.instrument.pair).abs() > Decimal::new(1, 6) {
            return;
        }
        let _ = self.apply_unique_trade(&Trade {
            trade_id: format!("recover:{}", order.order_id.as_str()).into(),
            order_id: order.order_id.clone(),
            pair: order.instrument.pair.clone(),
            price: order.price,
            quantity: order.filled_amount,
            side: order.side.clone(),
            timestamp: order
                .exchange_created_at
                .unwrap_or(order.internal_created_at),
            fee: Decimal::ZERO,
            fee_currency: trading_core::types::common::Symbol::from(
                order.instrument.pair.quote.as_str(),
            ),
        });
    }

    /// Record an intended order before calling the exchange.
    pub fn begin_submission(&mut self, submission: PendingSubmission) {
        self.submissions
            .insert(submission.client_order_id.clone(), submission);
    }

    /// Exchange answered. The id may now belong to an active order or a known reject.
    pub fn clear_submission(&mut self, client_order_id: &str) {
        self.submissions.remove(client_order_id);
    }

    pub fn clear_submission_for_order(&mut self, order: &Order) {
        if let Some(client_order_id) = order.client_order_id.as_ref() {
            self.submissions.remove(client_order_id.as_str());
        }
    }

    /// Transfer reserved exposure into a found working order without a gap or double count.
    ///
    /// Cumulative `filled_amount` is applied before the reservation is released so a missed
    /// private fill cannot disappear from effective exposure.
    pub fn import_working_order(&mut self, order: Order) {
        self.apply_recovered_fill(&order);
        self.clear_submission_for_order(&order);
        self.add_order(order);
    }

    /// Block further order sends after a durability or recovery failure.
    pub fn halt_new_exposure(&mut self, reason: impl Into<String>) {
        self.halt_new_exposure_with(
            trading_core::diagnostics::ReasonCode::ExposureHalted,
            reason,
        );
    }

    /// Block further order sends and record a stable reason code.
    pub fn halt_new_exposure_with(
        &mut self,
        code: trading_core::diagnostics::ReasonCode,
        reason: impl Into<String>,
    ) {
        let reason = reason.into();
        self.last_block_reason_code = Some(code);
        tracing::error!(
            event = "decision",
            action = "halt",
            reason_code = %code,
            reason = %reason,
            "new exposure halted"
        );
        trading_core::diagnostics::emit_decision("halt", code, &reason);
        self.exposure_halt = Some(reason);
    }

    pub fn exposure_halt_reason(&self) -> Option<&str> {
        self.exposure_halt.as_deref()
    }

    pub fn last_block_reason_code(&self) -> Option<trading_core::diagnostics::ReasonCode> {
        self.last_block_reason_code
    }

    /// True when the pair has an unresolved submission, the account is halted,
    /// or REST/WS position quantities disagree.
    pub fn blocks_new_exposure(&self, symbol: &Pair) -> bool {
        self.exposure_halt.is_some()
            || self.has_unresolved_submission(symbol)
            || self.position_disagreement(symbol)
    }

    /// Increases (opens/enlarges) are blocked until position sync and halt/unknown are clear.
    /// Reductions remain allowed unless the account is halted or a submission is unresolved.
    pub fn blocks_exposure_increase(&self, symbol: &Pair) -> bool {
        self.blocks_new_exposure(symbol) || self.position_sync_pending(symbol)
    }

    pub fn pending_submissions(&self) -> &HashMap<SmolStr, PendingSubmission> {
        &self.submissions
    }

    /// True while any submission on this pair has not been reconciled.
    pub fn has_unresolved_submission(&self, symbol: &Pair) -> bool {
        self.submissions
            .values()
            .any(|submission| &submission.symbol == symbol)
    }
}

/// Trait that all trading strategies must implement
#[async_trait]
pub trait Strategy: Send + Sync {
    /// Evaluate desired exposure. Must not construct exchange orders.
    async fn evaluate(
        &mut self,
        ctx: &crate::cex::intent::StrategyContext<'_>,
    ) -> Result<crate::cex::intent::StrategyIntent, EngineError>;

    /// Called when an execution report is received.
    /// Strategy-local bookkeeping only — account truth is `TradingState` via `ctx.account`.
    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError>;

    /// Last known market price for reconciliation. Default is unknown.
    fn get_last_price(&self) -> Option<rust_decimal::Decimal> {
        None
    }

    /// Seed completed daily bars `(timestamp_ms, high, low, close)`. Default ignores.
    async fn seed_daily_bars(
        &mut self,
        bars: &[(
            u64,
            rust_decimal::Decimal,
            rust_decimal::Decimal,
            rust_decimal::Decimal,
        )],
    ) -> Result<(), EngineError> {
        let _ = bars;
        Ok(())
    }

    /// A runner-owned completed candle. Default ignores so orderbook strategies stay unchanged.
    async fn on_completed_candle(
        &mut self,
        candle: &trading_core::types::CompletedCandle,
    ) -> Result<(), EngineError> {
        let _ = candle;
        Ok(())
    }

    /// Bootstrap finished reconstructing model state. Default no-op.
    fn finish_model_reconstruction(&mut self) {}

    /// Operator-visible model snapshot. Default is none.
    fn runtime_snapshot(&self, now_ms: u64) -> Option<crate::cex::shadow::StrategyRuntimeSnapshot> {
        let _ = now_ms;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::{Instrument, InstrumentType, Side, Symbol};
    use trading_core::types::exchange::Trade;

    #[test]
    fn test_trading_state_new() {
        let state = TradingState::new();
        assert!(state.positions.is_empty());
        assert!(state.active_orders.is_empty());
        assert!(state.pending_orders.is_empty());
    }

    #[test]
    fn test_trading_state_add_order_new() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");
        let order = Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, pair.clone()),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order.clone());
        assert_eq!(state.active_orders.len(), 1);
        assert_eq!(state.pending_orders.len(), 1);
        assert!(state.pending_orders.contains_key(&order.order_id));
    }

    #[test]
    fn test_trading_state_add_order_filled() {
        let mut state = TradingState::new();
        let order = Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::Filled,
            filled_amount: Decimal::new(10, 2),
        };

        state.add_order(order.clone());
        assert_eq!(state.active_orders.len(), 1);
        assert_eq!(state.pending_orders.len(), 0); // Filled orders not pending
    }

    #[test]
    fn test_trading_state_remove_order() {
        let mut state = TradingState::new();
        let order_id = OrderId::new("12345".into());
        let order = Order {
            order_id: order_id.clone(),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order);
        assert_eq!(state.active_orders.len(), 1);

        state.remove_order(&order_id);
        assert_eq!(state.active_orders.len(), 0);
        assert_eq!(state.pending_orders.len(), 0);
    }

    #[test]
    fn test_trading_state_update_order_status_to_filled() {
        let mut state = TradingState::new();
        let order_id = OrderId::new("12345".into());
        let order = Order {
            order_id: order_id.clone(),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order);
        assert_eq!(state.pending_orders.len(), 1);

        state.update_order_status(&order_id, OrderStatus::Filled, Some(Decimal::new(10, 2)));
        assert_eq!(state.pending_orders.len(), 0);
        assert_eq!(
            state.active_orders.get(&order_id).unwrap().status,
            OrderStatus::Filled
        );
        assert_eq!(
            state.active_orders.get(&order_id).unwrap().filled_amount,
            Decimal::new(10, 2)
        );
    }

    #[test]
    fn test_trading_state_update_position_add() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(90000, 0));
        let position = state.get_position(&pair).unwrap();
        assert_eq!(position.quantity, Decimal::new(1, 0));
        assert_eq!(position.avg_price, Decimal::new(90000, 0));
    }

    #[test]
    fn test_trading_state_update_position_add_multiple() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        // First buy at 90000
        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(90000, 0));
        // Second buy at 91000
        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(91000, 0));

        let position = state.get_position(&pair).unwrap();
        assert_eq!(position.quantity, Decimal::new(2, 0));
        // Average should be (90000 + 91000) / 2 = 90500
        assert_eq!(position.avg_price, Decimal::new(90500, 0));
    }

    #[test]
    fn test_trading_state_update_position_reduce() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        // Add position
        state.update_position(pair.clone(), Decimal::new(2, 0), Decimal::new(90000, 0));
        // Reduce position (sell)
        state.update_position(pair.clone(), Decimal::new(-1, 0), Decimal::new(91000, 0));

        let position = state.get_position(&pair).unwrap();
        assert_eq!(position.quantity, Decimal::new(1, 0));
    }

    #[test]
    fn test_trading_state_update_position_close() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        // Add position
        state.update_position(pair.clone(), Decimal::new(1, 0), Decimal::new(90000, 0));
        // Close position completely
        state.update_position(pair.clone(), Decimal::new(-1, 0), Decimal::new(91000, 0));

        assert!(state.get_position(&pair).is_none());
    }

    #[test]
    fn test_trading_state_get_active_order_count() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");

        let order1 = Order {
            order_id: OrderId::new("1".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let order2 = Order {
            order_id: OrderId::new("2".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        let order3 = Order {
            order_id: OrderId::new("3".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("ETH", "USDT")), // Different symbol
            side: Side::Buy,
            price: Decimal::new(3000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order1);
        state.add_order(order2);
        state.add_order(order3);

        assert_eq!(state.get_active_order_count(&pair), 2);
    }

    #[test]
    fn test_trading_state_get_stale_orders() {
        let mut state = TradingState::new();
        let order_id = OrderId::new("12345".into());
        let order = Order {
            order_id: order_id.clone(),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(90000, 0),
            amount: Decimal::new(10, 2),
            internal_created_at: 1734819700000,
            exchange_created_at: Some(1734819696263),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };

        state.add_order(order);

        // Manually set a very old timestamp
        let old_timestamp = 1000; // Very old
        state.pending_orders.insert(order_id.clone(), old_timestamp);

        let stale = state.get_stale_orders(60); // 60 second timeout
        assert!(stale.contains(&order_id));
    }

    #[test]
    fn effective_qty_includes_working_buy_and_fill_is_idempotent() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");
        let order = Order {
            order_id: OrderId::new("ord-1".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Futures, pair.clone()),
            side: Side::Buy,
            price: Decimal::new(77000, 0),
            amount: Decimal::new(2, 3),
            internal_created_at: 1,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        state.add_order(order.clone());
        assert_eq!(state.position_qty(&pair), Decimal::ZERO);
        assert_eq!(state.working_qty(&pair), Decimal::new(2, 3));
        assert_eq!(state.effective_position_qty(&pair), Decimal::new(2, 3));

        let mut filled = order.clone();
        filled.status = OrderStatus::Filled;
        filled.filled_amount = Decimal::new(2, 3);
        state.apply_fill(&filled);
        state.apply_fill(&filled);
        let trade = Trade {
            trade_id: "t-1".into(),
            order_id: filled.order_id.clone(),
            pair: pair.clone(),
            price: Decimal::new(77000, 0),
            quantity: Decimal::new(2, 3),
            side: Side::Buy,
            timestamp: 1,
            fee: Decimal::ZERO,
            fee_currency: Symbol::from("USDT"),
        };
        state.apply_unique_trade(&trade);
        state.apply_unique_trade(&trade);
        state.remove_order(&filled.order_id);
        assert_eq!(state.position_qty(&pair), Decimal::new(2, 3));
        assert_eq!(state.working_qty(&pair), Decimal::ZERO);
        assert_eq!(state.effective_position_qty(&pair), Decimal::new(2, 3));
    }

    #[test]
    fn halt_blocks_new_exposure_without_a_submission() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");
        assert!(!state.blocks_new_exposure(&pair));
        state.halt_new_exposure("journal unwritable");
        assert!(state.blocks_new_exposure(&pair));
        assert_eq!(state.exposure_halt_reason(), Some("journal unwritable"));
    }

    #[test]
    fn futures_fill_does_not_double_count_with_position_event() {
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        let pair = Pair::new("BTC", "USDT");
        state.mark_position_sync_pending(pair.clone());
        assert!(state.blocks_exposure_increase(&pair));
        assert!(!state.blocks_new_exposure(&pair));

        let order = Order {
            order_id: OrderId::new("fut-1".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Futures, pair.clone()),
            side: Side::Buy,
            price: Decimal::new(50000, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: None,
            status: OrderStatus::Filled,
            filled_amount: Decimal::new(1, 3),
        };
        state.apply_fill(&order);
        state.apply_fill(&order);
        assert_eq!(
            state.position_qty(&pair),
            Decimal::ZERO,
            "fills must not set futures position"
        );

        state.set_position(pair.clone(), Decimal::new(1, 3), Decimal::new(50000, 0));
        assert_eq!(state.position_qty(&pair), Decimal::new(1, 3));
        assert!(!state.position_sync_pending(&pair));
        assert!(!state.blocks_exposure_increase(&pair));
    }

    #[test]
    fn rest_position_overwrites_lying_ws_update() {
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        let pair = Pair::new("BTC", "USDT");
        state.set_position(pair.clone(), Decimal::new(1, 3), Decimal::new(50000, 0));
        state.apply_stream_position(pair.clone(), Decimal::new(1, 0), Decimal::new(50000, 0));
        assert_eq!(state.position_qty(&pair), Decimal::new(1, 0));
        assert!(state.position_disagreement(&pair));
        assert!(state.blocks_new_exposure(&pair));
        assert!(state.blocks_exposure_increase(&pair));
        state.set_position(pair.clone(), Decimal::new(1, 3), Decimal::new(50000, 0));
        assert_eq!(
            state.position_qty(&pair),
            Decimal::new(1, 3),
            "authoritative REST/snapshot qty must replace a bad WS update"
        );
        assert!(!state.position_disagreement(&pair));
        assert!(!state.blocks_new_exposure(&pair));
        let order = Order {
            order_id: OrderId::new("ord-1".into()),
            client_order_id: Some(smol_str::SmolStr::new("cid-1")),
            instrument: Instrument::new(InstrumentType::Futures, pair.clone()),
            side: Side::Buy,
            price: Decimal::new(50000, 0),
            amount: Decimal::new(2, 3),
            internal_created_at: 1,
            exchange_created_at: None,
            status: OrderStatus::PartiallyFilled,
            filled_amount: Decimal::new(1, 3),
        };
        state.apply_fill(&order);
        state.add_order(order.clone());
        assert_eq!(state.position_qty(&pair), Decimal::new(1, 3));
        assert_eq!(state.working_qty(&pair), Decimal::new(1, 3));
        state.remove_order(&order.order_id);
        assert_eq!(state.working_qty(&pair), Decimal::ZERO);
        assert_eq!(state.effective_position_qty(&pair), Decimal::new(1, 3));
    }

    #[test]
    fn unfilled_same_side_ignores_opposite_working() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");
        state.add_order(Order {
            order_id: OrderId::new("buy-1".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Futures, pair.clone()),
            side: Side::Buy,
            price: Decimal::new(100, 0),
            amount: Decimal::ONE,
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        });
        assert_eq!(state.unfilled_same_side_qty(&pair, Side::Buy), Decimal::ONE);
        assert_eq!(
            state.unfilled_same_side_qty(&pair, Side::Sell),
            Decimal::ZERO
        );
    }

    fn spot_trade(id: &str, order_id: &str, qty: Decimal, price: Decimal) -> Trade {
        Trade {
            trade_id: id.into(),
            order_id: OrderId::new(order_id.into()),
            pair: Pair::new("BTC", "USDT"),
            price,
            quantity: qty,
            side: Side::Buy,
            timestamp: 1,
            fee: Decimal::ZERO,
            fee_currency: Symbol::from("USDT"),
        }
    }

    fn spot_order(filled: Decimal) -> Order {
        Order {
            order_id: OrderId::new("ord-a".into()),
            client_order_id: Some(smol_str::SmolStr::new("cid-a")),
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: Decimal::new(1000, 0),
            amount: Decimal::new(1, 2),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: if filled > Decimal::ZERO {
                OrderStatus::PartiallyFilled
            } else {
                OrderStatus::New
            },
            filled_amount: filled,
        }
    }

    #[test]
    fn spot_order_plus_trade_changes_position_once() {
        let mut state = TradingState::new();
        let pair = Pair::new("BTC", "USDT");
        let mut order = spot_order(Decimal::new(1, 3));
        state.add_order(order.clone());
        state.apply_fill(&order);
        let trade = spot_trade("fill-1", "ord-a", Decimal::new(1, 3), Decimal::new(1000, 0));
        assert!(state.apply_unique_trade(&trade));
        state.apply_fill(&order);
        assert!(!state.apply_unique_trade(&trade));
        assert_eq!(state.position_qty(&pair), Decimal::new(1, 3));
        assert_eq!(
            state.get_position(&pair).unwrap().avg_price,
            Decimal::new(1000, 0)
        );

        order.filled_amount = Decimal::new(2, 3);
        state.apply_fill(&order);
        let trade2 = spot_trade("fill-2", "ord-a", Decimal::new(1, 3), Decimal::new(1020, 0));
        assert!(state.apply_unique_trade(&trade2));
        assert!(!state.apply_unique_trade(&trade2));
        assert_eq!(state.position_qty(&pair), Decimal::new(2, 3));
        assert_eq!(
            state.get_position(&pair).unwrap().avg_price,
            Decimal::new(1010, 0)
        );
    }
}
