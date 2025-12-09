pub mod authenticator;
pub mod channels;
pub mod circuit_breaker;
pub mod error;
pub mod grpc;
pub mod redis;
pub mod registry;
pub mod rest;
pub mod retry;
pub mod secret;
pub mod traits;
pub mod types;
pub mod utils;
pub mod websocket;
// Re-export WebSocket client traits for compatibility
pub use websocket::client::{MessageHandler, SubscriptionHandler, WebSocketClient};
// Re-export Exchange enum for compatibility
pub use types::common::Exchange;
