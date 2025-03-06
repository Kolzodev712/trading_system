use crate::data::public_data::{Orderbook, Trade};

use super::{
    data::{
        base_data::{OrderId, Pair},
        private_data::Order,
    },
    error::EngineError,
    Exchange,
};
use std::fmt::Debug;

/// Trait that defines the TradingClient interactions with the exchange
///
/// Each Bot is expected to implement the `TradingClient` trait to provide
/// an abstraction over the exchange APIs.
#[async_trait::async_trait]
pub trait TradingClient<OrderRequest, InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    OrderRequest: Send + Sync + 'static,
    InstrumentId: Clone + Debug + Send + 'static,
{
    /// Universal identifier of the exchange this client serves
    fn exchange_id(&self) -> Exchange;

    /// Paper test an oper order request if applicable on the given exchange.
    ///
    /// # Parameters
    /// - `req`: The order request (`OrderRequest`) containing order details.
    ///
    /// # Returns
    /// - `Ok(Order)`: The successfully created order.
    /// - `Err(EngineError)`: If the order creation fails due to validation or exchange errors.
    async fn test_open_order(&self, req: OrderRequest) -> Result<(), EngineError>;

    /// Creates a single order on the exchange.
    ///
    /// # Parameters
    /// - `req`: The order request (`OrderRequest`) containing order details.
    ///
    /// # Returns
    /// - `Ok(Order)`: The successfully created order.
    /// - `Err(EngineError)`: If the order creation fails due to validation or exchange errors.
    async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError>;

    /// Cancels a single order identified by trading pair and order ID.
    ///
    /// # Parameters
    /// - `order_id`: The unique ID of the order to cancel.
    ///
    /// # Returns
    /// - `Ok(())`: If the order is successfully canceled.
    /// - `Err(EngineError)`: If the cancellation fails due to an invalid order or exchange error.
    ///
    /// # Notes
    /// - Errors may include `OrderNotFound`, `OrderAlreadyFilled`, or `OrderAlreadyCancelled`.
    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError>;

    /// Cancels all open orders for the specified trading pair.
    ///
    /// # Parameters
    /// - `pair`: The trading pair (`InstrumentId`) for which all orders should be canceled.
    ///
    /// # Returns
    /// - `Ok(())`: If all orders are successfully canceled or there are no open orders.
    /// - `Err(EngineError)`: If the cancellation fails due to an exchange or network error.
    ///
    /// # Notes
    /// - May return successfully even if no orders exist for the given pair.
    async fn cancel_all_orders(&self, pair: &InstrumentId) -> Result<(), EngineError>;
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
    /// Universal identifier of the exchange this client serves
    fn exchange_id(&self) -> Exchange;

    async fn orderbooks(&mut self) -> Result<(), EngineError>;

    async fn trades(&mut self) -> Result<(), EngineError>;

    async fn orderbook_snapshot(&self, _pair: InstrumentId) -> Result<Orderbook, EngineError> {
        unimplemented!()
    }
    async fn trade_snapshot(&self, _pair: InstrumentId) -> Result<Trade, EngineError> {
        unimplemented!()
    }
}
