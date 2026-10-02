//! One-shot read-only five-item trading report.
//!
//! This crate is an observer. It must not be imported by runner, trading_engine,
//! gateway, strategy_core, or research crates.

pub mod collect;
pub mod config;
pub mod journal_source;
pub mod model;
pub mod render;
pub mod runner_source;
pub mod schedule;
pub mod store;
pub mod venue_source;
pub mod verify;

pub use collect::{collect_report, write_outputs};
pub use config::ReportConfig;
pub use model::{TradingReport, Verdict};
