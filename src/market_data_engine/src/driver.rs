use crate::exchanges::binance::{client::BinanceClient, config::BinanceConfig};
use core_logic::{error::EngineError, Exchange};

use super::config::MarketDataConfig;

use tracing::error;

#[derive(Debug, Clone)]
pub enum Method {
    Orderbooks,
    Trades,
}

#[derive(Debug, Clone)]
pub struct MarketDataDriver {
    exchange: Exchange,
    config: MarketDataConfig,
}

impl MarketDataDriver {
    pub fn new(exchange: Exchange, config: MarketDataConfig) -> Self {
        Self { exchange, config }
    }

    pub async fn run(&self, method: Method) -> Result<(), EngineError> {
        match self.exchange {
            Exchange::Binance => {
                let binance_config = self.config.clone().get_exchange_config::<BinanceConfig>()?;
                let mut client = BinanceClient::new(binance_config, 100);

                match method {
                    Method::Orderbooks => {
                        if let Err(e) = client.orderbook().await {
                            error!("Error in orderbook stream: {}", e);
                        }
                    }
                    Method::Trades => {
                        if let Err(e) = client.trades().await {
                            error!("Error in orderbook stream: {}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
