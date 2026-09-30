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

        reject_daily_loss_field(&config.risk_limit)?;
        let daily_loss_limit = None;

        let max_position = parse_optional_positive_decimal(&config.max_position_size)?;

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
        let current = level_manager
            .get_level(&level_id)
            .ok_or_else(|| Status::not_found(format!("Level {} not found", req.level_id)))?;
        let mut draft = current.config.clone();

        if let Some(config) = req.config {
            reject_daily_loss_field(&config.risk_limit)?;
            draft.enabled = config.enabled;
            if let Some(max_position) = parse_optional_positive_decimal(&config.max_position_size)?
            {
                draft.risk.max_position = Some(max_position);
            } else if config.max_position_size.trim() == "0"
                || config.max_position_size.trim() == "0.0"
            {
                draft.risk.max_position = None;
            }

            draft.lifecycle.auto_repost = config.repost_on_fill;
            draft.lifecycle.auto_repost_on_cancel = config.repost_on_cancel;

            if !config.price.is_empty() {
                if let Some(price_str) = config.price.strip_prefix("Fixed(") {
                    if let Some(price_str) = price_str.strip_suffix(')') {
                        let decimal =
                            rust_decimal::Decimal::from_str_exact(price_str).map_err(|e| {
                                Status::invalid_argument(format!("Invalid price format: {e}"))
                            })?;
                        draft.price = trading_engine::levels::config::PriceConfig::Fixed(decimal);
                    }
                }
            }

            if !config.quantity.is_empty() {
                if let Some(qty_str) = config.quantity.strip_prefix("Fixed(") {
                    if let Some(qty_str) = qty_str.strip_suffix(')') {
                        let decimal =
                            rust_decimal::Decimal::from_str_exact(qty_str).map_err(|e| {
                                Status::invalid_argument(format!("Invalid quantity format: {e}"))
                            })?;
                        draft.quantity =
                            trading_engine::levels::config::QuantityConfig::Fixed(decimal);
                    }
                }
            }
        }

        level_manager
            .update_level_config(&level_id, draft)
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
                    drop(client);
                    let mut level_manager = self.level_manager.write().await;
                    level_manager
                        .retire_verified_order_ids(&level_id, &owned)
                        .map_err(|e| {
                            Status::internal(format!("Failed to retire owned ids: {e}"))
                        })?;
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
        if !owned.is_empty() {
            level_manager
                .retire_verified_order_ids(&level_id, &owned)
                .map_err(|e| Status::internal(format!("Failed to retire owned ids: {e}")))?;
        }
        level_manager
            .complete_delete(&level_id)
            .map_err(|e| Status::failed_precondition(format!("{e}")))?;
        Ok(Response::new(trading_cli::DeleteLevelResponse {
            success: true,
        }))
    }
}

fn parse_optional_positive_decimal(raw: &str) -> Result<Option<rust_decimal::Decimal>, Status> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value = rust_decimal::Decimal::from_str_exact(trimmed)
        .map_err(|e| Status::invalid_argument(format!("Invalid decimal: {e}")))?;
    if value <= rust_decimal::Decimal::ZERO {
        return Ok(None);
    }
    Ok(Some(value))
}

