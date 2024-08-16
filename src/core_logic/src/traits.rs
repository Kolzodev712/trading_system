use crate::data::{OrderBook, Trade};

use super::{
    data::{Order, OrderId, OrderRequest, Pair},
    error::KolzoError,
    Exchange,
};
use std::fmt::Debug;

/// Trait that defines the TradingClient interactions with the exchange
///
/// Each Bot is expected to implement the `TradingClient` trait to provide
/// an abstraction over the exchange APIs.
#[async_trait::async_trait]
pub trait TradingClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + 'static,
{
    /// Keyrock universal identifier of the exchange this client serves
    const EXCHANGE_ID: Exchange;

    /// Creates a single order
    async fn open_order(&self, req: OrderRequest) -> Result<Order, KolzoError>;

    /// Cancels a single order by pair & order_id
    ///
    /// When exchange requires an additional parameters to cancel the order (for example
    /// order side), the client should take care of it using some form of open order
    /// cache.
    ///
    /// If the exchange rejects the cancel due to non-existent order,
    /// order already being completed or cancelled, the kolzo is
    /// expected to return appropriate [`KolzoError`] ([`KolzoError::OrderNotFound`],
    /// [`KolzoError::OrderAlreadyFilled`] and [`KolzoError::OrderAlreadyCancelled`] respectively).
    async fn cancel_order_by_id(
        &self,
        pair: &InstrumentId,
        order_id: OrderId,
    ) -> Result<(), KolzoError>;
}

/// Trait that defines the MarketDataClient interactions with the exchange
///
/// Each Bot is expected to implement the `MarketDataClient` trait to provide
/// an abstraction over the exchange APIs.
#[async_trait::async_trait]
pub trait MarketDataClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + 'static,
{
    async fn orderbook_snapshot(&self, _pair: InstrumentId) -> Result<Vec<OrderBook>, KolzoError> {
        unimplemented!()
    }

    async fn trade_snapshot(&self, _pair: InstrumentId) -> Result<Vec<Trade>, KolzoError> {
        unimplemented!()
    }
}
