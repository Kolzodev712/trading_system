//! Prometheus metrics support.

use crate::constants::{
    ATTR_COMMAND_TYPE, ATTR_ERROR_TYPE, ATTR_EVENT_TYPE, ATTR_OPERATION, ATTR_SUCCESS,
    METRIC_COMMANDS_TOTAL, METRIC_ERRORS_TOTAL, METRIC_EVENTS_TOTAL,
    METRIC_OPERATION_DURATION_SECONDS, VALUE_SUCCESS_FALSE, VALUE_SUCCESS_TRUE,
};
use opentelemetry::{
    global,
    metrics::{Counter, Histogram, Meter, MeterProvider as _},
    KeyValue,
};

#[derive(Clone)]
pub struct TradingMetrics {
    meter: Meter,
    pub command_counter: Counter<u64>,
    pub event_counter: Counter<u64>,
    pub operation_duration: Histogram<f64>,
    pub error_counter: Counter<u64>,
}

impl TradingMetrics {
    pub fn new(meter: Meter) -> Self {
        let command_counter = meter
            .u64_counter(METRIC_COMMANDS_TOTAL)
            .with_description("Total number of trading commands executed")
            .init();

        let event_counter = meter
            .u64_counter(METRIC_EVENTS_TOTAL)
            .with_description("Total number of trading events processed")
            .init();

        let operation_duration = meter
            .f64_histogram(METRIC_OPERATION_DURATION_SECONDS)
            .with_description("Duration of trading operations in seconds")
            .init();

        let error_counter = meter
            .u64_counter(METRIC_ERRORS_TOTAL)
            .with_description("Total number of trading errors")
            .init();

        Self {
            meter,
            command_counter,
            event_counter,
            operation_duration,
            error_counter,
        }
    }

    pub fn meter(&self) -> &Meter {
        &self.meter
    }

    pub fn record_command(&self, command_type: &'static str, success: bool) {
        let attributes = [
            KeyValue::new(ATTR_COMMAND_TYPE, command_type),
            KeyValue::new(
                ATTR_SUCCESS,
                if success {
                    VALUE_SUCCESS_TRUE
                } else {
                    VALUE_SUCCESS_FALSE
                },
            ),
        ];
        self.command_counter.add(1, &attributes);
    }

    pub fn record_event(&self, event_type: &'static str) {
        let attributes = [KeyValue::new(ATTR_EVENT_TYPE, event_type)];
        self.event_counter.add(1, &attributes);
    }

    pub fn record_duration(&self, operation: &'static str, duration_seconds: f64) {
        let attributes = [KeyValue::new(ATTR_OPERATION, operation)];
        self.operation_duration
            .record(duration_seconds, &attributes);
    }

    pub fn record_error(&self, error_type: &'static str) {
        let attributes = [KeyValue::new(ATTR_ERROR_TYPE, error_type)];
        self.error_counter.add(1, &attributes);
    }
}

pub fn get_metrics(name: &'static str) -> TradingMetrics {
    let meter_provider = global::meter_provider();
    let meter = meter_provider.meter(name);
    TradingMetrics::new(meter)
}
