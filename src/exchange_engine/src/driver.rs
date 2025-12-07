use super::config::{ExchangeConfig, ExchangeEngineConfig};
use crate::exchanges::binance::ws_client::BinanceClient as BinanceWSClient;
use tracing::{debug, error};
use trading_core::{
    error::EngineError,
    traits::TradingClient,
    types::{
        common::{OrderId, Pair},
        exchange::OrderRequest,
    },
};

#[derive(Debug, Clone)]
pub enum Method {
    TestOpenOrder(OrderRequest),
    OpenOrder(OrderRequest),
    CancelOrderById(OrderId),
    CancelAllOrders(Pair),
    FetchBalance,
}

pub struct ExchangeEngineDriver {
    ws_client: BinanceWSClient,
}

impl ExchangeEngineDriver {
    pub fn new(config: ExchangeEngineConfig) -> Result<Self, EngineError> {
        let ws_client = Self::create_clients(&config)?;
        Ok(Self { ws_client })
    }

    pub fn create_clients(config: &ExchangeEngineConfig) -> Result<BinanceWSClient, EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let ws_client = BinanceWSClient::new(binance_config.clone(), 1024);
                Ok(ws_client)
            } // _ => Err(EngineError::CustomError(anyhow::anyhow!("Unsupported exchange"))),
        }
    }

    pub async fn run(&self, method: Method) -> Result<(), EngineError> {
        let mut ws_client = self.ws_client.clone();
        let ws_task = tokio::spawn(async move {
            match ws_client.stream().await {
                Ok(_) => {
                    debug!("WebSocket loop successfully running");
                }
                Err(e) => {
                    error!("WebSocket client failed: {e}");
                }
            }
        });

        tokio::select! {
            res = async {
                match method {
                    Method::TestOpenOrder(order_request) => {
                        self.ws_client.test_open_order(order_request).await?;
                        debug!("Test order successful.");
                    }
                    Method::OpenOrder(order_request) => {
                        let order = self.ws_client.open_order(order_request).await?;
                        debug!("Order placed successfully: {:?}", order);
                    }
                    Method::CancelOrderById(order_id) => {
                        self.ws_client.cancel_order_by_id(order_id).await?;
                        debug!("Order canceled successfully.");
                    }
                    Method::CancelAllOrders(pair) => {
                        self.ws_client.cancel_all_orders(&pair).await?;
                        debug!("All orders canceled successfully.");
                    }
                    Method::FetchBalance => {
                        self.ws_client.fetch_balance().await?;
                        debug!("Balances successfully fetched.");
                    }
                }
                Ok::<(), EngineError>(())
            } => {
                res?;
            }
        };

        ws_task.await?;

        Ok(())
    }
}
