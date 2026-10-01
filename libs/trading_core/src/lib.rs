pub mod authenticator;
pub mod channels;
pub mod circuit_breaker;
pub mod config;
pub mod diagnostics;
pub mod error;
pub mod grpc;
pub mod persistence;
pub mod rate_limiter;
pub mod redis;
pub mod registry;
pub mod rest;
pub mod retry;
pub mod secret;
pub mod traits;
pub mod types;
pub mod utils;
pub mod venue;
pub mod websocket;
// Re-export WebSocket client traits for compatibility
pub use websocket::client::{MessageHandler, SubscriptionHandler, WebSocketClient};
// Re-export Exchange enum for compatibility
pub use types::common::Exchange;
