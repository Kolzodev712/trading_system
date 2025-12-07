//! Trading client trait
use crate::error::EngineError;
use crate::types::common::{Exchange, OrderId, Pair};
use crate::types::exchange::{Balance, Order, OrderRequest, Trade};
use async_trait::async_trait;
use std::fmt::Debug;

/// Trait that defines the TradingClient interactions with the exchange
///
/// Each exchange implementation should implement the `TradingClient` trait to provide
/// an abstraction over the exchange APIs.
///
/// Uses generics to allow exchange-specific OrderRequest types while maintaining type safety.
#[async_trait]
pub trait TradingClient<OrderRequestType = OrderRequest, InstrumentId = Pair>:
    Debug + Send + Sync + 'static
where
    OrderRequestType: Send + Sync + 'static,
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    /// Universal identifier of the exchange this client serves
    fn exchange_id(&self) -> Exchange;

    /// Paper test an open order request if applicable on the given exchange.
    async fn test_open_order(&self, req: OrderRequestType) -> Result<(), EngineError>;

    /// Creates a single order on the exchange.
    async fn open_order(&self, req: OrderRequestType) -> Result<Order, EngineError>;

    /// Cancels a single order identified by order ID.
    async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError>;

    /// Cancels all open orders for the specified trading pair.
    async fn cancel_all_orders(&self, pair: &InstrumentId) -> Result<(), EngineError>;

    /// Fetches the balances for the account.
    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError>;

    /// Fetches all open orders for the account.
    /// This method should be called at bootstrap and periodically for reconciliation.
    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError>;

    /// Fetches private trade history for the account.
    /// This method should be called at bootstrap and periodically for reconciliation.
    /// Optionally filters by trading pair if provided.
    async fn fetch_trades(&self, pair: Option<&InstrumentId>) -> Result<Vec<Trade>, EngineError>;

    /// Fetches asset market data (symbol info, trading rules, price filters, lot sizes, etc.)
    /// for a trading pair. This data is used to simplify level configuration.
    /// This method should be called at bootstrap and periodically for reconciliation.
    /// Returns asset/symbol data that can be used for level configuration.
    ///
    /// Note: This is asset/symbol data (trading rules, filters, etc.), NOT orderbook/trade data.
    /// Market data engine handles orderbooks and public trades via WebSocket.
    ///
    /// TODO: Return type will be ExchangeMarketData (to be defined in Proposal 08)
    /// For now, returns () as placeholder until ExchangeMarketData type is created.
    async fn fetch_market_data_snapshot(&self, pair: &InstrumentId) -> Result<(), EngineError>;
}
