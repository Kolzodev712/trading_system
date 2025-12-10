//! Level executor - executes levels by resolving indicator values and placing orders
use super::order_validation::{get_trading_rules, validate_order};
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
        }
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
                    .find(|b| b.symbol.as_ref() == level.config.symbol.quote.as_str())
                    .map(|b| b.free)
                    .unwrap_or(rust_decimal::Decimal::ZERO)
            }
            trading_core::types::common::Side::Sell => {
                // Need base currency balance
                balances
                    .iter()
                    .find(|b| b.symbol.as_ref() == level.config.symbol.base.as_str())
                    .map(|b| b.free)
                    .unwrap_or(rust_decimal::Decimal::ZERO)
            }
        };

        // Resolve price (with rounding to tick size)
        let resolved_price = self
            .resolver
            .resolve_price(
                &level.config.price,
                indicators,
                orderbook,
                &level.config.symbol,
            )
            .await?;

        // Resolve quantity (with rounding to step size)
        let resolved_quantity = self
            .resolver
            .resolve_quantity(
                &level.config.quantity,
                indicators,
                balance,
                current_price,
                &level.config.symbol,
            )
            .await?;

        // Validate order against trading rules (Asset Engine or config fallback)
        let rules = get_trading_rules(
            self.asset_service.as_ref(),
            self.exchange.clone(),
            &level.config.symbol,
            self.config_min_order_size,
            self.config_step_size,
            self.config_tick_size,
            self.config_min_notional,
        )
        .await;
        validate_order(resolved_price, resolved_quantity, &rules)?;

        // Convert order type
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
        let resolved = self
            .resolve_level(level, indicators, orderbook, balances)
            .await?;

        // Create order request
        let order_request = OrderRequest {
            symbol: Symbol::from(level.config.symbol.convert_to_smolstr()),
            side: level.config.side.clone(),
            order_type: resolved.resolved_order_type,
            price: resolved.resolved_price,
            amount: resolved.resolved_quantity,
            timestamp: Some(trading_core::utils::current_timestamp()),
            exchange_params: std::collections::HashMap::new(),
        };

        // Place order
        let client = self.trading_client.write().await;
        let order = client.open_order(order_request).await?;
        Ok(order)
    }

    /// Cancel all orders for a level
    pub async fn cancel_level_orders(&self, level: &Level) -> Result<(), EngineError> {
        let client = self.trading_client.write().await;

        for order_id in &level.active_orders {
            if let Err(e) = client.cancel_order_by_id(order_id.clone()).await {
                tracing::warn!(
                    "Failed to cancel order {} for level {}: {}",
                    order_id.as_str(),
                    level.config.id,
                    e
                );
            }
        }

        Ok(())
    }
}
