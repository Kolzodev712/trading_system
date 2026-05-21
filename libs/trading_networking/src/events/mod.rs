//! Event types and buses for hot (market data), cold (CEX state), and dex cold lanes.

mod cold;
mod cold_bus;
mod convert;
mod dex_cold;
mod dex_cold_bus;
mod hot;
mod hot_bus;

pub use cold::ColdEvent;
pub use cold_bus::{cold_event_bus, ColdEventBus, ColdEventBusError, ColdEventRx};
pub use convert::{cold_event_from_cex, dex_cold_event_from_dex, hot_event_from_market_data};
pub use dex_cold::DexColdEvent;
pub use dex_cold_bus::{dex_cold_event_bus, DexColdEventBus, DexColdEventBusError, DexColdEventRx};
pub use hot::HotEvent;
pub use hot_bus::{hot_event_channel, HotEventBusError, HotEventRx, HotEventTx};
