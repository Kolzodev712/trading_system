//! Exchange state management
//!
//! This module provides state management for exchange data (balances, orders, trades)
//! received from WebSocket streams. The state is kept in-memory for ultra-low latency access.

pub mod manager;
pub mod publisher;
pub mod reconciler;
pub mod state;
