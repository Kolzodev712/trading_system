//! Pure strategy models.
//!
//! This crate must not depend on exchange transport, runtime orchestration,
//! historical accounting, or live account state. Research and production both
//! call the same model.

pub mod btc_liquidation_shock_v1;
pub mod btc_price_ma_long_flat_v1;
