//! Channel types for market data updates
use crate::types::market_data::StructuredOrderbook as Orderbook;
use crate::types::market_data::StructuredPublicTrade as PublicTrade;
use tokio::sync::mpsc;

/// Market data update message
#[derive(Debug, Clone)]
pub struct MarketDataUpdate {
    /// Updated orderbooks
    pub orderbooks: Vec<Orderbook>,
    /// New public trades
    pub trades: Vec<PublicTrade>,
}

/// Market data update channel sender
pub type MarketDataUpdateSender = mpsc::UnboundedSender<MarketDataUpdate>;

/// Market data update channel receiver
pub type MarketDataUpdateReceiver = mpsc::UnboundedReceiver<MarketDataUpdate>;

/// Create a new market data update channel pair
pub fn market_data_update_channel() -> (MarketDataUpdateSender, MarketDataUpdateReceiver) {
    mpsc::unbounded_channel()
}
