//! Conversion helpers between internal types and protobuf types
use crate::grpc::trading_cli;
use trading_engine::levels::{config::OrderTypeConfig, level::LevelState, Level};

/// Convert LevelState to string
pub fn level_state_to_string(state: &LevelState) -> &'static str {
    match state {
        LevelState::Created => "Created",
        LevelState::Posted => "Posted",
        LevelState::Filled => "Filled",
        LevelState::Cancelled => "Cancelled",
        LevelState::Paused => "Paused",
        LevelState::Removed => "Removed",
    }
}

/// Convert OrderTypeConfig to string
pub fn order_type_to_string(order_type: &OrderTypeConfig) -> &'static str {
    match order_type {
        OrderTypeConfig::Limit => "Limit",
        OrderTypeConfig::Market => "Market",
        OrderTypeConfig::PostOnly => "PostOnly",
        OrderTypeConfig::IoC => "IoC",
        OrderTypeConfig::FoK => "FoK",
    }
}

/// Format PriceConfig as string
pub fn format_price_config(price: &trading_engine::levels::config::PriceConfig) -> String {
    match price {
        trading_engine::levels::config::PriceConfig::Fixed(d) => format!("Fixed({})", d),
        trading_engine::levels::config::PriceConfig::Indicator {
            indicator_name,
            offset_type,
            offset_value,
        } => format!(
            "Indicator({}, {:?}, {})",
            indicator_name, offset_type, offset_value
        ),
        trading_engine::levels::config::PriceConfig::BestBid { offset } => {
            format!("BestBid({})", offset)
        }
        trading_engine::levels::config::PriceConfig::BestAsk { offset } => {
            format!("BestAsk({})", offset)
        }
        trading_engine::levels::config::PriceConfig::MidPrice { offset_percent } => {
            format!("MidPrice({})", offset_percent)
        }
    }
}

/// Format QuantityConfig as string
pub fn format_quantity_config(quantity: &trading_engine::levels::config::QuantityConfig) -> String {
    match quantity {
        trading_engine::levels::config::QuantityConfig::Fixed(d) => format!("Fixed({})", d),
        trading_engine::levels::config::QuantityConfig::Indicator {
            indicator_name,
            multiplier,
            offset,
        } => format!(
            "Indicator({}, mul:{:?}, off:{:?})",
            indicator_name, multiplier, offset
        ),
        trading_engine::levels::config::QuantityConfig::BalancePercent(p) => {
            format!("BalancePercent({})", p)
        }
        trading_engine::levels::config::QuantityConfig::Notional(n) => {
            format!("Notional({})", n)
        }
    }
}

/// Convert Level to protobuf Level
pub fn level_to_proto(level: &Level) -> trading_cli::Level {
    let status = level_state_to_string(&level.state).to_string();
    let symbol = trading_cli::Symbol {
        base: level.config.symbol.base.as_str().to_string(),
        quote: level.config.symbol.quote.as_str().to_string(),
    };
    let order_type = order_type_to_string(&level.config.order_type).to_string();
    let side = match level.config.side {
        trading_core::types::common::Side::Buy => "Buy",
        trading_core::types::common::Side::Sell => "Sell",
    };

    let price_str = format_price_config(&level.config.price);
    let quantity_str = format_quantity_config(&level.config.quantity);

    let level_config = trading_cli::LevelConfig {
        id: level.config.id.to_string(),
        symbol: Some(symbol),
        side: side.to_string(),
        order_type,
        price: price_str,
        quantity: quantity_str,
        enabled: level.config.enabled,
        risk_limit: level
            .config
            .risk
            .daily_loss_limit
            .map(|d| d.to_string())
            .unwrap_or_default(),
        max_position_size: level
            .config
            .risk
            .max_position
            .map(|d| d.to_string())
            .unwrap_or_default(),
        repost_on_fill: level.config.lifecycle.auto_repost,
        repost_on_cancel: level.config.lifecycle.auto_repost_on_cancel,
    };

    let active_order_ids: Vec<String> = level
        .active_orders
        .iter()
        .map(|id| id.as_str().to_string())
        .collect();

    trading_cli::Level {
        id: level.config.id.to_string(),
        status,
        enabled: level.config.enabled,
        config: Some(level_config),
        active_orders: active_order_ids,
        filled_count: level.repost_count as i32,
    }
}
