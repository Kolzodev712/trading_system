//! Level management RPC handlers
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_core::grpc::TradingControl;
use trading_core::types::exchange::OrderRequest;
use trading_core::venue::cex::traits::TradingClient;
use trading_engine::levels::LevelManager;

use super::converters;

pub struct LevelService {
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    pub trading_control: Arc<TradingControl>,
    #[allow(dead_code)]
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

        if !config.risk_limit.is_empty() {
            return Err(Status::invalid_argument(
                "per-level daily_loss_limit is not enforceable until PnL attribution exists",
            ));
        }
        let daily_loss_limit = None;

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
                max_order_qty: None,
                max_notional: None,
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
        let instance_id = level_manager
            .add_level(level_config, pair)
            .await
            .map_err(|e| Status::internal(format!("Failed to create level: {}", e)))?;

        tracing::info!("[gRPC] Level {} created", instance_id);

        Ok(Response::new(trading_cli::CreateLevelResponse {
            level_id: instance_id.to_string(),
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
                return Err(Status::invalid_argument(
                    "per-level daily_loss_limit is not enforceable until PnL attribution exists",
                ));
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
                level_manager
                    .disable_level(&level_id)
                    .await
                    .map_err(|e| Status::internal(format!("Failed to disable level: {}", e)))?;
                let owned = level_manager
                    .get_level(&level_id)
                    .map(|level| level.active_orders.clone())
                    .unwrap_or_default();
                drop(level_manager);
                if !owned.is_empty() {
                    let client = self.trading_client.write().await;
                    if let Err(err) =
                        trading_engine::levels::cancel_owned_order_ids(&*client, &owned).await
                    {
                        return Err(Status::failed_precondition(format!(
                            "disable latched but owned-order cancel unverified: {err}"
                        )));
                    }
                }
            }
        }

        Ok(Response::new(trading_cli::ToggleLevelResponse {
            success: true,
        }))
    }

    pub async fn delete_level(
        &self,
        request: Request<trading_cli::DeleteLevelRequest>,
    ) -> Result<Response<trading_cli::DeleteLevelResponse>, Status> {
        let req = request.into_inner();
        let level_id = SmolStr::new(&req.level_id);
        let owned = {
            let mut level_manager = self.level_manager.write().await;
            level_manager
                .begin_delete(&level_id)
                .map_err(|e| Status::internal(format!("Failed to latch delete: {e}")))?
        };
        if !owned.is_empty() {
            let client = self.trading_client.write().await;
            if let Err(err) = trading_engine::levels::cancel_owned_order_ids(&*client, &owned).await
            {
                return Err(Status::failed_precondition(format!(
                    "delete latched; owned-order cancel unverified: {err}"
                )));
            }
        }
        let mut level_manager = self.level_manager.write().await;
        level_manager
            .complete_delete(&level_id)
            .map_err(|e| Status::internal(format!("Failed to complete delete: {e}")))?;
        Ok(Response::new(trading_cli::DeleteLevelResponse {
            success: true,
        }))
    }
}
