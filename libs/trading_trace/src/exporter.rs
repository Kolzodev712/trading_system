//! OpenTelemetry exporters for Jaeger and Prometheus.

use crate::config::ExportersConfig;
use anyhow::Result;
use opentelemetry::global;
use opentelemetry_sdk::trace::TracerProvider;

#[cfg(feature = "full")]
use crate::{
    config::JaegerConfig,
    constants::{ATTR_SERVICE_NAME, ATTR_SERVICE_VERSION},
};
#[cfg(feature = "full")]
use opentelemetry::KeyValue;
#[cfg(feature = "full")]
use opentelemetry_jaeger::new_agent_pipeline;
#[cfg(feature = "full")]
use opentelemetry_sdk::runtime::Tokio;
#[cfg(feature = "full")]
use opentelemetry_sdk::Resource;
#[cfg(feature = "full")]
use prometheus::{Encoder, Opts, Registry, TextEncoder};
#[cfg(feature = "full")]
use std::sync::Arc;

#[cfg(feature = "full")]
#[derive(Clone)]
pub struct ExporterHandles {
    /// Prometheus registry for /metrics scrape (when Prometheus exporter is enabled).
    pub(crate) prometheus_registry: Option<Arc<Registry>>,
}

#[cfg(not(feature = "full"))]
#[derive(Clone)]
pub struct ExporterHandles;

pub fn init_exporters(
    #[cfg(feature = "full")] exporters: &ExportersConfig,
    #[cfg(not(feature = "full"))] _exporters: &ExportersConfig,
    #[cfg(feature = "full")] service_name: &str,
    #[cfg(not(feature = "full"))] _service_name: &str,
    #[cfg(feature = "full")] service_version: Option<&str>,
    #[cfg(not(feature = "full"))] _service_version: Option<&str>,
) -> Result<ExporterHandles> {
    #[cfg(feature = "full")]
    {
        let mut resource_attrs = vec![KeyValue::new(ATTR_SERVICE_NAME, service_name.to_string())];
        if let Some(version) = service_version {
            resource_attrs.push(KeyValue::new(ATTR_SERVICE_VERSION, version.to_string()));
        }
        let resource = Resource::new(resource_attrs);
        let mut prometheus_registry = None::<Arc<Registry>>;
        let mut jaeger_installed = false;

        if let Some(jaeger_config) = &exporters.jaeger {
            if jaeger_config.enabled {
                match init_jaeger_tracer_provider(jaeger_config, service_name, resource.clone()) {
                    Ok(()) => {
                        jaeger_installed = true;
                        tracing::info!(
                            "Jaeger tracer installed (agent endpoint: {})",
                            jaeger_config.agent_endpoint
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Jaeger tracer init failed (traces will not be exported): {}",
                            e
                        );
                    }
                }
            }
        }

        if !jaeger_installed {
            let provider = TracerProvider::builder().build();
            global::set_tracer_provider(provider);
        }

        if let Some(prom_config) = &exporters.prometheus {
            if prom_config.enabled {
                prometheus_registry = init_prometheus_registry(service_name, service_version);
            }
        }

        Ok(ExporterHandles {
            prometheus_registry,
        })
    }

    #[cfg(not(feature = "full"))]
    {
        let provider = TracerProvider::builder().build();
        global::set_tracer_provider(provider);
        Ok(ExporterHandles)
    }
}

#[cfg(feature = "full")]
fn init_jaeger_tracer_provider(
    config: &JaegerConfig,
    service_name: &str,
    _resource: Resource,
) -> Result<()> {
    let pipeline = new_agent_pipeline()
        .with_service_name(service_name)
        .with_endpoint(&config.agent_endpoint);
    let _tracer = pipeline.install_batch(Tokio)?;
    Ok(())
}

#[cfg(feature = "full")]
fn init_prometheus_registry(
    service_name: &str,
    service_version: Option<&str>,
) -> Option<Arc<Registry>> {
    let registry = Registry::new();

    // trading_system_info{service="...",version="..."} = 1
    let opts = Opts::new("trading_system_info", "Trading system info")
        .const_label("service", service_name)
        .const_label("version", service_version.unwrap_or("unknown"));
    let info_gauge = prometheus::Gauge::with_opts(opts).ok()?;
    info_gauge.set(1.0);
    registry.register(Box::new(info_gauge)).ok()?;

    // Start time (Unix seconds) for uptime-style queries
    let start_opts = Opts::new(
        "trading_system_start_timestamp_seconds",
        "Start time of the process (Unix seconds)",
    );
    let start_gauge = prometheus::Gauge::with_opts(start_opts).ok()?;
    start_gauge.set(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64(),
    );
    registry.register(Box::new(start_gauge)).ok()?;

    Some(Arc::new(registry))
}

/// Encode Prometheus metrics for HTTP scrape (GET /metrics).
#[cfg(feature = "full")]
pub fn encode_prometheus_metrics(
    handles: &ExporterHandles,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let registry = match &handles.prometheus_registry {
        Some(r) => r,
        None => return Ok(String::new()),
    };
    let metric_families = registry.gather();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

pub fn shutdown_exporters() {
    global::shutdown_tracer_provider();
    global::shutdown_meter_provider();
}

pub fn get_tracer(name: &'static str) -> opentelemetry::global::BoxedTracer {
    global::tracer(name)
}

/// Record a single startup span so the service appears in Jaeger immediately.
/// Cold-path spans are only created on exchange/level updates; this ensures "trading-system" shows up on load.
pub fn record_startup_span() {
    use opentelemetry::trace::{Span, Tracer};
    let tracer = get_tracer("runner");
    let mut span = tracer.start("trading_system.start");
    span.end();
}
