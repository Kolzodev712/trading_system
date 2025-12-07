//! Indicator resolver - resolves indicator values for level configuration
use super::config::{PriceConfig, PriceOffsetType, QuantityConfig};
use indicator_engine::data::IndicatorValue;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use trading_core::error::EngineError;
use trading_core::types::market_data::Orderbook;

/// Resolves indicator values for level configuration
pub struct IndicatorResolver;

impl IndicatorResolver {
    /// Resolve price from PriceConfig
    pub fn resolve_price(
        &self,
        config: &PriceConfig,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        orderbook: Option<&Orderbook>,
    ) -> Result<Decimal, EngineError> {
        match config {
            PriceConfig::Fixed(price) => Ok(*price),

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
                let resolved_price = match offset_type {
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
                };

                Ok(resolved_price)
            }

            PriceConfig::BestBid { offset } => {
                let orderbook = orderbook.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Orderbook required for BestBid price"
                    ))
                })?;
                let best_bid = orderbook.bids.first().copied().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("No best bid price in orderbook"))
                })?;
                Ok(best_bid - offset)
            }

            PriceConfig::BestAsk { offset } => {
                let orderbook = orderbook.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Orderbook required for BestAsk price"
                    ))
                })?;
                let best_ask = orderbook.asks.first().copied().ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("No best ask price in orderbook"))
                })?;
                Ok(best_ask + offset)
            }

            PriceConfig::MidPrice { offset_percent } => {
                let orderbook = orderbook.ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!("Orderbook required for MidPrice"))
                })?;
                if orderbook.bids.is_empty() || orderbook.asks.is_empty() {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Cannot calculate mid price from orderbook: missing bids or asks"
                    )));
                }
                let best_bid = orderbook.bids[0];
                let best_ask = orderbook.asks[0];
                let mid_price = (best_bid + best_ask) / Decimal::from(2);
                Ok(mid_price * (Decimal::ONE + offset_percent / Decimal::from(100)))
            }
        }
    }

    /// Resolve quantity from QuantityConfig
    pub fn resolve_quantity(
        &self,
        config: &QuantityConfig,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        balance: Decimal,
        current_price: Decimal,
    ) -> Result<Decimal, EngineError> {
        match config {
            QuantityConfig::Fixed(qty) => Ok(*qty),

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
                    qty = qty * mult;
                }

                if let Some(off) = offset {
                    qty = qty + off;
                }

                Ok(qty.max(Decimal::ZERO))
            }

            QuantityConfig::BalancePercent(percent) => {
                Ok(balance * percent / Decimal::from(100) / current_price.max(Decimal::ONE))
            }

            QuantityConfig::Notional(notional) => Ok(notional / current_price.max(Decimal::ONE)),
        }
    }
}
