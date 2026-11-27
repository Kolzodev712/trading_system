//! Exchange state management (re-exported from `trading_core`).
//!
//! Ownership lives in `trading_core::venue::cex::state` so `trading_engine` can
//! depend on the thin CEX state API without pulling venue adapters.

pub use trading_core::venue::cex::state::{
    immediate, manager, publisher, reconciler, state, ExchangeState, ExchangeStateManager,
    ImmediateTrigger,
};
