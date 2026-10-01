//! Cancel only durable owned order IDs, then REST-verify those IDs are gone.
//! Cancel ACK, cache, timeout, and failed REST are not verification.
use trading_core::error::EngineError;
use trading_core::types::common::OrderId;
use trading_core::types::exchange::OrderRequest;
use trading_core::venue::cex::traits::TradingClient;

/// Best-effort cancel of owned IDs. Does not treat cancel ACK as proof the venue is clear.
pub async fn cancel_owned_order_ids(
    trading_client: &dyn TradingClient<OrderRequest>,
    order_ids: &[OrderId],
) -> Result<(), EngineError> {
    for order_id in order_ids {
        if let Err(err) = trading_client.cancel_order_by_id(order_id.clone()).await {
            tracing::warn!(
                event = "decision",
                reason_code = %trading_core::diagnostics::ReasonCode::CancelUnverified,
                order_id = order_id.as_str(),
                error = %err,
                "owned-order cancel ACK/error is not REST verification"
            );
        }
    }
    verify_owned_ids_absent(trading_client, order_ids).await
}

/// REST proof that none of `order_ids` remain working. Failed REST is unverified.
pub async fn verify_owned_ids_absent(
    trading_client: &dyn TradingClient<OrderRequest>,
    order_ids: &[OrderId],
) -> Result<(), EngineError> {
    if order_ids.is_empty() {
        return Ok(());
    }
    match trading_client.fetch_open_orders().await {
        Ok(orders) => {
            let still: Vec<String> = orders
                .iter()
                .filter(|order| {
                    order.is_working() && order_ids.iter().any(|id| id == &order.order_id)
                })
                .map(|order| order.order_id.as_str().to_string())
                .collect();
            if still.is_empty() {
                Ok(())
            } else {
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "owned-order cancel unverified: REST still shows {}",
                    still.join(",")
                )))
            }
        }
        Err(err) => Err(EngineError::CustomError(anyhow::anyhow!(
            "owned-order cancel unverified: REST open orders failed: {err}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rust_decimal_macros::dec;
    use std::sync::Mutex;
    use trading_core::types::common::{Exchange, Instrument, InstrumentType, Pair, Side};
    use trading_core::types::exchange::{
        Balance, BulkOrderRequest, BulkOrderResponse, Order, Trade,
    };

    #[derive(Debug)]
    struct RestClient {
        open: Mutex<Result<Vec<Order>, String>>,
        cancelled: Mutex<Vec<String>>,
    }

    impl RestClient {
        fn ok(orders: Vec<Order>) -> Self {
            Self {
                open: Mutex::new(Ok(orders)),
                cancelled: Mutex::new(vec![]),
            }
        }

        fn rest_fail() -> Self {
            Self {
                open: Mutex::new(Err("timeout".into())),
                cancelled: Mutex::new(vec![]),
            }
        }
    }

    fn working(id: &str) -> Order {
        Order {
            order_id: OrderId::new(id.into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
            side: Side::Buy,
            price: dec!(50000),
            amount: dec!(0.001),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: trading_core::types::common::OrderStatus::New,
            filled_amount: dec!(0),
        }
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for RestClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            panic!("owned cancel must not place");
        }
        async fn cancel_order_by_id(&self, order_id: OrderId) -> Result<(), EngineError> {
            self.cancelled
                .lock()
                .unwrap()
                .push(order_id.as_str().to_string());
            Ok(())
        }
        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            panic!("must not cancel_all");
        }
        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }
        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            match &*self.open.lock().unwrap() {
                Ok(orders) => Ok(orders.clone()),
                Err(err) => Err(EngineError::CustomError(anyhow::anyhow!("{err}"))),
            }
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
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_open_orders(
            &self,
            _req: BulkOrderRequest,
        ) -> Result<BulkOrderResponse, EngineError> {
            panic!("no bulk")
        }
        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    #[tokio::test]
    async fn rest_failure_is_not_verification() {
        let client = RestClient::rest_fail();
        let err = verify_owned_ids_absent(&client, &[OrderId::new("ex-1".into())])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unverified"));
        assert!(client.cancelled.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn still_working_after_ack_is_unverified() {
        let client = RestClient::ok(vec![working("ex-1")]);
        let err = cancel_owned_order_ids(&client, &[OrderId::new("ex-1".into())])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unverified"));
        assert_eq!(client.cancelled.lock().unwrap().as_slice(), ["ex-1"]);
    }

    #[tokio::test]
    async fn sibling_working_order_is_ignored() {
        let client = RestClient::ok(vec![working("other")]);
        cancel_owned_order_ids(&client, &[OrderId::new("ex-1".into())])
            .await
            .unwrap();
    }
}
