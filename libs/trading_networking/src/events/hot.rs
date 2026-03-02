//! Hot lane event types (market data).
//!
//! Ultra-low latency events with minimal allocations. Used for orderbook and public trade updates.

use bytes::Bytes;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use trading_core::types::{common::Pair, market_data::StructuredOrderbook};
use trading_trace::HotTraceContext;

/// Hot lane market data event (allocation-light, compact).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HotEvent {
    /// Orderbook update (compact representation).
    Orderbook {
        /// Pair (or use pair_id when symbol registry is available).
        pair: Pair,
        /// Best bid price.
        best_bid: Decimal,
        /// Best ask price.
        best_ask: Decimal,
        /// Best bid quantity.
        bid_qty: Decimal,
        /// Best ask quantity.
        ask_qty: Decimal,
        /// Timestamp (nanoseconds or milliseconds per exchange).
        timestamp: u64,
        /// Full snapshot only when needed (rare); not serialized in DTP (stays None after deserialize).
        #[serde(skip)]
        full_snapshot: Option<Arc<Bytes>>,
        /// Trace context (optional, sampled on hot path). Not serialized in DTP payload.
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<HotTraceContext>,
    },
    /// Public trade update.
    Trade {
        /// Pair.
        pair: Pair,
        /// Trade price.
        price: Decimal,
        /// Trade quantity.
        quantity: Decimal,
        /// Timestamp.
        timestamp: u64,
        /// Optional trade ID.
        trade_id: Option<smol_str::SmolStr>,
        /// Trace context (optional).
        #[serde(skip)]
        #[allow(clippy::type_complexity)]
        trace_context: Option<HotTraceContext>,
    },
}

impl HotEvent {
    /// Create Orderbook variant from a structured orderbook (convenience for dual-publish).
    pub fn orderbook_from_structured(
        ob: &StructuredOrderbook,
        trace_context: Option<HotTraceContext>,
    ) -> Self {
        let (best_bid, bid_qty) = ob
            .bids
            .first()
            .copied()
            .unwrap_or((Decimal::ZERO, Decimal::ZERO));
        let (best_ask, ask_qty) = ob
            .asks
            .first()
            .copied()
            .unwrap_or((Decimal::ZERO, Decimal::ZERO));
        HotEvent::Orderbook {
            pair: ob.pair.clone(),
            best_bid,
            best_ask,
            bid_qty,
            ask_qty,
            timestamp: ob.timestamp,
            full_snapshot: None,
            trace_context,
        }
    }
}
