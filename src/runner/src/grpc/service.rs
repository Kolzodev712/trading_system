//! gRPC Service Implementation
use async_trait::async_trait;
use exchange_engine::state::manager::ExchangeStateManager;
use indicator_engine::IndicatorService;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

// Import the generated protobuf code
use crate::grpc::trading_cli;
use crate::grpc::trading_cli::trading_cli_service_server::TradingCliService;

/// Trading CLI Service Implementation
pub struct TradingCliServiceImpl {
    #[allow(dead_code)] // Will be used in Phase 2-5 for level management
    level_manager: Arc<RwLock<LevelManager>>,
    #[allow(dead_code)] // Will be used in Phase 2-5 for level management
    trading_state_manager: Arc<TradingStateManager>,
    trading_control: Arc<TradingControl>,
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    exchange_state_manager: Arc<ExchangeStateManager>,
    indicator_service: Arc<Mutex<IndicatorService>>,
    // Store calculator directly - it's thread-safe, no locking needed for reads
    indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
}

impl TradingCliServiceImpl {
    pub fn new(
        level_manager: Arc<RwLock<LevelManager>>,
        trading_state_manager: Arc<TradingStateManager>,
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        exchange_state_manager: Arc<ExchangeStateManager>,
        indicator_service: Arc<Mutex<IndicatorService>>,
        indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
        trading_control: Arc<TradingControl>,
    ) -> Self {
        Self {
            level_manager,
            trading_state_manager,
            trading_control,
            trading_client,
            exchange_state_manager,
            indicator_service,
            indicator_calculator,
        }
    }

    /// Clone self for use in streaming tasks
    fn clone_for_streaming(&self) -> Self {
        Self {
            level_manager: self.level_manager.clone(),
            trading_state_manager: self.trading_state_manager.clone(),
            trading_control: self.trading_control.clone(),
            trading_client: self.trading_client.clone(),
            exchange_state_manager: self.exchange_state_manager.clone(),
            indicator_service: self.indicator_service.clone(),
            indicator_calculator: self.indicator_calculator.clone(),
        }
    }

