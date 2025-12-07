//! Market data client trait
use crate::error::EngineError;
use crate::types::common::{Exchange, Pair};
use crate::types::market_data::{LegacyOrderbook as Orderbook, LegacyTrade as PublicTrade};
use async_trait::async_trait;
use std::fmt::Debug;

/// Trait that defines the MarketDataClient interactions with the exchange
///
/// Each exchange implementation should implement the `MarketDataClient` trait to provide
/// an abstraction over the exchange market data APIs.
#[async_trait]
pub trait MarketDataClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    /// Universal identifier of the exchange this client serves
    fn exchange_id(&self) -> Exchange;

    /// Start streaming orderbooks
    async fn orderbooks(&mut self) -> Result<(), EngineError>;

    /// Start streaming trades
    async fn trades(&mut self) -> Result<(), EngineError>;

    /// Fetch orderbook snapshot for a pair
    async fn orderbook_snapshot(&self, pair: InstrumentId) -> Result<Orderbook, EngineError>;

    /// Fetch trade snapshot for a pair
    async fn trade_snapshot(&self, pair: InstrumentId) -> Result<PublicTrade, EngineError>;

    /// Fetches market data snapshot (orderbook and recent trades) for a trading pair.
    /// This method should be called at bootstrap and periodically for reconciliation.
    /// Returns both orderbook and recent trades in a single call for efficiency.
    async fn fetch_market_data_snapshot(
        &self,
        pair: InstrumentId,
    ) -> Result<(Orderbook, Vec<PublicTrade>), EngineError>;
}
