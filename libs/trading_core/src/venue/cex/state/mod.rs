//! Exchange state management (thin CEX API surface).
//!
//! Holds in-memory balances/orders/trades from WebSocket streams, reconciles with REST,
//! and publishes updates. Venue adapters live in `exchange_engine`; this module is the
//! shared state API that `trading_engine` depends on without pulling adapters.

pub mod immediate;
pub mod manager;
pub mod publisher;
pub mod reconciler;
#[allow(clippy::module_inception)]
pub mod state;

pub use immediate::ImmediateTrigger;
pub use manager::ExchangeStateManager;
pub use state::ExchangeState;