    /// Collect monitoring data from all sources
    async fn collect_monitoring_data(&self) -> Result<trading_cli::MonitoringData, Status> {
        let start = std::time::Instant::now();
        // Collect all data in parallel to avoid sequential blocking
        let ((orders, balances), positions, indicators, trades) = tokio::join!(
            // Get orders and balances from ExchangeStateManager
            async {
                let exchange_state_arc = self.exchange_state_manager.get_state();
                let exchange_state = exchange_state_arc.read().await;

                // Build order_id -> level_id mapping from LevelManager
                let level_manager = self.level_manager.read().await;
                let mut order_to_level: HashMap<String, (String, String)> = HashMap::new();

                for level in level_manager.get_all_levels() {
                    let level_id = level.config.id.to_string();
                    let order_type_str = match level.config.order_type {
                        trading_engine::levels::config::OrderTypeConfig::Limit => "Limit",
                        trading_engine::levels::config::OrderTypeConfig::Market => "Market",
                        trading_engine::levels::config::OrderTypeConfig::PostOnly => "PostOnly",
                        trading_engine::levels::config::OrderTypeConfig::IoC => "IoC",
                        trading_engine::levels::config::OrderTypeConfig::FoK => "FoK",
                    };

                    for order_id in &level.active_orders {
                        order_to_level.insert(
                            order_id.as_str().to_string(),
                            (level_id.clone(), order_type_str.to_string()),
                        );
                    }
                }
                drop(level_manager); // Release lock before continuing

                // Get open orders (active orders from exchange)
                let orders: Vec<trading_cli::Order> = exchange_state
                    .get_open_orders()
                    .iter()
                    .map(|order| {
                        let order_id_str = order.order_id.as_str().to_string();
                        let (level_id, order_type) = order_to_level
                            .get(&order_id_str)
                            .cloned()
                            .unwrap_or_else(|| (String::new(), "Limit".to_string())); // Default to Limit if not found

                        trading_cli::Order {
                            order_id: order_id_str,
                            symbol: order.symbol.to_string(),
                            side: format!("{:?}", order.side),
                            order_type,
                            price: order.price.to_string(),
                            quantity: order.amount.to_string(),
                            status: format!("{:?}", order.status),
                            level_id,
                            timestamp: order.internal_created_at as i64,
                        }
                    })
                    .collect();

                // Get balances (from WebSocket updates)
                let balances: Vec<trading_cli::Balance> = exchange_state
                    .get_balances()
                    .iter()
                    .map(|b| trading_cli::Balance {
                        asset: b.symbol.to_string(),
                        free: b.free.to_string(),
                        used: b.used.to_string(),
                        locked: b.used.to_string(), // used is the locked amount
                        total: b.total.to_string(),
                    })
                    .collect();

                (orders, balances)
            },
            // Get positions from TradingStateManager
            async {
                let trading_state_arc = self.trading_state_manager.get_state();
                let trading_state = trading_state_arc.read().await;
                let positions: Vec<trading_cli::Position> = trading_state
                    .positions
                    .values()
                    .map(|pos| trading_cli::Position {
                        symbol: pos.symbol.convert_to_smolstr().to_string(),
                        quantity: pos.quantity.to_string(),
                        avg_price: pos.avg_price.to_string(),
                    })
                    .collect();

                if !positions.is_empty() {
                    tracing::debug!(
                        "[gRPC] get_positions: {} position(s) found",
                        positions.len()
                    );
                }

                positions
            },
            // Get indicators
            async { self.get_indicators().await.unwrap_or_default() },
            // Get trades
            async { self.get_trades().await.unwrap_or_default() }
        );

        // Aggregated data
        let aggregated = trading_cli::AggregatedData {
            overall: Some(trading_cli::OverallStatistics {
                total_orders: orders.len() as i32,
                total_positions: positions.len() as i32,
                total_position_value: String::new(), // TODO: Calculate
                total_pnl: String::new(),            // TODO: Calculate
                total_balance: balances
                    .iter()
                    .find(|b| b.asset == "USDT" || b.asset == "USD")
                    .map(|b| b.total.clone())
                    .unwrap_or_default(),
                active_pairs: 1,  // TODO: Get from config
                active_levels: 0, // TODO: Get from LevelManager
            }),
            per_pair: std::collections::HashMap::new(),
            per_level: std::collections::HashMap::new(),
        };

        let elapsed = start.elapsed();
        if elapsed.as_millis() > 100 {
            tracing::warn!(
                "collect_monitoring_data took {}ms (slow!)",
                elapsed.as_millis()
            );
        }

        // Log if collection took too long
        let elapsed = start.elapsed();
        if elapsed.as_millis() > 100 {
            tracing::warn!(
                "[gRPC] collect_monitoring_data took {}ms (slow!)",
                elapsed.as_millis()
            );
        }

        Ok(trading_cli::MonitoringData {
            orders,
            balances,
            trades,
            positions,
            indicators,
            aggregated: Some(aggregated),
        })
    }

    /// Get recent trades from ExchangeStateManager (already populated via WebSocket)
    async fn get_trades(&self) -> Result<Vec<trading_cli::Trade>, Status> {
        // Get trades directly from ExchangeStateManager - they're already there from WebSocket updates
        let state_arc = self.exchange_state_manager.get_state();
        let trades: Vec<trading_core::types::exchange::Trade> = {
            let exchange_state = state_arc.read().await;
            // Clone the trades to own them before dropping the lock
            exchange_state
                .get_trades()
                .iter()
                .map(|t| (*t).clone())
                .collect()
        };

        // Convert to protobuf format, sorted by timestamp (most recent first)
        let mut converted: Vec<trading_cli::Trade> = trades
            .iter()
            .map(|t| trading_cli::Trade {
                trade_id: t.trade_id.to_string(),
                order_id: t.order_id.as_str().to_string(),
                symbol: t.pair.convert_to_smolstr().to_string(),
                side: format!("{:?}", t.side),
                price: t.price.to_string(),
                quantity: t.quantity.to_string(),
                timestamp: t.timestamp as i64,
            })
            .collect();

        // Sort by timestamp descending (most recent first)
        converted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Limit to most recent 100 trades for display
        converted.truncate(100);

        Ok(converted)
    }

