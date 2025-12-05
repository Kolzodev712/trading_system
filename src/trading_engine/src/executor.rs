use super::strategy::Strategy;
use core_logic::{
    data::{base_data::Pair, private_data::Order},
    error::EngineError,
    redis::{RedisMessage, ORDERBOOK_CHANNEL},
    traits::TradingClient,
};
use exchange_engine::exchanges::binance::websocket::types::user_data::execution_report::ExecutionReport;
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

/// Executes a trading strategy by:
/// 1. Consuming market data from Redis
/// 2. Calling strategy.on_orderbook() for each update
/// 3. Placing orders through the trading client
/// 4. Handling execution reports
pub struct StrategyExecutor<S: Strategy> {
    strategy: Arc<RwLock<S>>,
    trading_client: Arc<RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>>,
    redis_url: String,
    symbol: Pair,
}

impl<S: Strategy> StrategyExecutor<S> {
    pub fn new(
        strategy: Arc<RwLock<S>>,
        trading_client: Arc<
            RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
        >,
        redis_url: String,
        symbol: Pair,
    ) -> Self {
        Self {
            strategy,
            trading_client,
            redis_url,
            symbol,
        }
    }

    /// Main execution loop - consumes orderbooks from Redis and executes strategy
    pub async fn run(&self) -> Result<(), EngineError> {
        info!(
            "Starting strategy executor for {symbol}",
            symbol = self.symbol
        );

        let strategy = Arc::clone(&self.strategy);
        let trading_client = Arc::clone(&self.trading_client);
        let symbol = self.symbol.clone();

        // Subscribe to orderbook channel
        let mut pubsub_conn = redis::Client::open(&self.redis_url)?
            .get_async_pubsub()
            .await?;
        pubsub_conn.subscribe(ORDERBOOK_CHANNEL).await?;

        let mut stream = pubsub_conn.on_message();
        while let Some(message) = stream.next().await {
            let payload: String = message
                .get_payload()
                .map_err(|e| EngineError::CustomError(anyhow::anyhow!("Redis error: {e}")))?;

            match serde_json::from_str::<RedisMessage>(&payload) {
                Ok(RedisMessage::OrderbookMessage(orderbook)) => {
                    // Get strategy decision
                    let order_request = {
                        let mut strat = strategy.write().await;
                        let state = strat.get_state();
                        strat.on_orderbook(&orderbook, state).await?
                    };

                    // If strategy wants to place an order, do it
                    if let Some(req) = order_request {
                        // Check active orders limit
                        let active_count = {
                            let strat = strategy.read().await;
                            strat.get_state().get_active_order_count(&symbol)
                        };

                        if active_count >= 5 {
                            warn!("Too many active orders, skipping");
                            continue;
                        }

                        // Place the order
                        match trading_client.write().await.open_order(req.clone()).await {
                            Ok(order) => {
                                info!("Order placed: {order:?}");
                                // Add to active orders
                                let mut strat = strategy.write().await;
                                strat.get_state_mut().add_order(order);
                            }
                            Err(e) => {
                                error!("Failed to place order: {e}");
                            }
                        }
                    }
                }
                Ok(RedisMessage::TradeMessage(_)) => {
                    // Ignore trades for now
                }
                Err(e) => {
                    warn!("Failed to parse Redis message: {e}");
                }
            }
        }

        Ok(())
    }

    /// Handle execution report from exchange
    pub async fn handle_execution_report(
        &self,
        report: &ExecutionReport,
    ) -> Result<(), EngineError> {
        let order: Order = report.clone().into();

        let mut strategy = self.strategy.write().await;
        strategy
            .on_execution_report(&order, strategy.get_state_mut())
            .await?;

        Ok(())
    }
}

use tokio_stream::StreamExt;
