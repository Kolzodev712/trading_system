//! Runner orchestration type for the active venue stack.

use std::sync::Arc;

use dex_engine::runtime::DexEngineRuntime;
use tokio::sync::RwLock;
use trading_core::venue::cex::channels::CexVenueUpdateReceiver;
use trading_core::venue::cex::traits::TradingClient;
use trading_core::venue::cex::types::exchange::OrderRequest;
use trading_core::venue::dex::channels::DexVenueUpdateReceiver;
use trading_core::venue::{VenueId, VenueMode};

/// CEX execution handles.
pub struct CexVenueHandle {
    pub venue_id: VenueId,
    pub exchange_state_manager: Arc<exchange_engine::state::manager::ExchangeStateManager>,
    pub trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    pub cex_update_rx: CexVenueUpdateReceiver,
}

/// DEX execution handles.
pub struct DexVenueHandle {
    pub venue_id: VenueId,
    pub runtime: Arc<DexEngineRuntime>,
    pub dex_update_rx: DexVenueUpdateReceiver,
}

impl DexVenueHandle {
    pub fn dex_state_manager(&self) -> Arc<dex_engine::state::manager::DexStateManager> {
        self.runtime.state_manager.clone()
    }
}

/// Active venue — CEX, DEX, or both.
pub enum VenueInterface {
    Cex(CexVenueHandle),
    Dex(DexVenueHandle),
    Both {
        cex: CexVenueHandle,
        dex: DexVenueHandle,
    },
}

impl VenueInterface {
    pub fn mode(&self) -> VenueMode {
        match self {
            VenueInterface::Cex { .. } => VenueMode::Cex,
            VenueInterface::Dex { .. } => VenueMode::Dex,
            VenueInterface::Both { .. } => VenueMode::Both,
        }
    }

    pub fn primary_venue_id(&self) -> VenueId {
        match self {
            VenueInterface::Cex(h) => h.venue_id,
            VenueInterface::Dex(h) => h.venue_id,
            VenueInterface::Both { cex, .. } => cex.venue_id,
        }
    }

    pub fn exchange_state_manager(
        &self,
    ) -> Option<Arc<exchange_engine::state::manager::ExchangeStateManager>> {
        match self {
            VenueInterface::Cex(h) | VenueInterface::Both { cex: h, .. } => {
                Some(h.exchange_state_manager.clone())
            }
            VenueInterface::Dex { .. } => None,
        }
    }

    pub fn dex_state_manager(&self) -> Option<Arc<dex_engine::state::manager::DexStateManager>> {
        match self {
            VenueInterface::Dex(h) | VenueInterface::Both { dex: h, .. } => {
                Some(h.dex_state_manager())
            }
            VenueInterface::Cex { .. } => None,
        }
    }

    pub fn dex_runtime(&self) -> Option<Arc<DexEngineRuntime>> {
        match self {
            VenueInterface::Dex(h) | VenueInterface::Both { dex: h, .. } => Some(h.runtime.clone()),
            VenueInterface::Cex { .. } => None,
        }
    }

    pub fn trading_client(&self) -> Option<Arc<RwLock<dyn TradingClient<OrderRequest>>>> {
        match self {
            VenueInterface::Cex(h) | VenueInterface::Both { cex: h, .. } => {
                Some(h.trading_client.clone())
            }
            VenueInterface::Dex { .. } => None,
        }
    }

    pub fn cex_update_rx(&self) -> Option<CexVenueUpdateReceiver> {
        match self {
            VenueInterface::Cex(h) | VenueInterface::Both { cex: h, .. } => {
                Some(h.cex_update_rx.resubscribe())
            }
            VenueInterface::Dex { .. } => None,
        }
    }

    pub fn dex_update_rx(&self) -> Option<DexVenueUpdateReceiver> {
        match self {
            VenueInterface::Dex(h) | VenueInterface::Both { dex: h, .. } => {
                Some(h.dex_update_rx.resubscribe())
            }
            VenueInterface::Cex { .. } => None,
        }
    }
}
