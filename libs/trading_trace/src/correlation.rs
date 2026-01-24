//! Correlation ID types and utilities.

use std::sync::atomic::{AtomicU64, Ordering};

pub type CorrelationId = u64;

pub fn generate_correlation_id() -> CorrelationId {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_correlation_id() {
        let id1 = generate_correlation_id();
        let id2 = generate_correlation_id();
        assert!(id1 > 0);
        assert!(id2 > id1);
    }
}
