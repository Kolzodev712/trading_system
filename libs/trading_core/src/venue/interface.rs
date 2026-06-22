//! Venue identifiers and channel helpers (no engine crate dependencies).

use serde::{Deserialize, Serialize};

use crate::config::ChannelConfig;
use crate::venue::cex::channels::{
    cex_venue_update_channel, CexVenueUpdateReceiver, CexVenueUpdateSender,
};
use crate::venue::dex::channels::{
    dex_venue_update_channel, DexVenueUpdateReceiver, DexVenueUpdateSender,
};

/// Which execution stack is active for this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueMode {
    Cex,
    Dex,
    /// CEX and DEX stacks in one process (no Solana chain until Proposal 01).
    #[serde(alias = "dual", alias = "both")]
    Both,
}

/// Identifies a venue implementation (CEX exchange or DEX chain).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueId {
    Binance,
    Bybit,
    Paper,
    /// Reserved (Solana lending-era scaffold).
    Solana,
    /// DeepBook V3 CLOB exchange (`dex_engine`; chain in `DexChain`, e.g. Sui).
    #[serde(rename = "deepbook", alias = "deep_book", alias = "sui")]
    DeepBook,
}

/// Maps `Exchange` to `VenueId` for CEX configs.
pub fn venue_id_from_exchange(exchange: crate::types::common::Exchange) -> VenueId {
    use crate::types::common::Exchange;
    match exchange {
        Exchange::Binance => VenueId::Binance,
        Exchange::Bybit => VenueId::Bybit,
        Exchange::Paper => VenueId::Paper,
        Exchange::DeepBook => VenueId::DeepBook,
        Exchange::Unsupported => VenueId::Paper,
    }
}

/// Channel bundle created when wiring a venue.
pub struct VenueChannels {
    pub cex_tx: Option<CexVenueUpdateSender>,
    pub cex_rx: Option<CexVenueUpdateReceiver>,
    pub dex_tx: Option<DexVenueUpdateSender>,
    pub dex_rx: Option<DexVenueUpdateReceiver>,
}

impl VenueChannels {
    pub fn for_cex(channels: &ChannelConfig) -> Self {
        let (tx, rx) = cex_venue_update_channel(channels.cex_venue_update_capacity);
        Self {
            cex_tx: Some(tx),
            cex_rx: Some(rx),
            dex_tx: None,
            dex_rx: None,
        }
    }

    pub fn for_dex(channels: &ChannelConfig) -> Self {
        let (tx, rx) = dex_venue_update_channel(channels.dex_venue_update_capacity);
        Self {
            cex_tx: None,
            cex_rx: None,
            dex_tx: Some(tx),
            dex_rx: Some(rx),
        }
    }
}
