use super::config::{ExchangeConfig, ExchangeEngineConfig};
use crate::exchanges::binance::{client::BinanceClient, rest::client::BinanceHttpClient};
use core_logic::{
    data::{
        base_data::{OrderId, Pair},
        private_data::OrderRequest,
    },
    error::EngineError,
    traits::TradingClient,
};
use tracing::debug;

#[derive(Debug, Clone)]
pub enum Method {
    TestOpenOrder(OrderRequest),
    OpenOrder(OrderRequest),
    CancelOrderById(OrderId),
    CancelAllOrders(Pair),
}

pub struct ExchangeEngineDriver {
    exchange_client: Box<dyn TradingClient<OrderRequest>>,
}

impl ExchangeEngineDriver {
    pub fn new(config: ExchangeEngineConfig) -> Result<Self, EngineError> {
        let exchange_client = Self::create_exchange_client(&config)?;
        Ok(Self { exchange_client })
    }

    pub fn create_exchange_client(
        config: &ExchangeEngineConfig,
    ) -> Result<Box<dyn TradingClient<OrderRequest>>, EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let http_client = BinanceHttpClient::new(binance_config.clone());
                Ok(Box::new(BinanceClient::new(
                    http_client,
                    binance_config.clone(),
                )))
            } // _ => Err(EngineError::UnsupportedExchange)
        }
    }

    pub async fn run(&self, method: Method) -> Result<(), EngineError> {
        match method {
            Method::TestOpenOrder(order_request) => {
                self.exchange_client.test_open_order(order_request).await?;
                debug!("Test order successful.");
            }
            Method::OpenOrder(order_request) => {
                let order = self.exchange_client.open_order(order_request).await?;
                debug!("Order placed successfully: {:?}", order);
            }
            Method::CancelOrderById(order_id) => {
                self.exchange_client.cancel_order_by_id(order_id).await?;
                debug!("Order canceled successfully.");
            }
            Method::CancelAllOrders(pair) => {
                self.exchange_client.cancel_all_orders(&pair).await?;
                debug!("All orders canceled successfully.");
            }
        }
        Ok(())
    }
}