    /// Get indicators from IndicatorService
    async fn get_indicators(&self) -> Result<Vec<trading_cli::IndicatorValue>, Status> {
        // Access calculator directly - it's thread-safe, no locking needed
        let values = self.indicator_calculator.get_all_current_values();

        let mut converted: Vec<trading_cli::IndicatorValue> = values
            .into_iter()
            .map(|v| trading_cli::IndicatorValue {
                indicator_id: v.indicator_id.to_string(),
                indicator_name: v.indicator_name.to_string(),
                pair: v.pair.to_string(),
                value: v.value.to_string(),
                timestamp: v.timestamp as i64,
                period: v.period as i32,
                is_composite: v.is_composite,
            })
            .collect();

        // Sort by timestamp descending and limit
        converted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        converted.truncate(100);

        Ok(converted)
    }
}

#[async_trait]
impl TradingCliService for TradingCliServiceImpl {
    /// Set trading enabled/disabled
    async fn set_trading_enabled(
        &self,
        request: Request<trading_cli::SetTradingEnabledRequest>,
    ) -> Result<Response<trading_cli::SetTradingEnabledResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            "[gRPC] set_trading_enabled called with enabled={}",
            req.enabled
        );

        // Get active order IDs from trading state
        let active_order_ids = {
            let state_arc = self.trading_state_manager.get_state();
            let state = state_arc.read().await;
            state
                .active_orders
                .values()
                .map(|o| o.order_id.clone())
                .collect()
        };

        match self
            .trading_control
            .set_enabled(req.enabled, active_order_ids)
            .await
        {
            Ok(cancelled_orders) => {
                tracing::info!(
                    "[gRPC] set_trading_enabled succeeded, cancelled {} orders",
                    cancelled_orders.len()
                );
                Ok(Response::new(trading_cli::SetTradingEnabledResponse {
                    success: true,
                    message: if req.enabled {
                        "Trading enabled".to_string()
                    } else {
                        format!(
                            "Trading disabled. Cancelled {} orders.",
                            cancelled_orders.len()
                        )
                    },
                    cancelled_orders,
                }))
            }
            Err(e) => {
                tracing::error!("[gRPC] set_trading_enabled failed: {}", e);
                Err(Status::internal(format!(
                    "Failed to set trading status: {}",
                    e
                )))
            }
        }
    }

    /// Get trading status
    async fn get_trading_status(
        &self,
        _request: Request<trading_cli::GetTradingStatusRequest>,
    ) -> Result<Response<trading_cli::GetTradingStatusResponse>, Status> {
        tracing::info!("[gRPC] get_trading_status called");
        let enabled = self.trading_control.is_enabled().await;
        tracing::info!("[gRPC] get_trading_status returning: enabled={}", enabled);
        Ok(Response::new(trading_cli::GetTradingStatusResponse {
            enabled,
        }))
    }

    // Placeholder implementations for Phase 2-5 RPCs
    async fn get_levels(
        &self,
        _request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        let level_manager = self.level_manager.read().await;
        let all_levels = level_manager.get_all_levels();

        let mut proto_levels = Vec::new();
        for level in all_levels {
            let status = match level.state {
                trading_engine::levels::level::LevelState::Created => "Created",
                trading_engine::levels::level::LevelState::Posted => "Posted",
                trading_engine::levels::level::LevelState::Filled => "Filled",
                trading_engine::levels::level::LevelState::Cancelled => "Cancelled",
                trading_engine::levels::level::LevelState::Paused => "Paused",
                trading_engine::levels::level::LevelState::Removed => "Removed",
            };

            let symbol = trading_cli::Symbol {
                base: level.config.symbol.base.as_str().to_string(),
                quote: level.config.symbol.quote.as_str().to_string(),
            };

            let order_type = match level.config.order_type {
                trading_engine::levels::config::OrderTypeConfig::Limit => "Limit",
                trading_engine::levels::config::OrderTypeConfig::Market => "Market",
                trading_engine::levels::config::OrderTypeConfig::PostOnly => "PostOnly",
                trading_engine::levels::config::OrderTypeConfig::IoC => "IoC",
                trading_engine::levels::config::OrderTypeConfig::FoK => "FoK",
            };

            let side = match level.config.side {
                trading_core::types::common::Side::Buy => "Buy",
                trading_core::types::common::Side::Sell => "Sell",
            };

            // Format price config as string
            let price_str = match &level.config.price {
                trading_engine::levels::config::PriceConfig::Fixed(d) => format!("Fixed({})", d),
                trading_engine::levels::config::PriceConfig::Indicator {
                    indicator_name,
                    offset_type,
                    offset_value,
                } => format!(
                    "Indicator({}, {:?}, {})",
                    indicator_name, offset_type, offset_value
                ),
                trading_engine::levels::config::PriceConfig::BestBid { offset } => {
                    format!("BestBid({})", offset)
                }
                trading_engine::levels::config::PriceConfig::BestAsk { offset } => {
                    format!("BestAsk({})", offset)
                }
                trading_engine::levels::config::PriceConfig::MidPrice { offset_percent } => {
                    format!("MidPrice({})", offset_percent)
                }
            };

            // Format quantity config as string
            let quantity_str = match &level.config.quantity {
                trading_engine::levels::config::QuantityConfig::Fixed(d) => format!("Fixed({})", d),
                trading_engine::levels::config::QuantityConfig::Indicator {
                    indicator_name,
                    multiplier,
                    offset,
                } => format!(
                    "Indicator({}, mul:{:?}, off:{:?})",
                    indicator_name, multiplier, offset
                ),
                trading_engine::levels::config::QuantityConfig::BalancePercent(p) => {
                    format!("BalancePercent({})", p)
                }
                trading_engine::levels::config::QuantityConfig::Notional(n) => {
                    format!("Notional({})", n)
                }
            };

            let level_config = trading_cli::LevelConfig {
                id: level.config.id.to_string(),
                symbol: Some(symbol),
                side: side.to_string(),
                order_type: order_type.to_string(),
                price: price_str,
                quantity: quantity_str,
                enabled: level.config.enabled,
                risk_limit: level
                    .config
                    .risk
                    .daily_loss_limit
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                max_position_size: level
                    .config
                    .risk
                    .max_position
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                repost_on_fill: level.config.lifecycle.auto_repost,
                repost_on_cancel: level.config.lifecycle.auto_repost_on_cancel,
            };

            let active_order_ids: Vec<String> = level
                .active_orders
                .iter()
                .map(|id| id.as_str().to_string())
                .collect();

            let proto_level = trading_cli::Level {
                id: level.config.id.to_string(),
                status: status.to_string(),
                enabled: level.config.enabled,
                config: Some(level_config),
                active_orders: active_order_ids,
                filled_count: level.repost_count as i32, // Using repost_count as filled_count for now
            };

            proto_levels.push(proto_level);
        }

        Ok(Response::new(trading_cli::GetLevelsResponse {
            levels: proto_levels,
        }))
    }

    async fn get_level(
        &self,
        request: Request<trading_cli::GetLevelRequest>,
    ) -> Result<Response<trading_cli::GetLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = smol_str::SmolStr::new(&req.level_id);

        let level_manager = self.level_manager.read().await;
        let level = level_manager
            .get_level(&level_id)
            .ok_or_else(|| Status::not_found(format!("Level {} not found", req.level_id)))?;

        // Convert Level to proto Level (same logic as get_levels)
        let status = match level.state {
            trading_engine::levels::level::LevelState::Created => "Created",
            trading_engine::levels::level::LevelState::Posted => "Posted",
            trading_engine::levels::level::LevelState::Filled => "Filled",
            trading_engine::levels::level::LevelState::Cancelled => "Cancelled",
            trading_engine::levels::level::LevelState::Paused => "Paused",
            trading_engine::levels::level::LevelState::Removed => "Removed",
        };

        let symbol = trading_cli::Symbol {
            base: level.config.symbol.base.as_str().to_string(),
            quote: level.config.symbol.quote.as_str().to_string(),
        };

        let order_type = match level.config.order_type {
            trading_engine::levels::config::OrderTypeConfig::Limit => "Limit",
            trading_engine::levels::config::OrderTypeConfig::Market => "Market",
            trading_engine::levels::config::OrderTypeConfig::PostOnly => "PostOnly",
            trading_engine::levels::config::OrderTypeConfig::IoC => "IoC",
            trading_engine::levels::config::OrderTypeConfig::FoK => "FoK",
        };

        let side = match level.config.side {
            trading_core::types::common::Side::Buy => "Buy",
            trading_core::types::common::Side::Sell => "Sell",
        };

        // Format price config as string
        let price_str = match &level.config.price {
            trading_engine::levels::config::PriceConfig::Fixed(d) => format!("Fixed({})", d),
            trading_engine::levels::config::PriceConfig::Indicator {
                indicator_name,
                offset_type,
                offset_value,
            } => format!(
                "Indicator({}, {:?}, {})",
                indicator_name, offset_type, offset_value
            ),
            trading_engine::levels::config::PriceConfig::BestBid { offset } => {
                format!("BestBid({})", offset)
            }
            trading_engine::levels::config::PriceConfig::BestAsk { offset } => {
                format!("BestAsk({})", offset)
            }
            trading_engine::levels::config::PriceConfig::MidPrice { offset_percent } => {
                format!("MidPrice({})", offset_percent)
            }
        };

        // Format quantity config as string
        let quantity_str = match &level.config.quantity {
            trading_engine::levels::config::QuantityConfig::Fixed(d) => format!("Fixed({})", d),
            trading_engine::levels::config::QuantityConfig::Indicator {
                indicator_name,
                multiplier,
                offset,
            } => format!(
                "Indicator({}, mul:{:?}, off:{:?})",
                indicator_name, multiplier, offset
            ),
            trading_engine::levels::config::QuantityConfig::BalancePercent(p) => {
                format!("BalancePercent({})", p)
            }
            trading_engine::levels::config::QuantityConfig::Notional(n) => {
                format!("Notional({})", n)
            }
        };

        let level_config = trading_cli::LevelConfig {
            id: level.config.id.to_string(),
            symbol: Some(symbol),
            side: side.to_string(),
            order_type: order_type.to_string(),
            price: price_str,
            quantity: quantity_str,
            enabled: level.config.enabled,
            risk_limit: level
                .config
                .risk
                .daily_loss_limit
                .map(|d| d.to_string())
                .unwrap_or_default(),
            max_position_size: level
                .config
                .risk
                .max_position
                .map(|d| d.to_string())
                .unwrap_or_default(),
            repost_on_fill: level.config.lifecycle.auto_repost,
            repost_on_cancel: level.config.lifecycle.auto_repost_on_cancel,
        };

        let active_order_ids: Vec<String> = level
            .active_orders
            .iter()
            .map(|id| id.as_str().to_string())
            .collect();

        let proto_level = trading_cli::Level {
            id: level.config.id.to_string(),
            status: status.to_string(),
            enabled: level.config.enabled,
            config: Some(level_config),
            active_orders: active_order_ids,
            filled_count: level.repost_count as i32,
        };

        Ok(Response::new(trading_cli::GetLevelResponse {
            level: Some(proto_level),
        }))
    }

    async fn create_level(
        &self,
        _request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn update_level(
        &self,
        _request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn delete_level(
        &self,
        _request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = smol_str::SmolStr::new(&req.level_id);

        let mut level_manager = self.level_manager.write().await;

        // Get current enabled state and active orders to determine what to do
        let (current_enabled, active_orders) = level_manager
            .get_level(&level_id)
            .map(|l| (l.config.enabled, l.active_orders.clone()))
            .ok_or_else(|| Status::not_found(format!("Level {} not found", req.level_id)))?;

        // Only toggle if the requested state is different from current
        if current_enabled != req.enabled {
            if req.enabled {
                level_manager
                    .enable_level(&level_id)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to enable level: {}", e)))?;
            } else {
                // When disabling, cancel all active orders for this level
                if !active_orders.is_empty() {
                    let trading_client = self.trading_client.read().await;
                    let mut cancelled_count = 0;
                    let mut failed_count = 0;

                    for order_id in &active_orders {
                        match trading_client.cancel_order_by_id(order_id.clone()).await {
                            Ok(_) => {
                                cancelled_count += 1;
                                tracing::info!(
                                    "Cancelled order {} for disabled level {}",
                                    order_id.as_str(),
                                    level_id
                                );
                            }
                            Err(e) => {
                                failed_count += 1;
                                tracing::warn!(
                                    "Failed to cancel order {} for level {}: {}",
                                    order_id.as_str(),
                                    level_id,
                                    e
                                );
                            }
                        }
                    }
                    drop(trading_client);

                    tracing::info!(
                        "Level {} disabled: cancelled {} order(s), {} failed",
                        level_id,
                        cancelled_count,
                        failed_count
                    );

                    // Clear active orders from the level
                    if let Some(level) = level_manager.get_level_mut(&level_id) {
                        level.active_orders.clear();
                    }
                }

                // Now disable the level
                level_manager
                    .disable_level(&level_id)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to disable level: {}", e)))?;
            }
        }

        Ok(Response::new(trading_cli::ToggleLevelResponse {
            success: true,
        }))
    }

    async fn get_monitoring_data(
        &self,
        _request: Request<trading_cli::GetMonitoringDataRequest>,
    ) -> Result<Response<trading_cli::GetMonitoringDataResponse>, Status> {
        // Collect monitoring data from various sources
        let monitoring_data = self.collect_monitoring_data().await?;

        Ok(Response::new(trading_cli::GetMonitoringDataResponse {
            data: Some(monitoring_data),
        }))
    }

    type StreamMonitoringDataStream = tonic::codegen::tokio_stream::wrappers::ReceiverStream<
        Result<trading_cli::MonitoringDataUpdate, Status>,
    >;

    async fn stream_monitoring_data(
        &self,
        _request: Request<trading_cli::StreamMonitoringDataRequest>,
    ) -> Result<Response<Self::StreamMonitoringDataStream>, Status> {
        tracing::info!("[gRPC] stream_monitoring_data: RPC called");
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let service = self.clone_for_streaming();

        tracing::info!("[gRPC] stream_monitoring_data: spawning streaming task");
        tokio::spawn(async move {
            // Add panic handler to catch silent failures
            std::panic::set_hook(Box::new(|panic_info| {
                tracing::error!(
                    "[gRPC] stream_monitoring_data: PANIC in streaming task: {:?}",
                    panic_info
                );
            }));

            tracing::info!("[gRPC] stream_monitoring_data: task started");
            // Use longer interval to reduce load - 1 second instead of 500ms
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            // Skip the first tick which happens immediately
            interval.tick().await;
            loop {
                interval.tick().await;

                // Collect data - no timeout needed since we access calculator directly (no lock contention)
                match service.collect_monitoring_data().await {
                    Ok(data) => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;

                        let update = trading_cli::MonitoringDataUpdate {
                            data: Some(data),
                            timestamp,
                        };

                        if tx.send(Ok(update)).await.is_err() {
                            tracing::info!(
                                "[gRPC] stream_monitoring_data: receiver dropped, stopping"
                            );
                            break; // Receiver dropped
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "[gRPC] stream_monitoring_data: collect_monitoring_data failed: {}",
                            e
                        );
                        let _ = tx
                            .send(Err(Status::internal(format!(
                                "Failed to collect data: {}",
                                e
                            ))))
                            .await;
                        break;
                    }
                }
            }
            tracing::info!("[gRPC] stream_monitoring_data: task ended");
        });

        Ok(Response::new(
            tonic::codegen::tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }

    async fn get_system_health(
        &self,
        _request: Request<trading_cli::GetSystemHealthRequest>,
    ) -> Result<Response<trading_cli::GetSystemHealthResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn get_protocol_health(
        &self,
        _request: Request<trading_cli::GetProtocolHealthRequest>,
    ) -> Result<Response<trading_cli::GetProtocolHealthResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn replace_config(
        &self,
        _request: Request<trading_cli::ReplaceConfigRequest>,
    ) -> Result<Response<trading_cli::ReplaceConfigResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn validate_config(
        &self,
        _request: Request<trading_cli::ValidateConfigRequest>,
    ) -> Result<Response<trading_cli::ValidateConfigResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn preview_config_changes(
        &self,
        _request: Request<trading_cli::PreviewConfigChangesRequest>,
    ) -> Result<Response<trading_cli::PreviewConfigChangesResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }
}
