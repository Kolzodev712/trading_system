//! Indicator resolver - resolves indicator values for level configuration
use super::config::{PriceConfig, PriceOffsetType, QuantityConfig};
use super::order_validation::{get_trading_rules, round_price, round_quantity};
use asset_engine::AssetService;
use indicator_engine::data::IndicatorValue;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use trading_core::error::EngineError;
use trading_core::types::common::Exchange;
use trading_core::types::market_data::Orderbook;

/// Resolves indicator values for level configuration
pub struct IndicatorResolver {
    asset_service: Option<Arc<AssetService>>,
    exchange: Exchange,
    /// Config fallback values
    config_tick_size: Decimal,
    config_step_size: Decimal,
}

impl IndicatorResolver {
    /// Create a new indicator resolver
    pub fn new(
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        config_tick_size: Decimal,
        config_step_size: Decimal,
    ) -> Self {
        Self {
            asset_service,
            exchange,
            config_tick_size,
            config_step_size,
        }
    }

    /// Resolve price from PriceConfig and round to tick size
    pub async fn resolve_price(
        &self,
        config: &PriceConfig,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        orderbook: Option<&Orderbook>,
        pair: &trading_core::types::common::Pair,
    ) -> Result<Decimal, EngineError> {
        let resolved_price = match config {
            PriceConfig::Fixed(price) => *price,

            PriceConfig::Indicator {
                indicator_name,
                offset_type,
                offset_value,
            } => {
                // Find indicator by name (match any indicator with this name)
                let indicator = indicators
                    .values()
                    .find(|v| v.indicator_name == *indicator_name)
                    .ok_or_else(|| {
                        EngineError::CustomError(anyhow::anyhow!(
                            "Indicator not found: {}",
                            indicator_name
                        ))
                    })?;

                let base_price = indicator.value;
                match offset_type {
                    PriceOffsetType::Absolute => base_price + offset_value,
                    PriceOffsetType::Percent => {
                        base_price * (Decimal::ONE + offset_value / Decimal::from(100))
                    }
                    PriceOffsetType::PercentBelow => {
                        base_price * (Decimal::ONE - offset_value / Decimal::from(100))
                    }
                    PriceOffsetType::PercentAbove => {
                        base_price * (Decimal::ONE + offset_value / Decimal::from(100))
                    }
                }
            }

            PriceConfig::BestBid { offset } => {
                let orderbook = orderbook.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Orderbook required for BestBid price"
                    ))
                })?;
                let best_bid = orderbook.best_bid_price().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("No best bid price in orderbook"))
                })?;
                best_bid - offset
            }

            PriceConfig::BestAsk { offset } => {
                let orderbook = orderbook.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Orderbook required for BestAsk price"
                    ))
                })?;
                let best_ask = orderbook.best_ask_price().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("No best ask price in orderbook"))
                })?;
                best_ask + offset
            }

            PriceConfig::MidPrice { offset_percent } => {
                let orderbook = orderbook.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("Orderbook required for MidPrice"))
                })?;
                let mid_price = orderbook.mid_price().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Cannot calculate mid price from orderbook: missing bids or asks"
                    ))
                })?;
                mid_price * (Decimal::ONE + offset_percent / Decimal::from(100))
            }
        };

        // Round price to tick size (Asset Engine or config fallback)
        let rules = get_trading_rules(
            self.asset_service.as_ref(),
            self.exchange.clone(),
            pair,
            Decimal::ZERO, // min_order_size not needed for price
            self.config_step_size,
            self.config_tick_size,
            Decimal::ZERO, // min_notional not needed for price
        )
        .await;
        Ok(round_price(resolved_price, rules.tick_size))
    }

    /// Resolve quantity from QuantityConfig and round to step size
    pub async fn resolve_quantity(
        &self,
        config: &QuantityConfig,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        balance: Decimal,
        current_price: Decimal,
        pair: &trading_core::types::common::Pair,
    ) -> Result<Decimal, EngineError> {
        let resolved_qty = match config {
            QuantityConfig::Fixed(qty) => *qty,

            QuantityConfig::Indicator {
                indicator_name,
                multiplier,
                offset,
            } => {
                // Find indicator by name
                let indicator = indicators
                    .values()
                    .find(|v| v.indicator_name == *indicator_name)
                    .ok_or_else(|| {
                        EngineError::CustomError(anyhow::anyhow!(
                            "Indicator not found: {}",
                            indicator_name
                        ))
                    })?;

                let base_qty = indicator.value;
                let mut qty = base_qty;

                if let Some(mult) = multiplier {
                    qty *= mult;
                }

                if let Some(off) = offset {
                    qty += off;
                }

                qty.max(Decimal::ZERO)
            }

            QuantityConfig::BalancePercent(percent) => {
                balance * percent / Decimal::from(100) / current_price.max(Decimal::ONE)
            }

            QuantityConfig::Notional(notional) => notional / current_price.max(Decimal::ONE),
        };

        // Round quantity to step size (Asset Engine or config fallback)
        let rules = get_trading_rules(
            self.asset_service.as_ref(),
            self.exchange.clone(),
            pair,
            Decimal::ZERO, // min_order_size not needed for rounding
            self.config_step_size,
            self.config_tick_size,
            Decimal::ZERO, // min_notional not needed for rounding
        )
        .await;
        Ok(round_quantity(resolved_qty, rules.step_size))
    }
}
