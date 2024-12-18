use crate::exchanges::binance::{
    client::BinanceClient, config::BinanceConfig, rest::client::BinanceHttpClient,
    types::open_order::BinanceOrderRequest,
};
use core_logic::{error::EngineError, Exchange};

use super::config::ExchangeEngineConfig;

use tracing::error;

#[derive(Debug, Clone)]
pub enum Method {
    OpenOrder(BinanceOrderRequest),
    CancelOrderById,
    CancelAllOrders,
}

#[derive(Debug, Clone)]
pub struct ExchangeEngineDriver {
    exchange: Exchange,
    config: ExchangeEngineConfig,
}

impl ExchangeEngineDriver {
    pub fn new(exchange: Exchange, config: ExchangeEngineConfig) -> Self {
        Self { exchange, config }
    }

    pub async fn run(&self, method: Method) -> Result<(), EngineError> {
        match self.exchange {
            Exchange::Binance => {
                let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
                let http_client = BinanceHttpClient::new(binance_config.clone());
                let client = BinanceClient::new(http_client, binance_config.clone());

                match method {
                    Method::OpenOrder(mut order_request) => {
                        let time = client.get_time().await?;
                        order_request.timestamp = time.server_time;

                        match client.open_order_test(order_request).await {
                            Ok(response) => {
                                println!("Test order placed successfully: {:?}", response);
                            }
                            Err(e) => {
                                error!("Failed to place test order: {}", e);
                            }
                        }
                    }
                    Method::CancelAllOrders => match client.cancel_all().await {
                        Ok(response) => {
                            println!("Test order placed successfully: {:?}", response);
                        }
                        Err(e) => {
                            error!("Failed to place test order: {}", e);
                        }
                    },
                    _ => {} // add other methods respectively
                }
            }
        }

        Ok(())
    }
}
