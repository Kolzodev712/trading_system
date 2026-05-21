//! # Trading Networking Library
//!
//! Wire-format types and optional external transport for the trading system:
//!
//! - **Event types** ([`events`]): [`HotEvent`] (market data), [`ColdEvent`] (CEX state),
//!   [`DexColdEvent`] (DEX state), with optional trace context slots.
//! - **Conversion** ([`events::hot_event_from_market_data`], etc.): map `trading_core` channel
//!   updates to wire events (used by `runner::event_bridge`).
//! - **Event buses**: [`hot_event_channel`] (bounded MPSC) and [`ColdEventBus`] /
//!   [`DexColdEventBus`] (broadcast).
//! - **Commands** ([`commands`]): [`Command`] / [`CommandPayload`] for remote control.
//! - **Gateway** ([`gateway`]): [`EventGateway`] fans out buses to [`EventTransport`] impls.
//! - **Zenoh** (optional `zenoh` feature): publish/subscribe events and query-reply commands.
//!
//! Internal trading stays on `trading_core` broadcast channels; this crate is the
//! **external boundary only**. See `docs/PLATFORM_DATA_PLANE.md` and `docs/COMMAND_PLANE.md`.

pub mod commands;
pub mod error;
pub mod events;
pub mod gateway;
pub mod payload;

#[cfg(feature = "zenoh")]
pub mod zenoh_bridge;

pub use commands::{Command, CommandDispatcher, CommandPayload, CommandResponse, CommandRouter};
pub use error::NetworkError;
pub use events::{
    cold_event_bus, cold_event_from_cex, dex_cold_event_bus, dex_cold_event_from_dex,
    hot_event_channel, hot_event_from_market_data, ColdEvent, ColdEventBus, DexColdEvent,
    DexColdEventBus, HotEvent, HotEventRx, HotEventTx,
};
pub use payload::{
    deserialize_cold_event, deserialize_command, deserialize_dex_cold_event, deserialize_hot_event,
    serialize_cold_event, serialize_command, serialize_dex_cold_event, serialize_hot_event,
};

#[cfg(feature = "zenoh")]
pub use zenoh_bridge::{
    open_session, publish_cold_event, publish_dex_cold_event, publish_hot_event, request_command,
    run_command_queryable, CommandQueryHandler,
};
