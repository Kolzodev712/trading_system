//! Exchange-specific market data engine tests
//!
//! Each exchange implements the same test suite to ensure consistent behavior
//! across all exchange integrations.

#[cfg(test)]
pub mod binance;

#[cfg(test)]
pub mod bybit;
