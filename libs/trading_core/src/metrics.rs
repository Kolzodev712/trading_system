//! Process-wide counters shared across crates (runner scrape + engine lag paths).

use std::sync::atomic::{AtomicU64, Ordering};

/// Market-data broadcast lag (messages skipped).
pub static BROADCAST_LAG_MD: AtomicU64 = AtomicU64::new(0);
/// CEX private/venue update broadcast lag.
pub static BROADCAST_LAG_CEX: AtomicU64 = AtomicU64::new(0);
/// DEX venue update broadcast lag.
pub static BROADCAST_LAG_DEX: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn record_broadcast_lag_md(skipped: u64) {
    BROADCAST_LAG_MD.fetch_add(skipped, Ordering::Relaxed);
}

#[inline]
pub fn record_broadcast_lag_cex(skipped: u64) {
    BROADCAST_LAG_CEX.fetch_add(skipped, Ordering::Relaxed);
}

#[inline]
pub fn record_broadcast_lag_dex(skipped: u64) {
    BROADCAST_LAG_DEX.fetch_add(skipped, Ordering::Relaxed);
}
