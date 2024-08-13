use super::{
    data::{Account, Balance, Order, OrderId, OrderRequest, Pair},
    error::KolzoError,
    Exchange,
};
use std::fmt::Debug;

/// Trait that defines the kolzo's interactions with the exchange
///
/// Each kolzo is expected to implement the `KolzoClient` trait to provide
/// an abstraction over the exchange API.
#[async_trait::async_trait]
pub trait KolzoClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
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

    /// Fetches and returns the list of balances
    ///
    /// If the exchange supports streaming of balance updates this method can return
    /// list of balances it currently has in memory. Otherwise the kolzo should be
    /// implemented in to fetch the current balances from the exchange.
    async fn fetch_balances(&self) -> Result<Vec<Balance>, KolzoError> {
        unimplemented!()
    }

    /// Fetches and returns the account information
    ///
    /// If the exchange supports fetching of account information this method can return
    /// full margin information (positions, collaterals, etc.).
    async fn fetch_account_information(&self) -> Result<Option<Account>, KolzoError> {
        Ok(None)
    }

    /// Creates a single order
    async fn open_order(&self, req: OrderRequest) -> Result<Order, KolzoError>;

    /// Cancels a single order by pair & order_id
    ///
    /// When exchange requires an additional parameters to cancel the order (for example
    /// order side), the kolzo should take care of it using some form of open order
    /// cache.
    ///
    /// If the exchange rejects the cancel due to non-existent order,
    /// order already being completed or cancelled, the kolzo is
    /// expected to return appropriate [`kolzoError`] ([`kolzoError::OrderNotFound`],
    /// [`kolzoError::OrderAlreadyFilled`] and [`kolzoError::OrderAlreadyCancelled`] respectively).
    async fn cancel_order_by_id(
        &self,
        pair: &InstrumentId,
        order_id: OrderId,
    ) -> Result<(), KolzoError>;

    /// Cancels all orders for a given pair
    ///
    /// Exchanges usually provide an endpoint for cancelling all orders on a given market.
    /// If they provide only individual cancel endpoint, the kolzo is expected to implement
    /// custom logic to cancel all orders on a given pair.
    ///
    /// The return value of this method can be an empty vec in case the kolzo guarantees
    /// to call [`kolzoNotification::CancelOrder`] for each order that has been cancelled.
    /// Otherwise the return value is expected to contain a vector of IDs of the cancelled
    /// orders.
    ///
    /// TODO: we want to change signature to return `kolzoResult<Option<Vec<OrderId>>>` for APIs
    /// which don't support returning the cancelled order ids.
    async fn cancel_all(&self, pair: &Pair) -> Result<Vec<OrderId>, KolzoError>;
}
