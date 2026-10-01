//! Test-only venue workflow harness.
//!
//! Production crates (`runner`, `trading_engine`) must not depend on this package.

pub mod artifacts;
pub mod demo;
pub mod demo_levels;
pub mod grpc;
pub mod oracle;
pub mod paper;
pub mod process;
pub mod scenarios;
pub mod types;
pub mod venue;

#[cfg(test)]
mod oracle_tests;

pub use types::{
    Capability, InstrumentKind, Layer, ScenarioResult, Verdict, WorkflowError, WorkflowMatrix,
};
