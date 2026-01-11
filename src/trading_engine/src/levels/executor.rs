//! Level executor - executes levels by resolving indicator values and placing orders
use super::super::order_validation::{get_trading_rules, validate_order};
use super::{IndicatorResolver, Level, ResolvedLevel};
use asset_engine::AssetService;
use indicator_engine::data::IndicatorValue;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_core::traits::TradingClient;
use trading_core::types::common::{Exchange, Symbol};
use trading_core::types::exchange::{Order, OrderRequest};
use trading_core::types::market_data::Orderbook;

/// Executes levels by resolving indicator values and placing orders
pub struct LevelExecutor {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    resolver: IndicatorResolver,
    asset_service: Option<Arc<AssetService>>,
    exchange: Exchange,
    /// Config fallback values
    config_min_order_size: rust_decimal::Decimal,
    config_step_size: rust_decimal::Decimal,
    config_tick_size: rust_decimal::Decimal,
    config_min_notional: rust_decimal::Decimal,
    /// Trading control - checks if trading is enabled (optional, for TUI control)
    trading_enabled: Option<Arc<tokio::sync::RwLock<bool>>>,
}

impl LevelExecutor {
    /// Create a new level executor
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        config_min_order_size: rust_decimal::Decimal,
        config_step_size: rust_decimal::Decimal,
        config_tick_size: rust_decimal::Decimal,
        config_min_notional: rust_decimal::Decimal,
    ) -> Self {
        let resolver = IndicatorResolver::new(
            asset_service.clone(),
            exchange.clone(),
            config_tick_size,
            config_step_size,
        );
        Self {
            trading_client,
            resolver,
            asset_service,
            exchange,
            config_min_order_size,
            config_step_size,
            config_tick_size,
            config_min_notional,
            trading_enabled: None,
        }
    }

    /// Set trading enabled flag (for TUI control)
    pub fn with_trading_control(mut self, trading_enabled: Arc<tokio::sync::RwLock<bool>>) -> Self {
        self.trading_enabled = Some(trading_enabled);
        self
    }

    /// Resolve a level to an OrderRequest
    pub async fn resolve_level(
        &self,
        level: &Level,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        orderbook: Option<&Orderbook>,
        balances: &[trading_core::types::exchange::Balance],
    ) -> Result<ResolvedLevel, EngineError> {
        // Get current price for quantity calculations
        let current_price = if let Some(ob) = orderbook {
            ob.mid_price().ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!(
                    "Cannot determine current price from orderbook"
                ))
            })?
        } else {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Orderbook required for level resolution"
            )));
        };

        // Get balance for quantity calculations
        let balance = match level.config.side {
            trading_core::types::common::Side::Buy => {
                // Need quote currency balance
                balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == level.pair.quote.as_str())
                    .map(|b| b.free)
                    .unwrap_or(rust_decimal::Decimal::ZERO)
            }
            trading_core::types::common::Side::Sell => {
                // Need base currency balance
                balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == level.pair.base.as_str())
                    .map(|b| b.free)
                    .unwrap_or(rust_decimal::Decimal::ZERO)
            }
        };

        // Resolve price (with rounding to tick size)
        let resolved_price = self
            .resolver
            .resolve_price(&level.config.price, indicators, orderbook, &level.pair)
            .await?;

        // Resolve quantity (with rounding to step size)
        let resolved_quantity = self
            .resolver
            .resolve_quantity(
                &level.config.quantity,
                indicators,
                balance,
                current_price,
                &level.pair,
            )
            .await?;

        // Validate order against trading rules (Asset Engine or config fallback)
        let rules = get_trading_rules(
            self.asset_service.as_ref(),
            self.exchange.clone(),
            &level.pair,
            self.config_min_order_size,
            self.config_step_size,
            self.config_tick_size,
            self.config_min_notional,
        )
        .await;
        validate_order(resolved_price, resolved_quantity, &rules)?;

        // Convert order type (OrderTypeConfig is Clone but not Copy)
        let resolved_order_type = level.config.order_type.clone().into();

        Ok(ResolvedLevel {
            level: level.clone(),
            resolved_price,
            resolved_quantity,
            resolved_order_type,
        })
    }

    /// Post a level (place order)
    pub async fn post_level(
        &self,
        level: &Level,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        orderbook: Option<&Orderbook>,
        balances: &[trading_core::types::exchange::Balance],
    ) -> Result<Order, EngineError> {
        // Check if trading is enabled
        if let Some(ref trading_enabled) = self.trading_enabled {
            if !*trading_enabled.read().await {
                tracing::warn!(
                    "Trading is disabled - blocking order placement for level {}",
                    level.config.id
                );
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "Trading is disabled"
                )));
            }
        }

        let resolved = self
            .resolve_level(level, indicators, orderbook, balances)
            .await?;

        // Create order request
        let order_request = OrderRequest {
            symbol: Symbol::from(level.pair.convert_to_smolstr()),
            side: level.config.side.clone(),
            order_type: resolved.resolved_order_type,
            price: resolved.resolved_price,
            amount: resolved.resolved_quantity,
            timestamp: Some(trading_core::utils::current_timestamp()),
            instrument_type: trading_core::types::common::InstrumentType::Spot,
            exchange_params: std::collections::HashMap::new(),
        };

        // Place order
        let client = self.trading_client.write().await;
        let order = client.open_order(order_request).await?;
        Ok(order)
    }

    /// Amend an existing order for a level if price/quantity changed
    /// Returns true if order was amended, false if no amendment needed
    pub async fn amend_level_order_if_needed(
        &self,
        level: &Level,
        resolved: &ResolvedLevel,
        existing_order: &Order,
    ) -> Result<Option<Order>, EngineError> {
        use rust_decimal::Decimal;

        // Check if price or quantity changed significantly (use small epsilon for comparison)
        let price_changed =
            (existing_order.price - resolved.resolved_price).abs() > Decimal::new(1, 10); // 0.0000000001 difference threshold
        let quantity_changed =
            (existing_order.amount - resolved.resolved_quantity).abs() > Decimal::new(1, 10);

        // Only amend if price or quantity changed (and side/order_type haven't changed)
        if !price_changed && !quantity_changed {
            tracing::debug!(
                "Level {} order {} doesn't need amendment (price={}, qty={} unchanged)",
                level.config.id,
                existing_order.order_id.as_str(),
                existing_order.price,
                existing_order.amount
            );
            return Ok(None);
        }

        // Check if side changed - if so, cannot amend, must cancel+place
        if existing_order.side != resolved.level.config.side {
            tracing::debug!(
                "Level {} order {} side changed, cannot amend",
                level.config.id,
                existing_order.order_id.as_str()
            );
            return Ok(None);
        }

        tracing::info!(
            "Amending level {} order {}: price {} -> {}, quantity {} -> {}",
            level.config.id,
            existing_order.order_id.as_str(),
            existing_order.price,
            resolved.resolved_price,
            existing_order.amount,
            resolved.resolved_quantity
        );

        // Create order request with new values
        let order_request = OrderRequest {
            symbol: resolved.level.pair.convert_to_smolstr().into(),
            side: resolved.level.config.side.clone(),
            order_type: resolved.resolved_order_type.clone(),
            price: resolved.resolved_price,
            amount: resolved.resolved_quantity,
            timestamp: Some(trading_core::utils::current_timestamp()),
            instrument_type: trading_core::types::common::InstrumentType::Spot,
            exchange_params: std::collections::HashMap::new(),
        };

        // Amend order
        let client = self.trading_client.write().await;
        let amended_order = client
            .amend_order(existing_order.order_id.clone(), order_request)
            .await?;

        Ok(Some(amended_order))
    }

    /// Cancel all orders for a level
    /// Cancels individual orders for this level only (not all orders for the pair)
    pub async fn cancel_level_orders(&self, level: &Level) -> Result<(), EngineError> {
        let client = self.trading_client.write().await;

        if level.active_orders.is_empty() {
            tracing::debug!("Level {} has no active orders to cancel", level.config.id);
            return Ok(());
        }

        tracing::info!(
            "Cancelling {} order(s) for level {} ({}/{})",
            level.active_orders.len(),
            level.config.id,
            level.pair.base,
            level.pair.quote
        );

        let mut cancelled_count = 0;
        let mut failed_count = 0;

        // Cancel individual orders for this level only
        for order_id in &level.active_orders {
            tracing::info!(
                "Cancelling order {} for level {}",
                order_id.as_str(),
                level.config.id
            );
            match client.cancel_order_by_id(order_id.clone()).await {
                Ok(_) => {
                    cancelled_count += 1;
                    tracing::info!(
                        "Successfully cancelled order {} for level {}",
                        order_id.as_str(),
                        level.config.id
                    );
                }
                Err(e) => {
                    failed_count += 1;
                    tracing::warn!(
                        "Failed to cancel order {} for level {}: {}",
                        order_id.as_str(),
                        level.config.id,
                        e
                    );
                }
            }
        }

        tracing::info!(
            "Level {} cancellation complete: {} cancelled, {} failed",
            level.config.id,
            cancelled_count,
            failed_count
        );

        if failed_count > 0 {
            Err(EngineError::CustomError(anyhow::anyhow!(
                "Failed to cancel {} order(s) for level {}",
                failed_count,
                level.config.id
            )))
        } else {
            Ok(())
        }
    }
}
