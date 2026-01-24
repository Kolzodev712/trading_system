//! Cold path trace context with OpenTelemetry integration.

use opentelemetry::{
    trace::{Span as SpanTrait, Status, Tracer as TracerTrait},
    KeyValue,
};
use opentelemetry_sdk::trace::Span as SdkSpan;
use std::borrow::Cow;

use crate::{constants::ATTR_CORRELATION_ID, CorrelationId};

#[derive(Debug)]
pub struct ColdTraceContext {
    pub correlation_id: Option<CorrelationId>,
    pub operation: Cow<'static, str>,
    span: Option<SdkSpan>,
}

impl Clone for ColdTraceContext {
    fn clone(&self) -> Self {
        Self {
            correlation_id: self.correlation_id,
            operation: self.operation.clone(),
            span: None,
        }
    }
}

impl ColdTraceContext {
    pub fn new_root(
        operation: impl Into<Cow<'static, str>>,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self {
            correlation_id,
            operation: operation.into(),
            span: None,
        }
    }

    pub fn new_root_with_tracer(
        tracer: &impl TracerTrait<Span = SdkSpan>,
        operation: &'static str,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        let mut span = tracer.start(operation);

        if let Some(corr_id) = correlation_id {
            span.set_attribute(KeyValue::new(ATTR_CORRELATION_ID, corr_id as i64));
        }

        Self {
            correlation_id,
            operation: Cow::Borrowed(operation),
            span: Some(span),
        }
    }

    pub fn child(&self, operation: impl Into<Cow<'static, str>>) -> Self {
        Self {
            correlation_id: self.correlation_id,
            operation: operation.into(),
            span: None,
        }
    }

    pub fn child_with_tracer(
        &self,
        tracer: &impl TracerTrait<Span = SdkSpan>,
        operation: &'static str,
    ) -> Self {
        let span = tracer.start(operation);

        Self {
            correlation_id: self.correlation_id,
            operation: Cow::Borrowed(operation),
            span: Some(span),
        }
    }

    pub fn end(mut self) {
        if let Some(mut span) = self.span.take() {
            span.end();
        }
    }

    pub fn set_attribute(&mut self, key: &'static str, value: impl Into<opentelemetry::Value>) {
        if let Some(ref mut span) = self.span {
            span.set_attribute(KeyValue::new(key, value));
        }
    }

    pub fn add_event(&mut self, name: &'static str) {
        if let Some(ref mut span) = self.span {
            span.add_event(name, Vec::new());
        }
    }

    pub fn set_status(&mut self, status: Status) {
        if let Some(ref mut span) = self.span {
            span.set_status(status);
        }
    }

    pub fn span(&self) -> Option<&SdkSpan> {
        self.span.as_ref()
    }
}

impl Drop for ColdTraceContext {
    fn drop(&mut self) {
        if let Some(mut span) = self.span.take() {
            span.end();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cold_trace_context_new_root() {
        let ctx = ColdTraceContext::new_root("test_operation", None);
        assert_eq!(ctx.operation.as_ref(), "test_operation");
        assert_eq!(ctx.correlation_id, None);
    }

    #[test]
    fn test_cold_trace_context_with_correlation() {
        let correlation_id = 12345;
        let ctx = ColdTraceContext::new_root("test_operation", Some(correlation_id));
        assert_eq!(ctx.correlation_id, Some(correlation_id));
    }

    #[test]
    fn test_cold_trace_context_child() {
        let parent = ColdTraceContext::new_root("parent", None);
        let child = parent.child("child");

        assert_eq!(child.correlation_id, parent.correlation_id);
        assert_eq!(child.operation.as_ref(), "child");
    }

    #[test]
    fn test_cold_trace_context_end() {
        let ctx = ColdTraceContext::new_root("test", None);
        ctx.end();
    }
}
