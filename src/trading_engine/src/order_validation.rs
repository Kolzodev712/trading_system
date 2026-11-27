//! Common order validation and trading rules access
//! Used by both levels and strategies to get exchange-agnostic trading rules

use asset_engine::AssetService;
use rust_decimal::Decimal;
use std::sync::Arc;
use trading_core::error::EngineError;
use trading_core::types::common::{Exchange, Pair};

/// Trading rules with fallback to config values
#[derive(Debug, Clone)]
pub struct TradingRulesWithFallback {
    pub min_order_size: Decimal,
    pub max_order_size: Option<Decimal>,
    pub step_size: Decimal,
    pub min_price: Option<Decimal>,
    pub max_price: Option<Decimal>,
    pub tick_size: Decimal,
    pub min_notional: Decimal,
    pub max_notional: Option<Decimal>,
    pub price_precision: u8,
    pub quantity_precision: u8,
}

impl TradingRulesWithFallback {
    /// Create from Asset Engine data merged with config fallback values
    /// Exchange values take priority, config values used for None fields
    pub fn from_asset_engine_with_fallback(
        rules: &trading_core::types::asset::TradingRules,
        config_min_order_size: Decimal,
        config_step_size: Decimal,
        config_tick_size: Decimal,
        config_min_notional: Decimal,
    ) -> Self {
        Self {
            min_order_size: rules.min_order_size.unwrap_or(config_min_order_size),
            max_order_size: rules.max_order_size,
            step_size: rules.step_size.unwrap_or(config_step_size),
            min_price: rules.min_price,
            max_price: rules.max_price,
            tick_size: rules.tick_size.unwrap_or(config_tick_size),
            min_notional: rules.min_notional.unwrap_or(config_min_notional),
            max_notional: rules.max_notional,
            price_precision: rules.price_precision.unwrap_or(8),
            quantity_precision: rules.quantity_precision.unwrap_or(8),
        }
    }

    /// Create from config values only (fallback when exchange data unavailable)
    pub fn from_config(
        min_order_size: Decimal,
        step_size: Decimal,
        tick_size: Decimal,
        min_notional: Decimal,
    ) -> Self {
        Self {
            min_order_size,
            max_order_size: None,
            step_size,
            min_price: None,
            max_price: None,
            tick_size,
            min_notional,
            max_notional: None,
            price_precision: 8,    // Default precision
            quantity_precision: 8, // Default precision
        }
    }
}

/// Get trading rules from Asset Engine with config fallback
/// Exchange values take priority, config values used for None fields
pub async fn get_trading_rules(
    asset_service: Option<&Arc<AssetService>>,
    exchange: Exchange,
    pair: &Pair,
    config_min_order_size: Decimal,
    config_step_size: Decimal,
    config_tick_size: Decimal,
    config_min_notional: Decimal,
) -> TradingRulesWithFallback {
    // Try Asset Engine first (source of truth)
    if let Some(asset_service) = asset_service {
        if let Some(rules) = asset_service.get_trading_rules(exchange, pair).await {
            tracing::debug!(
                "Using Asset Engine trading rules for {} (with config fallback for None fields)",
                pair
            );
            return TradingRulesWithFallback::from_asset_engine_with_fallback(
                &rules,
                config_min_order_size,
                config_step_size,
                config_tick_size,
                config_min_notional,
            );
        }
    }

    // Fallback to config values only
    tracing::debug!(
        "Asset Engine data not available for {}, using config values",
        pair
    );
    TradingRulesWithFallback::from_config(
        config_min_order_size,
        config_step_size,
        config_tick_size,
        config_min_notional,
    )
}

/// Round price to tick size
pub fn round_price(price: Decimal, tick_size: Decimal) -> Decimal {
    if tick_size > Decimal::ZERO {
        (price / tick_size).floor() * tick_size
    } else {
        price
    }
}

/// Round quantity to step size
pub fn round_quantity(quantity: Decimal, step_size: Decimal) -> Decimal {
    if step_size > Decimal::ZERO {
        (quantity / step_size).floor() * step_size
    } else {
        quantity
    }
}

/// Validate order against trading rules
pub fn validate_order(
    price: Decimal,
    quantity: Decimal,
    rules: &TradingRulesWithFallback,
) -> Result<(), EngineError> {
    // Check min order size
    if quantity < rules.min_order_size {
        return Err(EngineError::CustomError(smol_str::format_smolstr!(
            "Order quantity {} is below minimum {}",
            quantity,
            rules.min_order_size
        )));
    }

    // Check max order size
    if let Some(max_size) = rules.max_order_size {
        if quantity > max_size {
            return Err(EngineError::CustomError(smol_str::format_smolstr!(
                "Order quantity {} exceeds maximum {}",
                quantity,
                max_size
            )));
        }
    }

    // Check min price
    if let Some(min_price) = rules.min_price {
        if price < min_price {
            return Err(EngineError::CustomError(smol_str::format_smolstr!(
                "Order price {} is below minimum {}",
                price,
                min_price
            )));
        }
    }

    // Check max price
    if let Some(max_price) = rules.max_price {
        if price > max_price {
            return Err(EngineError::CustomError(smol_str::format_smolstr!(
                "Order price {} exceeds maximum {}",
                price,
                max_price
            )));
        }
    }

    // Check min notional
    let notional = price * quantity;
    if notional < rules.min_notional {
        return Err(EngineError::CustomError(smol_str::format_smolstr!(
            "Order notional {} is below minimum {} (price: {}, quantity: {})",
            notional,
            rules.min_notional,
            price,
            quantity
        )));
    }

    // Check max notional
    if let Some(max_notional) = rules.max_notional {
        if notional > max_notional {
            return Err(EngineError::CustomError(smol_str::format_smolstr!(
                "Order notional {} exceeds maximum {}",
                notional,
                max_notional
            )));
        }
    }

    Ok(())
}
