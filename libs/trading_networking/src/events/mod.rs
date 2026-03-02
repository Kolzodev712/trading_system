//! Event types and buses for hot (market data) and cold (state) lanes.

mod cold;
mod cold_bus;
mod hot;
mod hot_bus;

pub use cold::ColdEvent;
pub use cold_bus::{cold_event_bus, ColdEventBus, ColdEventBusError, ColdEventRx};
pub use hot::HotEvent;
pub use hot_bus::{hot_event_channel, HotEventBus, HotEventRx, HotEventTx};
