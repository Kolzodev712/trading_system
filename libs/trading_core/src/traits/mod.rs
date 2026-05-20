//! Deprecated shim — import `trading_core::venue::cex::traits` in new code.
//!
//! `src/` is gated by `just check-venue-imports` (no `trading_core::traits` there).
pub use crate::venue::cex::traits::*;
