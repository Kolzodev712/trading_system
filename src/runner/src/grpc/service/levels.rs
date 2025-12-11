//! Level management RPC handlers
use crate::grpc::trading_cli;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::traits::TradingClient;
use trading_core::types::exchange::OrderRequest;
use trading_engine::levels::LevelManager;

use super::converters;

/// Service data needed for level operations
pub struct LevelService {
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
}

impl LevelService {
    /// Get all levels
    pub async fn get_levels(
        &self,
        _request: Request<trading_cli::GetLevelsRequest>,
    ) -> Result<Response<trading_cli::GetLevelsResponse>, Status> {
        let level_manager = self.level_manager.read().await;
        let all_levels = level_manager.get_all_levels();

        let proto_levels: Vec<trading_cli::Level> = all_levels
            .iter()
            .map(|level| converters::level_to_proto(level))
            .collect();

        Ok(Response::new(trading_cli::GetLevelsResponse {
            levels: proto_levels,
        }))
    }

    /// Get a single level by ID
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

    /// Create a new level
    pub async fn create_level(
        &self,
        request: Request<trading_cli::CreateLevelRequest>,
    ) -> Result<Response<trading_cli::CreateLevelResponse>, Status> {
        let req = request.into_inner();
        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("LevelConfig is required"))?;

        // Validate required fields
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

        // Parse price config (only Fixed for now)
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

        // Parse quantity config (only Fixed for now)
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

        // Parse side
        let side = match config.side.as_str() {
            "Buy" => trading_core::types::common::Side::Buy,
            "Sell" => trading_core::types::common::Side::Sell,
            _ => {
                return Err(Status::invalid_argument("Side must be 'Buy' or 'Sell'"));
            }
        };

        // Parse order type
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

        // Parse risk limits
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

        // Build LevelConfig
        let level_config = trading_engine::levels::config::LevelConfig {
            id: SmolStr::new(&config.id),
            name: None,
            symbol: trading_core::types::common::Pair {
                base: SmolStr::new(&symbol.base),
                quote: SmolStr::new(&symbol.quote),
            },
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

        // Add level via LevelManager
        let mut level_manager = self.level_manager.write().await;
        level_manager
            .add_level(level_config.clone())
            .await
            .map_err(|e| Status::internal(format!("Failed to create level: {}", e)))?;

        tracing::info!("[gRPC] Level {} created", config.id);

        Ok(Response::new(trading_cli::CreateLevelResponse {
            level_id: config.id,
        }))
    }

    /// Update an existing level
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

        // Update fields from the provided config
        if let Some(config) = req.config {
            // Update enabled state
            level.config.enabled = config.enabled;

            // Update risk limits if provided
            if !config.risk_limit.is_empty() {
                match rust_decimal::Decimal::from_str_exact(&config.risk_limit) {
                    Ok(decimal) => {
                        level.config.risk.daily_loss_limit = Some(decimal);
                    }
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
                    Ok(decimal) => {
                        level.config.risk.max_position = Some(decimal);
                    }
                    Err(e) => {
                        return Err(Status::invalid_argument(format!(
                            "Invalid max_position_size format: {}",
                            e
                        )));
                    }
                }
            }

            // Update lifecycle settings
            level.config.lifecycle.auto_repost = config.repost_on_fill;
            level.config.lifecycle.auto_repost_on_cancel = config.repost_on_cancel;

            // Update price if it's a Fixed value (parse from string like "Fixed(123.45)")
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

            // Update quantity if it's a Fixed value (parse from string like "Fixed(0.001)")
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

        // Clone config before dropping the level reference
        let updated_config = level.config.clone();
        let _ = level; // Drop the mutable reference to level

        // Persist to MongoDB via LevelManager
        level_manager
            .update_level_config(&level_id, updated_config)
            .await
            .map_err(|e| Status::internal(format!("Failed to update level: {}", e)))?;

        tracing::info!("[gRPC] Level {} updated", req.level_id);

        Ok(Response::new(trading_cli::UpdateLevelResponse {
            success: true,
        }))
    }

    /// Toggle level enabled/disabled
    pub async fn toggle_level(
        &self,
        request: Request<trading_cli::ToggleLevelRequest>,
    ) -> Result<Response<trading_cli::ToggleLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = SmolStr::new(&req.level_id);

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
}
