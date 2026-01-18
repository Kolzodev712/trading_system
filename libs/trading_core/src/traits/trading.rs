//! Trading client trait
use crate::error::EngineError;
use crate::types::common::{Exchange, OrderId, Pair};
use crate::types::exchange::{Balance, Order, OrderRequest, Trade};
use crate::types::futures::{
    FuturesBalance, FuturesOrderRequest, LeverageInfo, MarginType, PositionMode,
};
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

    /// Amends (modifies) an existing order by canceling it and placing a new one.
    /// Returns the new order. The original order will be marked as Amended in state.
    ///
    /// Note: Some exchanges don't support direct order amendments, so this method
    /// implements the cancel + place new order pattern.
    async fn amend_order(
        &self,
        order_id: OrderId,
        new_request: OrderRequestType,
    ) -> Result<Order, EngineError>;

    // ===== BULK OPERATION METHODS =====
    // These methods handle batching internally:
    // - Try bulk operation if supported
    // - Fall back to single operations automatically
    // - Return results preserving input order

    /// Bulk open multiple orders in a single request.
    /// Exchange implementations should try bulk first, fall back to single operations.
    ///
    /// Exchanges must implement this method. For exchanges without bulk support,
    /// implement a fallback to single operations.
    async fn bulk_open_orders(
        &self,
        req: crate::types::exchange::BulkOrderRequest,
    ) -> Result<crate::types::exchange::BulkOrderResponse, EngineError>;

    /// Bulk cancel multiple orders by their IDs.
    /// Exchange implementations should try bulk first, fall back to single operations.
    /// Default implementation falls back to single operations.
    async fn bulk_cancel_orders(
        &self,
        req: crate::types::exchange::BulkCancelRequest,
    ) -> Result<crate::types::exchange::BulkCancelResponse, EngineError> {
        // Default: fall back to single operations
        let mut results = Vec::new();
        let mut cancelled_ids = Vec::new();
        let mut failed_ids = Vec::new();

        for (idx, order_id) in req.order_ids.into_iter().enumerate() {
            match self.cancel_order_by_id(order_id.clone()).await {
                Ok(_) => {
                    results.push(crate::types::exchange::BulkOperationResult {
                        success: true,
                        data: Some(order_id.clone()),
                        error: None,
                        index: idx,
                    });
                    cancelled_ids.push(order_id);
                }
                Err(e) => {
                    let error_msg = format!("{}", e);
                    results.push(crate::types::exchange::BulkOperationResult {
                        success: false,
                        data: None,
                        error: Some(error_msg.clone()),
                        index: idx,
                    });
                    failed_ids.push((order_id, error_msg));
                }
            }
        }

        Ok(crate::types::exchange::BulkCancelResponse {
            results,
            cancelled_ids,
            failed_ids,
        })
    }

    /// Bulk amend multiple orders.
    /// Exchange implementations should try bulk first, fall back to single operations.
    ///
    /// Exchanges must implement this method. For exchanges without bulk support,
    /// implement a fallback to single operations.
    async fn bulk_amend_orders(
        &self,
        req: crate::types::exchange::BulkAmendRequest,
    ) -> Result<crate::types::exchange::BulkAmendResponse, EngineError>;

    // ===== FUTURES METHODS (Optional) =====
    // All have default implementations that return "not supported"
    // Exchanges with unified APIs (like Bybit) implement these in the same client
    // Exchanges with separate APIs (like Binance) have separate clients
    // The standard open_order() method should handle both spot and futures orders
    // based on OrderRequest.instrument_type

    /// Fetch all open futures positions
    /// Default implementation returns "not supported"
    async fn fetch_positions(&self) -> Result<Vec<crate::types::exchange::Position>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures positions not supported".into(),
        ))
    }

    /// Fetch position for a specific pair
    /// Default implementation returns "not supported"
    async fn fetch_position(
        &self,
        _pair: &InstrumentId,
    ) -> Result<Option<crate::types::exchange::Position>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures positions not supported".into(),
        ))
    }

    /// Fetch collateral balance for futures trading account
    /// Default implementation returns "not supported"
    async fn fetch_collateral_balance(&self) -> Result<Vec<FuturesBalance>, EngineError> {
        Err(EngineError::NotSupported(
            "Collateral balance not supported".into(),
        ))
    }

    /// Set leverage for a trading pair
    /// Default implementation returns "not supported"
    async fn set_leverage(
        &self,
        _pair: &InstrumentId,
        _leverage: u32,
    ) -> Result<LeverageInfo, EngineError> {
        Err(EngineError::NotSupported(
            "Leverage setting not supported".into(),
        ))
    }

    /// Get current leverage for a trading pair
    /// Default implementation returns "not supported"
    async fn get_leverage(&self, _pair: &InstrumentId) -> Result<LeverageInfo, EngineError> {
        Err(EngineError::NotSupported(
            "Leverage query not supported".into(),
        ))
    }

    /// Set position mode (one-way or hedge)
    /// Default implementation returns "not supported"
    async fn set_position_mode(&self, _mode: PositionMode) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Position mode setting not supported".into(),
        ))
    }

    /// Get current position mode
    /// Default implementation returns "not supported"
    async fn get_position_mode(&self) -> Result<PositionMode, EngineError> {
        Err(EngineError::NotSupported(
            "Position mode query not supported".into(),
        ))
    }

    /// Set margin type (isolated or cross)
    /// Default implementation returns "not supported"
    async fn set_margin_type(
        &self,
        _pair: &InstrumentId,
        _margin_type: MarginType,
    ) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Margin type setting not supported".into(),
        ))
    }

    /// Get current margin type for a pair
    /// Default implementation returns "not supported"
    async fn get_margin_type(&self, _pair: &InstrumentId) -> Result<MarginType, EngineError> {
        Err(EngineError::NotSupported(
            "Margin type query not supported".into(),
        ))
    }

    /// Place a futures order (convenience method)
    /// The standard open_order() should handle both spot and futures
    /// based on OrderRequest.instrument_type
    /// Default implementation returns "not supported"
    async fn place_futures_order(&self, _req: FuturesOrderRequest) -> Result<Order, EngineError> {
        Err(EngineError::NotSupported(
            "Futures orders not supported".into(),
        ))
    }

    /// Cancel all futures orders for a pair
    /// Default implementation returns "not supported"
    async fn cancel_all_futures_orders(&self, _pair: &InstrumentId) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Futures order cancellation not supported".into(),
        ))
    }

    /// Fetch open futures orders
    /// Default implementation returns "not supported"
    async fn fetch_open_futures_orders(
        &self,
        _pair: Option<&InstrumentId>,
    ) -> Result<Vec<Order>, EngineError> {
        Err(EngineError::NotSupported(
            "Futures orders not supported".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{InstrumentType, Pair};
    use rust_decimal::Decimal;

    /// Mock implementation to test default futures methods
    #[derive(Debug)]
    struct MockSpotOnlyClient;

    #[async_trait]
    impl TradingClient<OrderRequest> for MockSpotOnlyClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Binance
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("Mock".into()))
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_trades(&self, _pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }

        async fn amend_order(
            &self,
            _order_id: OrderId,
            _new_request: OrderRequest,
        ) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("Mock".into()))
        }

        async fn bulk_open_orders(
            &self,
            req: crate::types::exchange::BulkOrderRequest,
        ) -> Result<crate::types::exchange::BulkOrderResponse, EngineError> {
            // Fallback to single operations
            let mut results = Vec::new();
            let mut success_count = 0;
            let mut failure_count = 0;

            for (idx, order_req) in req.requests.into_iter().enumerate() {
                match self.open_order(order_req).await {
                    Ok(order) => {
                        results.push(crate::types::exchange::BulkOperationResult {
                            success: true,
                            data: Some(order),
                            error: None,
                            index: idx,
                        });
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(crate::types::exchange::BulkOperationResult {
                            success: false,
                            data: None,
                            error: Some(format!("{}", e)),
                            index: idx,
                        });
                        failure_count += 1;
                    }
                }
            }

            Ok(crate::types::exchange::BulkOrderResponse {
                results,
                success_count,
                failure_count,
            })
        }

        async fn bulk_cancel_orders(
            &self,
            req: crate::types::exchange::BulkCancelRequest,
        ) -> Result<crate::types::exchange::BulkCancelResponse, EngineError> {
            // Fallback to single operations
            let mut results = Vec::new();
            let mut cancelled_ids = Vec::new();
            let mut failed_ids = Vec::new();

            for (idx, order_id) in req.order_ids.into_iter().enumerate() {
                match self.cancel_order_by_id(order_id.clone()).await {
                    Ok(_) => {
                        results.push(crate::types::exchange::BulkOperationResult {
                            success: true,
                            data: Some(order_id.clone()),
                            error: None,
                            index: idx,
                        });
                        cancelled_ids.push(order_id);
                    }
                    Err(e) => {
                        let error_msg = format!("{}", e);
                        results.push(crate::types::exchange::BulkOperationResult {
                            success: false,
                            data: None,
                            error: Some(error_msg.clone()),
                            index: idx,
                        });
                        failed_ids.push((order_id, error_msg));
                    }
                }
            }

            Ok(crate::types::exchange::BulkCancelResponse {
                results,
                cancelled_ids,
                failed_ids,
            })
        }

        async fn bulk_amend_orders(
            &self,
            req: crate::types::exchange::BulkAmendRequest,
        ) -> Result<crate::types::exchange::BulkAmendResponse, EngineError> {
            // Fallback to single operations
            let mut results = Vec::new();
            let mut amended_orders = Vec::new();
            let mut failed_amendments = Vec::new();

            for (idx, amendment) in req.amendments.into_iter().enumerate() {
                match self
                    .amend_order(amendment.order_id.clone(), amendment.new_request)
                    .await
                {
                    Ok(order) => {
                        results.push(crate::types::exchange::BulkOperationResult {
                            success: true,
                            data: Some(order.clone()),
                            error: None,
                            index: idx,
                        });
                        amended_orders.push(order);
                    }
                    Err(e) => {
                        let error_msg = format!("{}", e);
                        results.push(crate::types::exchange::BulkOperationResult {
                            success: false,
                            data: None,
                            error: Some(error_msg.clone()),
                            index: idx,
                        });
                        failed_amendments.push((amendment.order_id, error_msg));
                    }
                }
            }

            Ok(crate::types::exchange::BulkAmendResponse {
                results,
                amended_orders,
                failed_amendments,
            })
        }
    }

    #[tokio::test]
    async fn test_default_futures_methods_return_not_supported() {
        let client = MockSpotOnlyClient;

        // Test all default futures methods return NotSupported
        assert!(matches!(
            client.fetch_positions().await,
            Err(EngineError::NotSupported(_))
        ));

        let pair = Pair::new("BTC", "USDT");
        assert!(matches!(
            client.fetch_position(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.fetch_collateral_balance().await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.set_leverage(&pair, 10).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.get_leverage(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.set_position_mode(PositionMode::OneWay).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.get_position_mode().await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.set_margin_type(&pair, MarginType::Isolated).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.get_margin_type(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        let futures_req = FuturesOrderRequest {
            base: OrderRequest {
                instrument: crate::types::common::Instrument::new(
                    InstrumentType::Futures,
                    Pair::new("BTC", "USDT"),
                ),
                side: crate::types::common::Side::Buy,
                order_type: crate::types::common::OrderType::Limit,
                price: Decimal::new(50000, 0),
                amount: Decimal::new(1, 0),
                timestamp: None,
                exchange_params: std::collections::HashMap::new(),
            },
            position_side: None,
            reduce_only: None,
            close_position: None,
            stop_price: None,
            activation_price: None,
            callback_rate: None,
            working_type: None,
            price_protect: None,
        };

        assert!(matches!(
            client.place_futures_order(futures_req).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.cancel_all_futures_orders(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.fetch_open_futures_orders(None).await,
            Err(EngineError::NotSupported(_))
        ));
    }
}
