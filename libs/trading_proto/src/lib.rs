//! Generated gRPC / protobuf types for the trading control-plane API.
//!
//! Source of truth: `proto/trading_cli.proto`. Compiled once here so the
//! gateway (server), CLI, trading_report and workflow_harness (clients) do not
//! each re-run `tonic_build`.

#[allow(clippy::all)]
pub mod trading_cli {
    tonic::include_proto!("trading_cli");
}
