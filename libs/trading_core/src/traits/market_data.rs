//! Market data client trait
use crate::error::EngineError;
use crate::types::common::{Exchange, Pair};
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
}
