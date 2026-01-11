//! Strategy Runner / Execution Layer
//!
//! Runtime execution of strategies:
//! - IR interpreter (interprets StrategyIR at runtime)
//! - Event loop (handles indicator updates, market data)
//! - Strategy runtime state management
//! - Order generation and execution
//!
//! This is the execution engine - it takes strategy IR and executes it.

// TODO: Implement runtime execution logic

/// IR Interpreter
pub mod interpreter {
    // TODO: Implement IR interpreter
    // - Load and register strategy IRs
    // - Evaluate strategy logic
    // - Generate orders based on conditions
}

/// Event Loop
pub mod event_loop {
    // TODO: Implement event loop
    // - Subscribe to indicator updates
    // - Subscribe to market data
    // - Process events and trigger strategy evaluation
}

/// Runtime State Management
pub mod runtime {
    // TODO: Implement runtime state
    // - Strategy state tracking
    // - Parameter cache
    // - Indicator value cache
}

/// Strategy Execution Engine
pub mod executor {
    // TODO: Implement execution engine
    // - Coordinates interpreter, event loop, runtime
    // - Manages strategy lifecycle
    // - Handles order generation
}

