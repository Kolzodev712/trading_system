//! Read-only live risk snapshot. The engine still decides whether an order is allowed.

use crate::grpc::trading_cli;
use rust_decimal::Decimal;
use trading_engine::risk::RiskLimits;
use trading_engine::state::TradingState;

fn opt_dec(value: Option<Decimal>) -> String {
    value.map(|v| v.normalize().to_string()).unwrap_or_default()
}

fn opt_usize(value: Option<usize>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

pub fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn usage_from_state(state: &TradingState, instrument: &str) -> (Decimal, Decimal, Decimal) {
    let mut pairs: Vec<trading_core::types::common::Pair> =
        state.positions.keys().cloned().collect();
    for order in state.active_orders.values() {
        pairs.push(order.instrument.pair.clone());
    }
    pairs.sort_by(|a, b| a.base.cmp(&b.base).then(a.quote.cmp(&b.quote)));
    pairs.dedup();
    let selected: Vec<_> = pairs
        .into_iter()
        .filter(|pair| instrument_matches(instrument, pair))
        .collect();
    if selected.is_empty() {
        return (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    }
    let position = selected.iter().map(|pair| state.position_qty(pair)).sum();
    let working = selected.iter().map(|pair| state.working_qty(pair)).sum();
    let reserved = selected
        .iter()
        .map(|pair| state.reserved_submission_qty(pair))
        .sum();
    (position, working, reserved)
}

fn instrument_matches(instrument: &str, pair: &trading_core::types::common::Pair) -> bool {
    if instrument.is_empty() || instrument.eq_ignore_ascii_case("account") {
        return false;
    }
    let compact = instrument.replace(['/', '-'], "");
    let slash = format!("{}/{}", pair.base, pair.quote);
    let dash = format!("{}-{}", pair.base, pair.quote);
    let joined = format!("{}{}", pair.base, pair.quote);
    instrument.eq_ignore_ascii_case(&slash)
        || instrument.eq_ignore_ascii_case(&dash)
        || compact.eq_ignore_ascii_case(&joined)
}

pub fn build_risk_snapshot(
    limits: &RiskLimits,
    state: &TradingState,
    instrument: &str,
    daily_pnl: Option<Decimal>,
    observed_at_unix_ms: i64,
) -> trading_cli::RiskSnapshot {
    let (position_qty, working_qty, reserved_qty) = usage_from_state(state, instrument);
    let (daily, daily_source) = match daily_pnl {
        Some(v) => (v.normalize().to_string(), "ledger"),
        None => (String::new(), "unavailable"),
    };
    trading_cli::RiskSnapshot {
        available: true,
        unavailable_reason: String::new(),
        observed_at_unix_ms,
        total_max_position: opt_dec(limits.total_max_position),
        max_total_orders: opt_usize(limits.max_total_orders),
        max_notional: opt_dec(limits.max_notional),
        max_order_qty: opt_dec(limits.max_order_qty),
        daily_loss_limit_all_pairs: opt_dec(limits.daily_loss_limit_all_pairs),
        position_qty: position_qty.normalize().to_string(),
        working_qty: working_qty.normalize().to_string(),
        reserved_qty: reserved_qty.normalize().to_string(),
        daily_pnl: daily,
        daily_pnl_source: daily_source.to_string(),
        headroom: "unknown".to_string(),
        instrument: instrument.to_string(),
    }
}

pub fn unavailable_risk_snapshot(reason: impl Into<String>) -> trading_cli::RiskSnapshot {
    trading_cli::RiskSnapshot {
        available: false,
        unavailable_reason: reason.into(),
        observed_at_unix_ms: now_unix_ms(),
        total_max_position: String::new(),
        max_total_orders: String::new(),
        max_notional: String::new(),
        max_order_qty: String::new(),
        daily_loss_limit_all_pairs: String::new(),
        position_qty: String::new(),
        working_qty: String::new(),
        reserved_qty: String::new(),
        daily_pnl: String::new(),
        daily_pnl_source: "unavailable".to_string(),
        headroom: "unknown".to_string(),
        instrument: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::build_risk_snapshot;
    use rust_decimal::Decimal;
    use trading_engine::risk::RiskLimits;
    use trading_engine::state::TradingState;

    #[test]
    fn snapshot_reflects_updated_live_limits() {
        let mut limits = RiskLimits {
            total_max_position: Some(Decimal::from(1)),
            max_order_qty: Some(Decimal::new(2, 3)),
            ..RiskLimits::default()
        };
        let state = TradingState::new();
        let first = build_risk_snapshot(&limits, &state, "BTC/USDT", None, 1);
        assert_eq!(first.total_max_position, "1");
        assert_eq!(first.max_order_qty, "0.002");
        assert_eq!(first.headroom, "unknown");
        assert!(first.available);

        limits.total_max_position = Some(Decimal::from(5));
        let second = build_risk_snapshot(&limits, &state, "BTC/USDT", None, 2);
        assert_eq!(second.total_max_position, "5");
        assert_ne!(first.total_max_position, second.total_max_position);
    }

    #[test]
    fn missing_daily_pnl_is_unavailable_not_zero() {
        let snap = build_risk_snapshot(
            &RiskLimits::default(),
            &TradingState::new(),
            "BTC/USDT",
            None,
            1,
        );
        assert_eq!(snap.daily_pnl_source, "unavailable");
        assert!(snap.daily_pnl.is_empty());
    }

    #[test]
    fn requested_instrument_is_not_replaced_by_first_position() {
        let mut state = TradingState::new();
        state.set_position(
            trading_core::types::common::Pair::new("BTC", "USDT"),
            Decimal::new(2, 3),
            Decimal::from(50_000),
        );
        let snap = build_risk_snapshot(&RiskLimits::default(), &state, "ETH/USDT", None, 1);
        assert_eq!(snap.instrument, "ETH/USDT");
        assert_eq!(snap.position_qty, "0");
        assert_ne!(snap.position_qty, "0.002");
    }
}
