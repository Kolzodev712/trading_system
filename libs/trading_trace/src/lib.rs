//! Trading Trace Library
//!
//! Hybrid tracing approach: lightweight hot path (<1μs) and OpenTelemetry cold path (<10μs).

pub mod bridge;
pub mod cold;
pub mod config;
pub mod constants;
pub mod context;
pub mod correlation;
pub mod exporter;
pub mod hot;
pub mod metrics;

pub use bridge::TraceBridge;
pub use cold::ColdTraceContext;
pub use config::TraceConfig;
pub use context::TraceContext;
pub use correlation::{generate_correlation_id, CorrelationId};
pub use exporter::{get_tracer, init_exporters, shutdown_exporters, ExporterHandles};
pub use hot::HotTraceContext;
pub use metrics::{get_metrics, TradingMetrics};
