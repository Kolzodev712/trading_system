//! Resolve quote prices from orderbooks and indicator values.

use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::collections::HashMap;
use trading_core::types::market_data::StructuredOrderbook;
use trading_core::venue::dex::DexError;

use super::config::DexQuotePricingRegistry;
use super::types::{DexIndicatorValue, DexPriceConfig, DexPricingOffsetType};

pub fn resolve_quote_price(
    quote_id: &str,
    pool: &str,
    price: &DexPriceConfig,
    quote_pricing: &DexQuotePricingRegistry,
    orderbooks: &HashMap<SmolStr, StructuredOrderbook>,
    indicators: &HashMap<SmolStr, DexIndicatorValue>,
) -> Result<Decimal, DexError> {
    match price {
        DexPriceConfig::Fixed(d) => Ok(*d),
        DexPriceConfig::Binding => {
            let rule = quote_pricing.get(quote_id).ok_or_else(|| {
                DexError::InvalidConfig(
                    format!(
                        "price Binding but no quote_pricing for quote_id {quote_id} in config/dex/indicators.json"
                    )
                    .into(),
                )
            })?;
            let indicator_name = SmolStr::new(&rule.indicator_ref);
            let base = indicator_value(&indicator_name, indicators).ok_or_else(|| {
                DexError::InvalidConfig(
                    format!(
                        "indicator `{}` not ready for quote {quote_id}",
                        rule.indicator_ref
                    )
                    .into(),
                )
            })?;
            apply_offset(base, rule.offset_type, rule.offset_value)
        }
        DexPriceConfig::Mid { offset_percent } => {
            let mid = orderbook_for_pool(pool, orderbooks)?
                .mid_price()
                .ok_or_else(|| DexError::InvalidConfig(format!("no mid for pool {pool}").into()))?;
            Ok(mid * (Decimal::ONE + *offset_percent / Decimal::from(100)))
        }
        DexPriceConfig::BestBid { offset } => {
            let bid = orderbook_for_pool(pool, orderbooks)?
                .best_bid_price()
                .ok_or_else(|| DexError::InvalidConfig(format!("no bid for pool {pool}").into()))?;
            Ok(bid - *offset)
        }
        DexPriceConfig::BestAsk { offset } => {
            let ask = orderbook_for_pool(pool, orderbooks)?
                .best_ask_price()
                .ok_or_else(|| DexError::InvalidConfig(format!("no ask for pool {pool}").into()))?;
            Ok(ask + *offset)
        }
    }
}

fn orderbook_for_pool<'a>(
    pool: &str,
    orderbooks: &'a HashMap<SmolStr, StructuredOrderbook>,
) -> Result<&'a StructuredOrderbook, DexError> {
    orderbooks.get(&SmolStr::new(pool)).ok_or_else(|| {
        DexError::InvalidConfig(format!("no orderbook cached for pool {pool}").into())
    })
}

fn indicator_value(
    indicator_ref: &SmolStr,
    indicators: &HashMap<SmolStr, DexIndicatorValue>,
) -> Option<Decimal> {
    indicators.get(indicator_ref).map(|v| v.value).or_else(|| {
        indicators
            .values()
            .find(|v| v.name.as_str() == indicator_ref.as_str())
            .map(|v| v.value)
    })
}

fn apply_offset(
    base: Decimal,
    offset_type: DexPricingOffsetType,
    offset_value: Decimal,
) -> Result<Decimal, DexError> {
    let resolved = match offset_type {
        DexPricingOffsetType::Absolute => base + offset_value,
        DexPricingOffsetType::Percent => base * (Decimal::ONE + offset_value / Decimal::from(100)),
        DexPricingOffsetType::PercentBelow => {
            base * (Decimal::ONE - offset_value / Decimal::from(100))
        }
        DexPricingOffsetType::PercentAbove => {
            base * (Decimal::ONE + offset_value / Decimal::from(100))
        }
    };
    if resolved <= Decimal::ZERO {
        return Err(DexError::InvalidConfig(
            format!("resolved price must be positive, got {resolved}").into(),
        ));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex::indicators::config::DexQuotePricingRule;
    use rust_decimal_macros::dec;
    use trading_core::types::common::Pair;
    use trading_core::types::market_data::StructuredOrderbook;

    fn sample_book() -> StructuredOrderbook {
        StructuredOrderbook {
            pair: Pair::new("SUI", "DBUSDC"),
            bids: vec![(dec!(3.0), dec!(1.0))],
            asks: vec![(dec!(3.2), dec!(1.0))],
            timestamp: 1,
            update_kind: Default::default(),
            sequence: None,
        }
    }

    #[test]
    fn resolves_mid_with_offset() {
        let mut books = HashMap::new();
        books.insert(SmolStr::new("SUI_DBUSDC"), sample_book());
        let price = DexPriceConfig::Mid {
            offset_percent: dec!(0),
        };
        let resolved = resolve_quote_price(
            "bid_1",
            "SUI_DBUSDC",
            &price,
            &DexQuotePricingRegistry::default(),
            &books,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(resolved, dec!(3.1));
    }

    #[test]
    fn resolves_binding_from_indicator_map() {
        let mut indicators = HashMap::new();
        indicators.insert(
            SmolStr::new("mid_sui"),
            DexIndicatorValue {
                indicator_id: SmolStr::new("mid_sui"),
                name: SmolStr::new("Mid"),
                pool: SmolStr::new("SUI_DBUSDC"),
                value: dec!(3.1),
                timestamp: 1,
            },
        );
        let pricing = DexQuotePricingRegistry::from_rules(vec![DexQuotePricingRule {
            quote_id: "bid_1".into(),
            indicator_ref: "mid_sui".into(),
            offset_type: DexPricingOffsetType::PercentBelow,
            offset_value: dec!(1),
        }])
        .unwrap();
        let resolved = resolve_quote_price(
            "bid_1",
            "SUI_DBUSDC",
            &DexPriceConfig::Binding,
            &pricing,
            &HashMap::new(),
            &indicators,
        )
        .unwrap();
        assert_eq!(resolved, dec!(3.069));
    }
}
