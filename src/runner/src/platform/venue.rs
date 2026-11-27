//! Runner orchestration type for the active venue stack.

use std::sync::Arc;

#[cfg(feature = "dex")]
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

/// DEX execution handles (requires `feature = "dex"`).
#[cfg(feature = "dex")]
pub struct DexVenueHandle {
    pub venue_id: VenueId,
    pub runtime: Arc<DexEngineRuntime>,
    pub dex_update_rx: DexVenueUpdateReceiver,
}

#[cfg(feature = "dex")]
impl DexVenueHandle {
    pub fn dex_state_manager(&self) -> Arc<dex_engine::state::manager::DexStateManager> {
        self.runtime.state_manager.clone()
    }
}

/// Active venue — CEX only by default; DEX and Both require `feature = "dex"`.
pub enum VenueInterface {
    Cex(CexVenueHandle),
    #[cfg(feature = "dex")]
    Dex(DexVenueHandle),
    #[cfg(feature = "dex")]
    Both {
        cex: CexVenueHandle,
        dex: DexVenueHandle,
    },
}

impl VenueInterface {
    pub fn mode(&self) -> VenueMode {
        match self {
            VenueInterface::Cex { .. } => VenueMode::Cex,
            #[cfg(feature = "dex")]
            VenueInterface::Dex { .. } => VenueMode::Dex,
            #[cfg(feature = "dex")]
            VenueInterface::Both { .. } => VenueMode::Both,
        }
    }

    pub fn primary_venue_id(&self) -> VenueId {
        match self {
            VenueInterface::Cex(h) => h.venue_id,
            #[cfg(feature = "dex")]
            VenueInterface::Dex(h) => h.venue_id,
            #[cfg(feature = "dex")]
            VenueInterface::Both { cex, .. } => cex.venue_id,
        }
    }

    /// CEX handle when this venue stack has one (`Cex` or `Both`).
    pub fn cex_handle(&self) -> Option<&CexVenueHandle> {
        match self {
            VenueInterface::Cex(h) => Some(h),
            #[cfg(feature = "dex")]
            VenueInterface::Both { cex, .. } => Some(cex),
            #[cfg(feature = "dex")]
            VenueInterface::Dex { .. } => None,
        }
    }

    pub fn exchange_state_manager(
        &self,
    ) -> Option<Arc<exchange_engine::state::manager::ExchangeStateManager>> {
        self.cex_handle().map(|h| h.exchange_state_manager.clone())
    }

    #[cfg(feature = "dex")]
    fn dex_handle(&self) -> Option<&DexVenueHandle> {
        match self {
            VenueInterface::Dex(h) | VenueInterface::Both { dex: h, .. } => Some(h),
            VenueInterface::Cex { .. } => None,
        }
    }

    #[cfg(feature = "dex")]
    pub fn dex_state_manager(&self) -> Option<Arc<dex_engine::state::manager::DexStateManager>> {
        self.dex_handle().map(|h| h.dex_state_manager())
    }

    #[cfg(feature = "dex")]
    pub fn dex_runtime(&self) -> Option<Arc<DexEngineRuntime>> {
        self.dex_handle().map(|h| h.runtime.clone())
    }

    pub fn trading_client(&self) -> Option<Arc<RwLock<dyn TradingClient<OrderRequest>>>> {
        self.cex_handle().map(|h| h.trading_client.clone())
    }

    pub fn cex_update_rx(&self) -> Option<CexVenueUpdateReceiver> {
        self.cex_handle().map(|h| h.cex_update_rx.resubscribe())
    }

    /// DEX venue updates; always `None` without `feature = "dex"`.
    pub fn dex_update_rx(&self) -> Option<DexVenueUpdateReceiver> {
        #[cfg(feature = "dex")]
        {
            self.dex_handle().map(|h| h.dex_update_rx.resubscribe())
        }
        #[cfg(not(feature = "dex"))]
        {
            None
        }
    }

    /// Whether a DEX venue handle is active; always `false` without `feature = "dex"`.
    pub fn has_dex(&self) -> bool {
        #[cfg(feature = "dex")]
        {
            self.dex_handle().is_some()
        }
        #[cfg(not(feature = "dex"))]
        {
            false
        }
    }
}
