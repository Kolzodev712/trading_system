//! Conversion helpers between internal types and protobuf types
use crate::grpc::trading_cli;
use trading_engine::levels::{config::OrderTypeConfig, level::LevelState, Level};

pub fn level_state_to_string(state: &LevelState) -> &'static str {
    match state {
        LevelState::Created => "Created",
        LevelState::Posted => "Posted",
        LevelState::Filled => "Filled",
        LevelState::Cancelled => "Cancelled",
        LevelState::Paused => "Paused",
        LevelState::Removed => "Removed",
        LevelState::Submitting => "Submitting",
        LevelState::Deleting => "Deleting",
    }
}

pub fn order_type_to_string(order_type: &OrderTypeConfig) -> &'static str {
    match order_type {
        OrderTypeConfig::Limit => "Limit",
        OrderTypeConfig::Market => "Market",
        OrderTypeConfig::PostOnly => "PostOnly",
        OrderTypeConfig::IoC => "IoC",
        OrderTypeConfig::FoK => "FoK",
    }
}

pub fn format_price_config(price: &trading_engine::levels::config::PriceConfig) -> String {
    match price {
        trading_engine::levels::config::PriceConfig::Fixed(d) => format!("Fixed({})", d),
        trading_engine::levels::config::PriceConfig::Binding => {
            "Binding(indicators_config)".to_string()
        }
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

pub fn format_quantity_config(quantity: &trading_engine::levels::config::QuantityConfig) -> String {
    match quantity {
        trading_engine::levels::config::QuantityConfig::Fixed(d) => format!("Fixed({})", d),
        trading_engine::levels::config::QuantityConfig::BalancePercent(p) => {
            format!("BalancePercent({})", p)
        }
        trading_engine::levels::config::QuantityConfig::Notional(n) => format!("Notional({})", n),
    }
}

pub fn level_to_proto(level: &Level) -> trading_cli::Level {
    let status = level_state_to_string(&level.state).to_string();
    let symbol = trading_cli::Symbol {
        base: level.pair.base.as_str().to_string(),
        quote: level.pair.quote.as_str().to_string(),
    };
    let order_type = order_type_to_string(&level.config.order_type).to_string();
    let side = match level.config.side {
        trading_core::types::common::Side::Buy => "Buy",
        trading_core::types::common::Side::Sell => "Sell",
    };

    let price_str = format_price_config(&level.config.price);
    let quantity_str = format_quantity_config(&level.config.quantity);

    let pair_str = level.pair.convert_to_smolstr();
    let expanded_id = format!("{}_{}", level.config.id, pair_str);

    let level_config = trading_cli::LevelConfig {
        id: level.config.id.to_string(),
        symbol: Some(symbol),
        side: side.to_string(),
        order_type,
        price: price_str,
        quantity: quantity_str,
        enabled: level.config.enabled,
        risk_limit: String::new(),
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
        id: expanded_id,
        status,
        enabled: level.config.enabled,
        config: Some(level_config),
        active_orders: active_order_ids,
        filled_count: level.fill_count as i32,
    }
}
