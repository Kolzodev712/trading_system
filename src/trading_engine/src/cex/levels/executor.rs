//! Level executor - executes levels by resolving indicator values and placing orders
use super::{IndicatorResolver, Level, ResolvedLevel};
use crate::order_validation::{get_trading_rules, validate_order};
use asset_engine::AssetService;
use indicator_engine::data::IndicatorValue;
use indicator_engine::level_pricing::LevelPricingRegistry;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_core::types::common::{Exchange, InstrumentType};
use trading_core::types::exchange::BulkCancelRequest;
use trading_core::types::exchange::OrderRequest;
use trading_core::types::market_data::Orderbook;
use trading_core::venue::cex::traits::TradingClient;

/// Executes levels by resolving indicator values and placing orders
pub struct LevelExecutor {
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    resolver: IndicatorResolver,
    asset_service: Option<Arc<AssetService>>,
    exchange: Exchange,
    /// Instrument type for orders (Spot or Futures)
    instrument_type: InstrumentType,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        instrument_type: InstrumentType,
        level_pricing: Arc<parking_lot::RwLock<LevelPricingRegistry>>,
        config_min_order_size: rust_decimal::Decimal,
        config_step_size: rust_decimal::Decimal,
        config_tick_size: rust_decimal::Decimal,
        config_min_notional: rust_decimal::Decimal,
    ) -> Self {
        let resolver = IndicatorResolver::new(
            asset_service.clone(),
            exchange.clone(),
            level_pricing,
            config_tick_size,
            config_step_size,
        );
        Self {
            trading_client,
            resolver,
            asset_service,
            exchange,
            instrument_type,
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

    /// Get instrument type
    pub fn instrument_type(&self) -> &InstrumentType {
        &self.instrument_type
    }

    /// Get trading enabled state (for batch operations)
    pub async fn is_trading_enabled(&self) -> bool {
        if let Some(ref trading_enabled) = self.trading_enabled {
            *trading_enabled.read().await
        } else {
            true // Default to enabled if no control set
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
            .resolve_price(
                &level.config.price,
                level.config.id.as_str(),
                indicators,
                orderbook,
                &level.pair,
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

    /// Cancel all orders for a level
    /// Cancels individual orders for this level only (not all orders for the pair)
    pub async fn cancel_level_orders(&self, level: &Level) -> Result<(), EngineError> {
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

        // Use bulk cancel
        let order_ids: Vec<_> = level.active_orders.to_vec();
        let bulk_request = BulkCancelRequest {
            order_ids: order_ids.clone(),
            pair: None,
        };
        let trading_client = self.trading_client.write().await;
        let bulk_response = trading_client.bulk_cancel_orders(bulk_request).await;

        let mut cancelled_count = 0;
        let mut failed_count = 0;

        match bulk_response {
            Ok(response) => {
                for result in response.results {
                    if result.success {
                        cancelled_count += 1;
                    } else {
                        failed_count += 1;
                        tracing::warn!(
                            "Failed to cancel order: {}",
                            result.error.unwrap_or_else(|| "Unknown error".to_string())
                        );
                    }
                }
            }
            Err(e) => {
                failed_count = order_ids.len();
                tracing::error!("Bulk cancel orders failed: {}", e);
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
