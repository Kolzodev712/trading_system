//! Configuration for tracing system.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    pub service_name: String,
    pub service_version: Option<String>,
    pub hot_path: HotPathConfig,
    pub cold_path: ColdPathConfig,
    pub exporters: ExportersConfig,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            service_name: "trading-system".to_string(),
            service_version: Some("0.1.0".to_string()),
            hot_path: HotPathConfig::default(),
            cold_path: ColdPathConfig::default(),
            exporters: ExportersConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotPathConfig {
    #[serde(default = "default_hot_sample_rate")]
    pub sample_rate: u32,
}

impl Default for HotPathConfig {
    fn default() -> Self {
        Self { sample_rate: 100 }
    }
}

fn default_hot_sample_rate() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdPathConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cold_sample_rate")]
    pub sample_rate: f64,
}

impl Default for ColdPathConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_rate: 1.0,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_cold_sample_rate() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportersConfig {
    pub jaeger: Option<JaegerConfig>,
    pub prometheus: Option<PrometheusConfig>,
}

impl Default for ExportersConfig {
    fn default() -> Self {
        Self {
            jaeger: Some(JaegerConfig::default()),
            prometheus: Some(PrometheusConfig::default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JaegerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_jaeger_endpoint")]
    pub agent_endpoint: String,
}

impl Default for JaegerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            agent_endpoint: "localhost:6831".to_string(),
        }
    }
}

fn default_jaeger_endpoint() -> String {
    "localhost:6831".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_config_default() {
        let config = TraceConfig::default();
        assert_eq!(config.service_name, "trading-system");
        assert_eq!(config.hot_path.sample_rate, 100);
        assert!(config.cold_path.enabled);
    }

    #[test]
    fn test_trace_config_serialization() {
        let config = TraceConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TraceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.service_name, deserialized.service_name);
    }
}
