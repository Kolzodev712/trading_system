//! Zenoh transport counters (updated when `ZENOH_ENABLED=1`).

use std::sync::atomic::{AtomicU64, Ordering};

pub static ZENOH_TRANSPORT_SENT: AtomicU64 = AtomicU64::new(0);
pub static ZENOH_TRANSPORT_DROPPED: AtomicU64 = AtomicU64::new(0);
pub static ZENOH_TRANSPORT_PUBLISH_ERRORS: AtomicU64 = AtomicU64::new(0);

pub fn record_transport_snapshot(sent: u64, dropped: u64, publish_errors: u64) {
    ZENOH_TRANSPORT_SENT.store(sent, Ordering::Relaxed);
    ZENOH_TRANSPORT_DROPPED.store(dropped, Ordering::Relaxed);
    ZENOH_TRANSPORT_PUBLISH_ERRORS.store(publish_errors, Ordering::Relaxed);
}
