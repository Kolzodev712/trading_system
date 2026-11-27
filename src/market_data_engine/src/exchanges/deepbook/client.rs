//! DeepBook indexer polling client for market data (exchange: DeepBook, chain: Sui).

use std::time::Duration;

use async_trait::async_trait;
use dex_engine::DeepBookIndexerClient;
use tracing::{error, info, warn};
use trading_core::error::EngineError;
use trading_core::types::common::Exchange;
use trading_core::venue::cex::traits::MarketDataClient;

use crate::messages::WSMessageHandler;

use super::config::DeepBookMarketDataConfig;
use super::map::{orderbook_to_structured, trade_to_structured};

#[derive(Debug)]
pub struct DeepBookMarketDataClient {
    config: DeepBookMarketDataConfig,
    indexer: DeepBookIndexerClient,
    message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
}

impl DeepBookMarketDataClient {
    pub fn new(
        config: DeepBookMarketDataConfig,
        message_handler: Option<Box<dyn WSMessageHandler + Send + Sync>>,
    ) -> Result<Self, EngineError> {
        let indexer = DeepBookIndexerClient::new(config.indexer_url.clone()).map_err(|e| {
            EngineError::CustomError(smol_str::format_smolstr!("DeepBook indexer client: {e}"))
        })?;
        Ok(Self {
            config,
            indexer,
            message_handler,
        })
    }

    async fn poll_once(&self) -> Result<(), EngineError> {
        let handler = self
            .message_handler
            .as_ref()
            .ok_or(EngineError::NoMessageHandler)?;

        for pool in &self.config.pools {
            let pool_name = pool.as_str();
            match self
                .indexer
                .get_orderbook(pool_name, self.config.orderbook_depth)
                .await
            {
                Ok(snapshot) => {
                    let ob = orderbook_to_structured(pool_name, &snapshot);
                    handler.handle_orderbook(ob).await?;
                }
                Err(e) => {
                    warn!("DeepBook orderbook poll failed for {pool_name}: {e}");
                }
            }

            match self
                .indexer
                .get_trades(pool_name, self.config.trades_limit)
                .await
            {
                Ok(trades) => {
                    for trade in trades {
                        if let Some(structured) = trade_to_structured(pool_name, &trade) {
                            handler.handle_trade(structured).await?;
                        }
                    }
                }
                Err(e) => {
                    warn!("DeepBook trades poll failed for {pool_name}: {e}");
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MarketDataClient for DeepBookMarketDataClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::DeepBook
    }

    async fn orderbooks(&mut self) -> Result<(), EngineError> {
        let interval = Duration::from_millis(self.config.poll_interval_ms);
        info!(
            "DeepBook (Sui) market data: polling {} pool(s) every {:?}",
            self.config.pools.len(),
            interval
        );
        loop {
            if let Err(e) = self.poll_once().await {
                error!("DeepBook market data poll error: {e}");
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn trades(&mut self) -> Result<(), EngineError> {
        Ok(())
    }
}
