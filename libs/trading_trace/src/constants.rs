//! Constants for attribute keys, metric names, and standard values.

// OpenTelemetry standard attribute keys
pub const ATTR_SERVICE_NAME: &str = "service.name";
pub const ATTR_SERVICE_VERSION: &str = "service.version";
pub const ATTR_CORRELATION_ID: &str = "correlation_id";

// Custom attribute keys
pub const ATTR_COMMAND_TYPE: &str = "command_type";
pub const ATTR_EVENT_TYPE: &str = "event_type";
pub const ATTR_OPERATION: &str = "operation";
pub const ATTR_ERROR_TYPE: &str = "error_type";
pub const ATTR_SUCCESS: &str = "success";

// Attribute values
pub const VALUE_SUCCESS_TRUE: &str = "true";
pub const VALUE_SUCCESS_FALSE: &str = "false";

// Metric names
pub const METRIC_COMMANDS_TOTAL: &str = "trading_commands_total";
pub const METRIC_EVENTS_TOTAL: &str = "trading_events_total";
pub const METRIC_OPERATION_DURATION_SECONDS: &str = "trading_operation_duration_seconds";
pub const METRIC_ERRORS_TOTAL: &str = "trading_errors_total";
