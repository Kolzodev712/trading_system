use super::{
    data::{Order, OrderId, OrderRequest, Pair},
    error::KolzoError,
    Exchange,
};
use std::fmt::Debug;

/// Trait that defines the kolzo's interactions with the exchange
///
/// Each kolzo is expected to implement the `TradingClient` trait to provide
/// an abstraction over the exchange API.
#[async_trait::async_trait]
pub trait TradingClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + 'static,
{
    /// Keyrock universal identifier of the exchange this client serves
    const EXCHANGE_ID: Exchange;

    /// Boolean constant that determines whether the kolzo client supports pre-condition validation for open order requests
    const PRECONDITION_CHECKED: bool = false;

    /// If the exchange supports reliable streaming connection (eg. one that is streaming
    /// order updates with sequence numbering, for example FIX) this method can return
    /// list of open orders it currently has in memory. Otherwise the kolzo should be
    /// implemented in to fetch the orders from the exchange.
    async fn fetch_open_orders(&self) -> Result<Vec<Order>, KolzoError>;

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
