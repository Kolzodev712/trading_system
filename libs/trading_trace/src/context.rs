//! Unified trace context for networking layer.

use crate::{ColdTraceContext, HotTraceContext};

#[derive(Debug, Clone, Default)]
pub enum TraceContext {
    Hot(HotTraceContext),
    Cold(ColdTraceContext),
    #[default]
    None,
}

impl TraceContext {
    pub fn correlation_id(&self) -> Option<u64> {
        match self {
            TraceContext::Hot(ctx) => ctx.correlation_id,
            TraceContext::Cold(ctx) => ctx.correlation_id,
            TraceContext::None => None,
        }
    }

    pub fn as_hot(&self) -> Option<&HotTraceContext> {
        match self {
            TraceContext::Hot(ctx) => Some(ctx),
            _ => None,
        }
    }

    pub fn as_cold(&self) -> Option<&ColdTraceContext> {
        match self {
            TraceContext::Cold(ctx) => Some(ctx),
            _ => None,
        }
    }

    pub fn is_hot(&self) -> bool {
        matches!(self, TraceContext::Hot(_))
    }

    pub fn is_cold(&self) -> bool {
        matches!(self, TraceContext::Cold(_))
    }

    pub fn is_some(&self) -> bool {
        !matches!(self, TraceContext::None)
    }
}

impl From<HotTraceContext> for TraceContext {
    fn from(ctx: HotTraceContext) -> Self {
        TraceContext::Hot(ctx)
    }
}

impl From<ColdTraceContext> for TraceContext {
    fn from(ctx: ColdTraceContext) -> Self {
        TraceContext::Cold(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_correlation_id() {
        let hot_ctx = HotTraceContext::new().with_correlation_id(12345);
        let trace_ctx = TraceContext::Hot(hot_ctx);
        assert_eq!(trace_ctx.correlation_id(), Some(12345));
    }

    #[test]
    fn test_trace_context_none() {
        let trace_ctx = TraceContext::None;
        assert_eq!(trace_ctx.correlation_id(), None);
        assert!(!trace_ctx.is_some());
    }

    #[test]
    fn test_trace_context_from_hot() {
        let hot_ctx = HotTraceContext::new();
        let trace_ctx: TraceContext = hot_ctx.into();
        assert!(trace_ctx.is_hot());
        assert!(!trace_ctx.is_cold());
        assert!(trace_ctx.is_some());
    }
}
