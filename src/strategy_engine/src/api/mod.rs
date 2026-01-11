//! Strategy Engine API
//!
//! Core types, traits, and building blocks for the strategy engine.
//! This module defines:
//! - Strategy IR types and definitions
//! - Configuration types
//! - Context types for strategy execution
//! - Strategy management interfaces
//!
//! This is the foundation layer - all other modules build on these types.

/// Strategy Intermediate Representation
pub mod ir;

/// Configuration management types
pub mod config {
    // TODO: Define configuration types
    // - Strategy config loading
    // - Parameter update types
    // - Validation types
}

/// Strategy execution context
pub mod context {
    // TODO: Define execution context types
    // - Market state
    // - Indicator values
    // - Strategy state
}

/// Strategy management API
pub mod manager {
    // TODO: Define manager interface types
    // - Strategy lifecycle
    // - Registration
    // - Updates
}