fn reject_daily_loss_field(raw: &str) -> Result<(), Status> {
    if parse_optional_positive_decimal(raw)?.is_some() {
        return Err(Status::invalid_argument(
            "per-level daily_loss_limit is not enforceable until PnL attribution exists",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use trading_core::error::EngineError;
    use trading_core::registry::symbol::SymbolRegistry;
    use trading_core::types::common::{Exchange, OrderId, Pair};
    use trading_core::types::exchange::{Balance, Order, Trade};
    use trading_core::venue::cex::channels::cex_venue_update_channel;

    #[derive(Debug)]
    struct EmptyRestClient;

    #[async_trait]
    impl TradingClient<OrderRequest> for EmptyRestClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }
        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            panic!("must not cancel_all");
        }
        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }
        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(vec![])
        }
        async fn fetch_trades(&self, _pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
            Ok(vec![])
        }
        async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }
        async fn amend_order(
            &self,
            _order_id: OrderId,
            _new_request: OrderRequest,
        ) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_open_orders(
            &self,
            _req: trading_core::types::exchange::BulkOrderRequest,
        ) -> Result<trading_core::types::exchange::BulkOrderResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    fn level_service() -> LevelService {
        let client: Arc<RwLock<dyn TradingClient<OrderRequest>>> =
            Arc::new(RwLock::new(EmptyRestClient));
        let (tx, _rx) = cex_venue_update_channel(8);
        let exchange_state_manager = Arc::new(ExchangeStateManager::new(
            tx,
            Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")])),
            client.clone(),
            0,
            false,
        ));
        LevelService {
            level_manager: Arc::new(RwLock::new(LevelManager::new())),
            trading_client: client.clone(),
            trading_control: Arc::new(TradingControl::new(client)),
            exchange_state_manager,
        }
    }

    fn ui_create_config() -> trading_cli::LevelConfig {
        trading_cli::LevelConfig {
            id: "buy_level_1".into(),
            symbol: Some(trading_cli::Symbol {
                base: "BTC".into(),
                quote: "USDT".into(),
            }),
            side: "Buy".into(),
            order_type: "Limit".into(),
            price: "Fixed(50000)".into(),
            quantity: "Fixed(0.001)".into(),
            enabled: true,
            risk_limit: "0".into(),
            max_position_size: "0".into(),
            repost_on_fill: false,
            repost_on_cancel: false,
        }
    }

    #[tokio::test]
    async fn create_accepts_ui_default_zero_risk_fields() {
        let service = level_service();
        let resp = service
            .create_level(Request::new(trading_cli::CreateLevelRequest {
                config: Some(ui_create_config()),
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.level_id, "buy_level_1_BTCUSDT");
    }

    #[tokio::test]
    async fn rejected_update_does_not_change_enabled() {
        let service = level_service();
        service
            .create_level(Request::new(trading_cli::CreateLevelRequest {
                config: Some(ui_create_config()),
            }))
            .await
            .unwrap();
        let mut bad = ui_create_config();
        bad.enabled = false;
        bad.risk_limit = "50".into();
        let err = service
            .update_level(Request::new(trading_cli::UpdateLevelRequest {
                level_id: "buy_level_1_BTCUSDT".into(),
                config: Some(bad),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let level = service
            .get_level(Request::new(trading_cli::GetLevelRequest {
                level_id: "buy_level_1_BTCUSDT".into(),
            }))
            .await
            .unwrap()
            .into_inner()
            .level
            .unwrap();
        assert!(level.enabled);
    }

    #[tokio::test]
    async fn delete_completes_from_rest_without_websocket_report() {
        let service = level_service();
        service
            .create_level(Request::new(trading_cli::CreateLevelRequest {
                config: Some(ui_create_config()),
            }))
            .await
            .unwrap();
        {
            let mut manager = service.level_manager.write().await;
            manager
                .confirm_posted(
                    &SmolStr::new("buy_level_1_BTCUSDT"),
                    &SmolStr::new("cid-1"),
                    &OrderId::new("ex-1".into()),
                )
                .unwrap();
        }
        service
            .delete_level(Request::new(trading_cli::DeleteLevelRequest {
                level_id: "buy_level_1_BTCUSDT".into(),
            }))
            .await
            .unwrap();
        let err = service
            .get_level(Request::new(trading_cli::GetLevelRequest {
                level_id: "buy_level_1_BTCUSDT".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn parse_zero_risk_limit_is_unset() {
        assert!(parse_optional_positive_decimal("0").unwrap().is_none());
        assert!(parse_optional_positive_decimal("0.0").unwrap().is_none());
        assert!(parse_optional_positive_decimal("").unwrap().is_none());
        assert!(parse_optional_positive_decimal("1.5").unwrap().is_some());
        reject_daily_loss_field("0").unwrap();
        assert!(reject_daily_loss_field("10").is_err());
    }
}
