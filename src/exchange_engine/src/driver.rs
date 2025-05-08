use super::config::{ExchangeConfig, ExchangeEngineConfig};
use crate::exchanges::binance::{
    rest::client::BinanceHttpClient, rest_client::BinanceClient as BinanceRestClient,
    ws_client::BinanceClient as BinanceWSClient,
};
use core_logic::{
    data::{
        base_data::{OrderId, Pair},
        private_data::OrderRequest,
    },
    error::EngineError,
    traits::TradingClient,
};
use tracing::{debug, error};

#[derive(Debug, Clone)]
pub enum Method {
    TestOpenOrder(OrderRequest),
    OpenOrder(OrderRequest),
    CancelOrderById(OrderId),
    CancelAllOrders(Pair),
    FetchBalance,
}

pub struct ExchangeEngineDriver {
    rest_trading_client: Box<dyn TradingClient<OrderRequest>>,
    private_data_client: BinanceWSClient,
}

impl ExchangeEngineDriver {
    pub fn new(config: ExchangeEngineConfig) -> Result<Self, EngineError> {
        let (rest_trading_client, private_data_client) = Self::create_clients(&config)?;
        Ok(Self {
            rest_trading_client,
            private_data_client,
        })
    }

    pub fn create_clients(
        config: &ExchangeEngineConfig,
    ) -> Result<(Box<dyn TradingClient<OrderRequest>>, BinanceWSClient), EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let rest_client = BinanceRestClient::new(
                    BinanceHttpClient::new(binance_config.clone()),
                    binance_config.clone(),
                );

                let ws_client = BinanceWSClient::new(binance_config.clone(), 1024);

                Ok((Box::new(rest_client), ws_client))
            } // _ => Err(EngineError::CustomError(anyhow::anyhow!("Unsupported exchange"))),
        }
    }

    pub async fn run(&self, method: Method) -> Result<(), EngineError> {
        let mut ws_client = self.private_data_client.clone();
        let ws_task = tokio::spawn(async move {
            match ws_client.stream().await {
                Ok(_) => {
                    debug!("WebSocket loop successfully running");
                }
                Err(e) => {
                    error!("WebSocket client failed: {}", e);
                }
            }
        });

        tokio::select! {
            res = async {
                match method {
                    Method::TestOpenOrder(order_request) => {
                        self.rest_trading_client.test_open_order(order_request).await?;
                        debug!("Test order successful.");
                    }
                    Method::OpenOrder(order_request) => {
                        let order = self.rest_trading_client.open_order(order_request).await?;
                        debug!("Order placed successfully: {:?}", order);
                    }
                    Method::CancelOrderById(order_id) => {
                        self.rest_trading_client.cancel_order_by_id(order_id).await?;
                        debug!("Order canceled successfully.");
                    }
                    Method::CancelAllOrders(pair) => {
                        self.rest_trading_client.cancel_all_orders(&pair).await?;
                        debug!("All orders canceled successfully.");
                    }
                    Method::FetchBalance => {
                        self.rest_trading_client.fetch_balance().await?;
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
