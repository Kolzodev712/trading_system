//! Hot path trace context for high-frequency operations.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotTraceContext {
    pub trace_id: u64,
    pub span_id: u64,
    pub parent_span_id: Option<u64>,
    pub correlation_id: Option<u64>,
}

impl HotTraceContext {
    pub fn new() -> Self {
        Self {
            trace_id: generate_trace_id(),
            span_id: generate_span_id(),
            parent_span_id: None,
            correlation_id: None,
        }
    }

    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: generate_span_id(),
            parent_span_id: Some(self.span_id),
            correlation_id: self.correlation_id,
        }
    }

    pub fn with_correlation_id(self, correlation_id: u64) -> Self {
        Self {
            correlation_id: Some(correlation_id),
            ..self
        }
    }
}

impl Default for HotTraceContext {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_trace_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let random: u64 = rand::random();
    counter.wrapping_mul(0x9e3779b9) ^ random
}

fn generate_span_id() -> u64 {
    rand::random()
}

pub fn should_trace_hot(sample_rate: u32) -> bool {
    if sample_rate == 0 {
        return false;
    }
    let random: u32 = rand::random();
    random.is_multiple_of(sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hot_trace_context_new() {
        let ctx = HotTraceContext::new();
        assert_ne!(ctx.trace_id, 0);
        assert_ne!(ctx.span_id, 0);
        assert_eq!(ctx.parent_span_id, None);
        assert_eq!(ctx.correlation_id, None);
    }

    #[test]
    fn test_hot_trace_context_child() {
        let parent = HotTraceContext::new();
        let child = parent.child();

        assert_eq!(child.trace_id, parent.trace_id);
        assert_ne!(child.span_id, parent.span_id);
        assert_eq!(child.parent_span_id, Some(parent.span_id));
        assert_eq!(child.correlation_id, parent.correlation_id);
    }

    #[test]
    fn test_with_correlation_id() {
        let mut ctx = HotTraceContext::new();
        let correlation_id = 12345;
        ctx = ctx.with_correlation_id(correlation_id);
        assert_eq!(ctx.correlation_id, Some(correlation_id));
    }

    #[test]
    fn test_should_trace_hot() {
        let mut traced_count = 0;
        for _ in 0..1000 {
            if should_trace_hot(100) {
                traced_count += 1;
            }
        }
        assert!(
            (5..=15).contains(&traced_count),
            "Sampling rate seems off: {} traces in 1000",
            traced_count
        );
    }

    #[test]
    fn test_should_trace_hot_zero_rate() {
        assert!(!should_trace_hot(0));
    }
}
