//! Cold lane event types (orders, balances, positions, trades).
//!
//! State-change events with optional trace context. Aligned with `ExchangeUpdate` for easy dual-publish.

use serde::{Deserialize, Serialize};
use trading_core::types::exchange::{Balance, Order, Position, Trade};
use trading_trace::ColdTraceContext;

/// Cold lane event (exchange/account state updates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColdEvent {
    /// Order update.
    Order {
        /// Order data.
        order: Order,
        /// Trace context (optional). Not serialized in DTP payload.
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
    /// Balance update.
    Balance {
        /// Balance data.
        balance: Balance,
        /// Trace context (optional).
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
    /// Position update.
    Position {
        /// Position data.
        position: Position,
        /// Trace context (optional).
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
    /// Trade (execution) update.
    Trade {
        /// Trade data.
        trade: Trade,
        /// Trace context (optional).
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<ColdTraceContext>,
    },
}
