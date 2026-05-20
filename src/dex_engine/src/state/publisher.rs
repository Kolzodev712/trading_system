use tracing::warn;
use trading_core::venue::dex::channels::{DexVenueUpdate, DexVenueUpdateSender};
use trading_core::venue::dex::DexError;

#[derive(Debug, Clone)]
pub struct DexStatePublisher {
    tx: DexVenueUpdateSender,
}

impl DexStatePublisher {
    pub fn new(tx: DexVenueUpdateSender) -> Self {
        Self { tx }
    }

    pub fn publish(&self, update: DexVenueUpdate) -> Result<(), DexError> {
        self.tx.send(update).map(|_| ()).map_err(|e| {
            warn!("DEX venue update channel send failed: {e}");
            DexError::ChannelSend(e.to_string().into())
        })
    }
}
