//! OpenTelemetry exporters for Jaeger and Prometheus.

use crate::config::ExportersConfig;
use anyhow::Result;
use opentelemetry::{global, trace::Tracer};
use opentelemetry_sdk::trace::TracerProvider;

#[cfg(feature = "full")]
use crate::{
    config::JaegerConfig,
    constants::{ATTR_SERVICE_NAME, ATTR_SERVICE_VERSION},
};
#[cfg(feature = "full")]
use opentelemetry::KeyValue;
#[cfg(feature = "full")]
use opentelemetry_sdk::{metrics::MeterProvider, Resource};
#[cfg(feature = "full")]
use std::sync::Arc;

#[cfg(feature = "full")]
use opentelemetry_jaeger::new_agent_pipeline;
#[cfg(feature = "full")]
use opentelemetry_prometheus::{exporter as prometheus_exporter, PrometheusExporter};
#[cfg(feature = "full")]
use opentelemetry_sdk::runtime::Tokio;

#[cfg(feature = "full")]
pub struct ExporterHandles {
    pub prometheus_exporter: Option<Arc<PrometheusExporter>>,
}

#[cfg(not(feature = "full"))]
pub struct ExporterHandles;

pub fn init_exporters(
    #[cfg(feature = "full")] exporters: &ExportersConfig,
    #[cfg(not(feature = "full"))] _exporters: &ExportersConfig,
    #[cfg(feature = "full")] service_name: &str,
    #[cfg(not(feature = "full"))] _service_name: &str,
    #[cfg(feature = "full")] service_version: Option<&str>,
    #[cfg(not(feature = "full"))] _service_version: Option<&str>,
) -> Result<ExporterHandles> {
    let provider = TracerProvider::builder().build();
    global::set_tracer_provider(provider);

    #[cfg(feature = "full")]
    {
        let mut resource_attrs = vec![KeyValue::new(ATTR_SERVICE_NAME, service_name.to_string())];
        if let Some(version) = service_version {
            resource_attrs.push(KeyValue::new(ATTR_SERVICE_VERSION, version.to_string()));
        }
        let resource = Resource::new(resource_attrs);
        let mut prometheus_exporter = None;

        if let Some(jaeger_config) = &exporters.jaeger {
            if jaeger_config.enabled {
                let _ = init_jaeger_tracer_provider(jaeger_config, service_name, resource.clone());
            }
        }

        if let Some(prom_config) = &exporters.prometheus {
            if prom_config.enabled {
                if let Ok((_meter_provider, exporter)) =
                    init_prometheus_exporter(service_name, resource.clone())
                {
                    prometheus_exporter = Some(exporter);
                }
            }
        }

        Ok(ExporterHandles {
            prometheus_exporter,
        })
    }

    #[cfg(not(feature = "full"))]
    {
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
fn init_prometheus_exporter(
    _service_name: &str,
    _resource: Resource,
) -> Result<(Arc<MeterProvider>, Arc<PrometheusExporter>)> {
    let exporter = Arc::new(prometheus_exporter().build()?);
    let provider = MeterProvider::builder().build();
    global::set_meter_provider(provider.clone());
    Ok((Arc::new(provider), exporter))
}

pub fn shutdown_exporters() {
    global::shutdown_tracer_provider();
    global::shutdown_meter_provider();
}

pub fn get_tracer(name: &'static str) -> impl Tracer {
    global::tracer(name)
}
