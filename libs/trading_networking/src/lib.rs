//! # Trading Networking Library
//!
//! Reusable networking infrastructure for the trading system:
//!
//! - **Event types** ([`events`]): [`HotEvent`] (market data: orderbook, trades) and [`ColdEvent`]
//!   (exchange/account state: orders, balances, positions, trades), both with optional trace
//!   context slots for observability.
//! - **Event buses**: [`HotEventBus`] (bounded MPSC, low latency) and [`ColdEventBus`] (broadcast,
//!   multiple subscribers).
//! - **Commands** ([`commands`]): [`Command`] / [`CommandPayload`] types aligned with gRPC operations,
//!   plus [`CommandRouter`] and [`CommandDispatcher`] for request/response over a single-consumer channel.
//! - **Payload** ([`payload`]): JSON (de)serialization for hot/cold events and commands (wire format).
//! - **Zenoh** (optional `zenoh` feature): publish/subscribe for events and query-reply for commands over [Zenoh](https://zenoh.io/).
//!
//! This crate is the **event/command/payload foundation**; gateway, transports, and runner
//! integration are implemented in a later phase.

pub mod commands;
pub mod error;
pub mod events;
pub mod payload;

#[cfg(feature = "zenoh")]
pub mod zenoh_bridge;

pub use commands::{Command, CommandDispatcher, CommandPayload, CommandResponse, CommandRouter};
pub use error::{DtpError, NetworkError};
pub use events::{ColdEvent, ColdEventBus, HotEvent, HotEventBus};
pub use payload::{
    deserialize_cold_event, deserialize_command, deserialize_hot_event, serialize_cold_event,
    serialize_command, serialize_hot_event,
};

#[cfg(feature = "zenoh")]
pub use zenoh_bridge::{
    open_session, publish_cold_event, publish_hot_event, request_command, run_command_queryable,
    CommandQueryHandler,
};
