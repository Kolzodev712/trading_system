//! Level management RPC handlers
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::{LevelExecutor, LevelManager};

use super::converters;

pub struct LevelService {
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    pub trading_control: Arc<TradingControl>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
}

impl LevelService {
    pub async fn get_levels(
        &self,
        _request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        let level_manager = self.level_manager.read().await;
        let all_levels = level_manager.get_all_levels();

        let proto_levels: Vec<trading_cli::Level> = all_levels
            .into_iter()
            .map(converters::level_to_proto)
            .collect();

        Ok(Response::new(trading_cli::GetLevelsResponse {
            levels: proto_levels,
        }))
    }

    pub async fn get_level(
        &self,
        request: Request<trading_cli::GetLevelRequest>,
    ) -> Result<Response<trading_cli::GetLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = SmolStr::new(&req.level_id);

        let level_manager = self.level_manager.read().await;
        let level = level_manager
            .get_level(&level_id)
            .ok_or_else(|| Status::not_found(format!("Level {} not found", req.level_id)))?;

        let proto_level = converters::level_to_proto(level);

        Ok(Response::new(trading_cli::GetLevelResponse {
            level: Some(proto_level),
        }))
    }

    pub async fn create_level(
        &self,
        request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        let req = request.into_inner();
        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("LevelConfig is required"))?;

        if config.id.is_empty() {
            return Err(Status::invalid_argument("Level ID is required"));
        }

        let symbol = config
            .symbol
            .ok_or_else(|| Status::invalid_argument("Symbol is required"))?;

        if symbol.base.is_empty() || symbol.quote.is_empty() {
            return Err(Status::invalid_argument(
                "Symbol base and quote are required",
            ));
        }

        let price_config = if config.price.is_empty() {
            return Err(Status::invalid_argument("Price is required"));
        } else if let Some(price_str) = config.price.strip_prefix("Fixed(") {
            if let Some(price_str) = price_str.strip_suffix(')') {
                match rust_decimal::Decimal::from_str_exact(price_str) {
                    Ok(decimal) => trading_engine::levels::config::PriceConfig::Fixed(decimal),
                    Err(e) => {
                        return Err(Status::invalid_argument(format!(
                            "Invalid price format: {}",
                            e
                        )));
                    }
                }
            } else {
                return Err(Status::invalid_argument("Invalid price format"));
            }
        } else {
            return Err(Status::invalid_argument(
                "Only Fixed price is supported for now",
            ));
        };

        let quantity_config = if config.quantity.is_empty() {
            return Err(Status::invalid_argument("Quantity is required"));
        } else if let Some(qty_str) = config.quantity.strip_prefix("Fixed(") {
            if let Some(qty_str) = qty_str.strip_suffix(')') {
                match rust_decimal::Decimal::from_str_exact(qty_str) {
                    Ok(decimal) => trading_engine::levels::config::QuantityConfig::Fixed(decimal),
                    Err(e) => {
                        return Err(Status::invalid_argument(format!(
                            "Invalid quantity format: {}",
                            e
                        )));
                    }
                }
            } else {
                return Err(Status::invalid_argument("Invalid quantity format"));
            }
        } else {
            return Err(Status::invalid_argument(
                "Only Fixed quantity is supported for now",
            ));
        };

        let side = match config.side.as_str() {
            "Buy" => trading_core::types::common::Side::Buy,
            "Sell" => trading_core::types::common::Side::Sell,
            _ => return Err(Status::invalid_argument("Side must be 'Buy' or 'Sell'")),
        };

        let order_type = match config.order_type.as_str() {
            "Limit" => trading_engine::levels::config::OrderTypeConfig::Limit,
            "Market" => trading_engine::levels::config::OrderTypeConfig::Market,
            "PostOnly" => trading_engine::levels::config::OrderTypeConfig::PostOnly,
            "IoC" => trading_engine::levels::config::OrderTypeConfig::IoC,
            "FoK" => trading_engine::levels::config::OrderTypeConfig::FoK,
            _ => {
                return Err(Status::invalid_argument(format!(
                    "Invalid order type: {}",
                    config.order_type
                )));
            }
        };

        let daily_loss_limit = if !config.risk_limit.is_empty() {
            Some(
                rust_decimal::Decimal::from_str_exact(&config.risk_limit)
                    .map_err(|e| Status::invalid_argument(format!("Invalid risk_limit: {}", e)))?,
            )
        } else {
            None
        };

        let max_position = if !config.max_position_size.is_empty() {
            Some(
                rust_decimal::Decimal::from_str_exact(&config.max_position_size).map_err(|e| {
                    Status::invalid_argument(format!("Invalid max_position_size: {}", e))
                })?,
            )
        } else {
            None
        };

        let level_config = trading_engine::levels::config::LevelConfig {
            id: SmolStr::new(&config.id),
            name: None,
            side,
            order_type,
            quantity: quantity_config,
            price: price_config,
            risk: trading_engine::levels::config::LevelRiskConfig {
                max_position,
                max_orders: None,
                daily_loss_limit,
            },
            lifecycle: trading_engine::levels::config::LifecycleConfig {
                auto_repost: config.repost_on_fill,
                auto_repost_on_cancel: config.repost_on_cancel,
                max_reposts: None,
                repost_delay_secs: None,
                repost_conditions: Vec::new(),
            },
            enabled: config.enabled,
            group: None,
        };

        let pair = trading_core::types::common::Pair {
            base: SmolStr::new(&symbol.base),
            quote: SmolStr::new(&symbol.quote),
        };

        let mut level_manager = self.level_manager.write().await;
        level_manager
            .add_level(level_config.clone(), pair)
            .await
            .map_err(|e| Status::internal(format!("Failed to create level: {}", e)))?;

        tracing::info!("[gRPC] Level {} created", config.id);

        Ok(Response::new(trading_cli::CreateLevelResponse {
            level_id: config.id,
        }))
    }

    pub async fn update_level(
        &self,
        request: Request<trading_cli::UpdateLevelRequest>,
    ) -> Result<Response<trading_cli::UpdateLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = SmolStr::new(&req.level_id);

        let mut level_manager = self.level_manager.write().await;

        let level = level_manager
            .get_level_mut(&level_id)
            .ok_or_else(|| Status::not_found(format!("Level {} not found", req.level_id)))?;

        if let Some(config) = req.config {
            level.config.enabled = config.enabled;

            if !config.risk_limit.is_empty() {
                match rust_decimal::Decimal::from_str_exact(&config.risk_limit) {
                    Ok(decimal) => level.config.risk.daily_loss_limit = Some(decimal),
                    Err(e) => {
                        return Err(Status::invalid_argument(format!(
                            "Invalid risk_limit format: {}",
                            e
                        )));
                    }
                }
            }

            if !config.max_position_size.is_empty() {
                match rust_decimal::Decimal::from_str_exact(&config.max_position_size) {
                    Ok(decimal) => level.config.risk.max_position = Some(decimal),
                    Err(e) => {
                        return Err(Status::invalid_argument(format!(
                            "Invalid max_position_size format: {}",
                            e
                        )));
                    }
                }
            }

            level.config.lifecycle.auto_repost = config.repost_on_fill;
            level.config.lifecycle.auto_repost_on_cancel = config.repost_on_cancel;

            if !config.price.is_empty() {
                if let Some(price_str) = config.price.strip_prefix("Fixed(") {
                    if let Some(price_str) = price_str.strip_suffix(')') {
                        match rust_decimal::Decimal::from_str_exact(price_str) {
                            Ok(decimal) => {
                                level.config.price =
                                    trading_engine::levels::config::PriceConfig::Fixed(decimal);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse fixed price '{}': {}",
                                    price_str,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            if !config.quantity.is_empty() {
                if let Some(qty_str) = config.quantity.strip_prefix("Fixed(") {
                    if let Some(qty_str) = qty_str.strip_suffix(')') {
                        match rust_decimal::Decimal::from_str_exact(qty_str) {
                            Ok(decimal) => {
                                level.config.quantity =
                                    trading_engine::levels::config::QuantityConfig::Fixed(decimal);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse fixed quantity '{}': {}",
                                    qty_str,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        let updated_config = level.config.clone();
        let _ = level;

        level_manager
            .update_level_config(&level_id, updated_config)
            .await
            .map_err(|e| Status::internal(format!("Failed to update level: {}", e)))?;

        tracing::info!("[gRPC] Level {} updated", req.level_id);

        Ok(Response::new(trading_cli::UpdateLevelResponse {
            success: true,
        }))
    }

    pub async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = SmolStr::new(&req.level_id);

        let mut level_manager = self.level_manager.write().await;

        let current_enabled = level_manager
            .get_level(&level_id)
            .map(|l| l.config.enabled)
            .ok_or_else(|| Status::not_found(format!("Level {} not found", req.level_id)))?;

        let global_trading_enabled = self.trading_control.is_enabled().await;
        if !global_trading_enabled && req.enabled {
            return Err(Status::failed_precondition(
                "Cannot enable individual level when global trading is disabled. Enable global trading first.",
            ));
        }

        if current_enabled != req.enabled {
            if req.enabled {
                level_manager
                    .enable_level(&level_id)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to enable level: {}", e)))?;
            } else {
                let level_for_cancel = level_manager
                    .get_level(&level_id)
                    .cloned()
                    .ok_or_else(|| Status::not_found(format!("Level {} not found", level_id)))?;

                let orders_in_level = level_for_cancel.active_orders.len();
                let orders_on_exchange = self
                    .exchange_state_manager
                    .get_open_orders_for_pair(&level_for_cancel.pair)
                    .await
                    .len();

                if orders_in_level > 0 || orders_on_exchange > 0 {
                    tracing::info!(
                        "[gRPC] Disabling level {} ({}/{}), cancelling orders (level has {}, exchange has {})...",
                        level_id,
                        level_for_cancel.pair.base,
                        level_for_cancel.pair.quote,
                        orders_in_level,
                        orders_on_exchange
                    );

                    if orders_in_level > 0 {
                        let executor = LevelExecutor::new(
                            self.trading_client.clone(),
                            None,
                            trading_core::types::common::Exchange::Binance,
                            trading_core::types::common::InstrumentType::Spot,
                            rust_decimal::Decimal::new(1, 5),
                            rust_decimal::Decimal::new(1, 5),
                            rust_decimal::Decimal::new(1, 2),
                            rust_decimal::Decimal::new(5, 0),
                        );

                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(5),
                            executor.cancel_level_orders(&level_for_cancel),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                tracing::info!(
                                    "[gRPC] Successfully cancelled orders for level {}",
                                    level_id
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(
                                    "[gRPC] Failed to cancel some orders for level {}: {}",
                                    level_id,
                                    e
                                );
                            }
                            Err(_) => {
                                tracing::error!(
                                    "[gRPC] Timeout cancelling orders for level {} (5s timeout)",
                                    level_id
                                );
                            }
                        }
                    }

                    if orders_on_exchange > 0 && orders_in_level == 0 {
                        tracing::info!(
                            "[gRPC] Level {} has {} open order(s) on exchange but not in active_orders, using cancel_all_orders as fallback (WARNING: will cancel ALL orders for {}/{})",
                            level_id,
                            orders_on_exchange,
                            level_for_cancel.pair.base,
                            level_for_cancel.pair.quote
                        );
                        let pair_to_cancel = level_for_cancel.pair.clone();
                        drop(level_for_cancel);

                        tracing::info!(
                            "[gRPC] Calling cancel_all_orders for {}/{}...",
                            pair_to_cancel.base,
                            pair_to_cancel.quote
                        );

                        let trading_client_arc = Arc::clone(&self.trading_client);
                        let pair_clone = pair_to_cancel.clone();
                        let cancel_result = async move {
                            let trading_client = trading_client_arc.read().await;
                            trading_client.cancel_all_orders(&pair_clone).await
                        }
                        .await;

                        match cancel_result {
                            Ok(_) => {
                                tracing::info!(
                                    "[gRPC] Successfully cancelled all orders for {}/{} (fallback)",
                                    pair_to_cancel.base,
                                    pair_to_cancel.quote
                                );
                            }
                            Err(e) => {
                                let error_msg = format!("{}", e);
                                if error_msg.contains("Unknown order")
                                    || error_msg.contains("-2011")
                                {
                                    tracing::info!(
                                        "[gRPC] Orders for {}/{} were already cancelled (likely by global disable or another level)",
                                        pair_to_cancel.base,
                                        pair_to_cancel.quote
                                    );
                                } else if error_msg.contains("Timeout acquiring RPC channel")
                                    || error_msg.contains("RPC channel not ready")
                                    || error_msg.contains("RPC channel closed")
                                {
                                    tracing::error!(
                                        "[gRPC] RPC channel unavailable when cancelling orders for {}/{}: {} - connection may be down",
                                        pair_to_cancel.base,
                                        pair_to_cancel.quote,
                                        e
                                    );
                                } else {
                                    tracing::warn!(
                                        "[gRPC] Failed to cancel all orders for {}/{} (fallback): {}",
                                        pair_to_cancel.base,
                                        pair_to_cancel.quote,
                                        e
                                    );
                                }
                            }
                        }
                    }

                    if let Some(level) = level_manager.get_level_mut(&level_id) {
                        level.active_orders.clear();
                    }
                } else {
                    tracing::info!(
                        "[gRPC] Disabling level {} ({}/{}), no orders to cancel",
                        level_id,
                        level_for_cancel.pair.base,
                        level_for_cancel.pair.quote
                    );
                }

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
}
