//! Indicator resolver - resolves indicator values for level configuration
use super::config::{PriceConfig, PriceOffsetType, QuantityConfig};
use crate::order_validation::{get_trading_rules, round_price, round_quantity};
use asset_engine::AssetService;
use indicator_engine::data::IndicatorValue;
use indicator_engine::level_pricing::{LevelPricingRegistry, PricingOffsetType};
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
    level_pricing: Arc<parking_lot::RwLock<LevelPricingRegistry>>,
    /// Config fallback values
    config_tick_size: Decimal,
    config_step_size: Decimal,
}

impl IndicatorResolver {
    /// Create a new indicator resolver
    pub fn new(
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        level_pricing: Arc<parking_lot::RwLock<LevelPricingRegistry>>,
        config_tick_size: Decimal,
        config_step_size: Decimal,
    ) -> Self {
        Self {
            asset_service,
            exchange,
            level_pricing,
            config_tick_size,
            config_step_size,
        }
    }

    /// Resolve price from PriceConfig and round to tick size
    pub async fn resolve_price(
        &self,
        config: &PriceConfig,
        level_template_id: &str,
        indicators: &HashMap<SmolStr, IndicatorValue>,
        orderbook: Option<&Orderbook>,
        pair: &trading_core::types::common::Pair,
    ) -> Result<Decimal, EngineError> {
        let resolved_price = match config {
            PriceConfig::Fixed(price) => *price,

            PriceConfig::Binding => {
                let rule = self
                    .level_pricing
                    .read()
                    .get(level_template_id)
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::CustomError(anyhow::anyhow!(
                            "No level_pricing binding for level template {}",
                            level_template_id
                        ))
                    })?;
                let indicator_name = SmolStr::new(&rule.indicator_ref);
                let offset_type = to_price_offset_type(rule.offset_type);
                let offset_value = rule.offset_value;
                resolve_indicator_price(
                    &indicator_name,
                    offset_type,
                    offset_value,
                    indicators,
                    pair,
                )?
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
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_quantity(
        &self,
        config: &QuantityConfig,
        _indicators: &HashMap<SmolStr, IndicatorValue>,
        balance: Decimal,
        current_price: Decimal,
        pair: &trading_core::types::common::Pair,
        side: trading_core::types::common::Side,
        instrument: &trading_core::types::common::InstrumentType,
    ) -> Result<Decimal, EngineError> {
        let resolved_qty = match config {
            QuantityConfig::Fixed(qty) => *qty,

            QuantityConfig::BalancePercent(percent) => {
                balance_percent_quantity(*percent, balance, current_price, side, instrument)?
            }

            QuantityConfig::Notional(notional) => {
                if current_price <= Decimal::ZERO {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Notional sizing requires a valid price"
                    )));
                }
                notional / current_price
            }
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

fn to_price_offset_type(offset: PricingOffsetType) -> PriceOffsetType {
    match offset {
        PricingOffsetType::Absolute => PriceOffsetType::Absolute,
        PricingOffsetType::Percent => PriceOffsetType::Percent,
        PricingOffsetType::PercentBelow => PriceOffsetType::PercentBelow,
        PricingOffsetType::PercentAbove => PriceOffsetType::PercentAbove,
    }
}

fn resolve_indicator_price(
    indicator_name: &SmolStr,
    offset_type: PriceOffsetType,
    offset_value: Decimal,
    indicators: &HashMap<SmolStr, IndicatorValue>,
    pair: &trading_core::types::common::Pair,
) -> Result<Decimal, EngineError> {
    let pair_str = pair.convert_to_smolstr();
    let resolved_indicator_id =
        if indicator_name.contains('_') && !indicator_name.contains(pair_str.as_str()) {
            SmolStr::new(format!("{}_{}", indicator_name, pair_str))
        } else {
            indicator_name.clone()
        };

    let indicator = indicators
        .get(&resolved_indicator_id)
        .or_else(|| indicators.get(indicator_name))
        .or_else(|| {
            indicators
                .values()
                .find(|v| v.indicator_name == *indicator_name || v.indicator_id == *indicator_name)
        })
        .ok_or_else(|| {
            let available: Vec<_> = indicators.keys().map(|k| k.as_str()).collect();
            EngineError::CustomError(anyhow::anyhow!(
                "Indicator not found: {} (resolved as: {}, available: {:?})",
                indicator_name,
                resolved_indicator_id,
                available
            ))
        })?;

    let base_price = indicator.value;
    Ok(match offset_type {
        PriceOffsetType::Absolute => base_price + offset_value,
        PriceOffsetType::Percent => base_price * (Decimal::ONE + offset_value / Decimal::from(100)),
        PriceOffsetType::PercentBelow => {
            base_price * (Decimal::ONE - offset_value / Decimal::from(100))
        }
        PriceOffsetType::PercentAbove => {
            base_price * (Decimal::ONE + offset_value / Decimal::from(100))
        }
    })
}

pub(crate) fn balance_percent_quantity(
    percent: Decimal,
    balance: Decimal,
    current_price: Decimal,
    side: trading_core::types::common::Side,
    instrument: &trading_core::types::common::InstrumentType,
) -> Result<Decimal, EngineError> {
    if percent <= Decimal::ZERO {
        return Err(EngineError::CustomError(anyhow::anyhow!(
            "BalancePercent must be strictly positive"
        )));
    }
    let fraction = percent / Decimal::from(100);
    match instrument {
        trading_core::types::common::InstrumentType::Futures
        | trading_core::types::common::InstrumentType::Derivitives => {
            if current_price <= Decimal::ZERO {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "Futures BalancePercent requires a valid price"
                )));
            }
            Ok((balance * fraction) / current_price)
        }
        _ => match side {
            trading_core::types::common::Side::Sell => Ok(balance * fraction),
            trading_core::types::common::Side::Buy => {
                if current_price <= Decimal::ZERO {
                    return Err(EngineError::CustomError(anyhow::anyhow!(
                        "Spot buy BalancePercent requires a valid price"
                    )));
                }
                Ok((balance * fraction) / current_price)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use trading_core::types::common::{InstrumentType, Side};

    #[test]
    fn spot_sell_balance_percent_does_not_divide_by_price() {
        let qty = balance_percent_quantity(
            dec!(50),
            dec!(2),
            dec!(50000),
            Side::Sell,
            &InstrumentType::Spot,
        )
        .unwrap();
        assert_eq!(qty, dec!(1));
    }

    #[test]
    fn spot_buy_balance_percent_converts_quote() {
        let qty = balance_percent_quantity(
            dec!(100),
            dec!(1000),
            dec!(50000),
            Side::Buy,
            &InstrumentType::Spot,
        )
        .unwrap();
        assert_eq!(qty, dec!(1000) / dec!(50000));
    }

    #[test]
    fn futures_balance_percent_uses_collateral_over_price() {
        let qty = balance_percent_quantity(
            dec!(10),
            dec!(1000),
            dec!(50000),
            Side::Sell,
            &InstrumentType::Futures,
        )
        .unwrap();
        assert_eq!(qty, dec!(1000) * dec!(10) / dec!(100) / dec!(50000));
    }
}
