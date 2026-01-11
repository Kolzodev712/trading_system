//! Strategy Engine
//!
//! Provides:
//! - Strategy DSL and IR (Intermediate Representation) translation
//! - IR interpreter for runtime strategy execution
//! - Runtime strategy parameter updates
//!
//! Architecture:
//! - Strategies can define their own indicators OR use system-wide indicators
//! - Strategies can be updated at runtime (parameters only, not structure)
//! - Strategy execution uses IR interpreter (for Python-derived strategies)
//!   or native Rust implementation (for maximum performance)
//!
//! Structure:
//! - `api/`: Core types, IR definitions, configuration types
//! - `runner/`: Execution layer, event loop, IR interpreter
//! - `python_core/`: Python translation support and validation

pub mod api;
pub mod python_core;
pub mod runner;

// Re-export commonly used types
pub use api::ir::{StrategyIR, StrategyIndicators, StrategyLogic, StrategyParameters};

