//! Market data state management
//!
//! This module provides state management for market data (orderbooks, public trades)
//! received from WebSocket streams. The state is kept in-memory for ultra-low latency access.

pub mod manager;
pub mod publisher;
pub mod state;
