//! Level runtime types
use super::config::LevelConfig;
use rust_decimal::Decimal;
use trading_core::types::common::{OrderId, OrderType};

/// Runtime level representation with resolved values
#[derive(Debug, Clone)]
pub struct Level {
    pub config: LevelConfig,

    /// Current state
    pub state: LevelState,

    /// Active orders for this level
    pub active_orders: Vec<OrderId>,

    /// Repost count
    pub repost_count: usize,

    /// Last posted timestamp
    pub last_posted_at: Option<u64>,

    /// Last filled timestamp
    pub last_filled_at: Option<u64>,
}

impl Level {
    /// Create a new level from config
    pub fn new(config: LevelConfig) -> Self {
        Self {
            config,
            state: LevelState::Created,
            active_orders: Vec::new(),
            repost_count: 0,
            last_posted_at: None,
            last_filled_at: None,
        }
    }

    /// Check if level is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if level is ready to post
    pub fn is_ready_to_post(&self) -> bool {
        self.config.enabled
            && matches!(self.state, LevelState::Created)
            && self.active_orders.is_empty()
    }
}

/// Level state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelState {
    /// Level created but not posted
    Created,

    /// Order posted, waiting for fill
    Posted,

    /// Order filled, waiting to repost
    Filled,

    /// Order cancelled, waiting to repost
    Cancelled,

    /// Level paused (manual or condition)
    Paused,

    /// Level removed
    Removed,
}

/// Resolved level with calculated price/quantity
#[derive(Debug, Clone)]
pub struct ResolvedLevel {
    pub level: Level,

    /// Resolved price (from config + indicators)
    pub resolved_price: Decimal,

    /// Resolved quantity (from config + indicators)
    pub resolved_quantity: Decimal,

    /// Resolved order type
    pub resolved_order_type: OrderType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::levels::config::{
        LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
    };
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;
    use trading_core::types::common::{Pair, Side};

    #[test]
    fn test_level_new() {
        let config = LevelConfig {
            id: SmolStr::new("test"),
            name: None,
            symbol: Pair::new("BTC", "USDT"),
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: None,
        };

        let level = Level::new(config);
        assert_eq!(level.state, LevelState::Created);
        assert!(level.active_orders.is_empty());
        assert_eq!(level.repost_count, 0);
    }

    #[test]
    fn test_level_is_ready_to_post() {
        let config = LevelConfig {
            id: SmolStr::new("test"),
            name: None,
            symbol: Pair::new("BTC", "USDT"),
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: None,
        };

        let level = Level::new(config);
        assert!(level.is_ready_to_post());

        let mut disabled_level = level.clone();
        disabled_level.config.enabled = false;
        assert!(!disabled_level.is_ready_to_post());
    }
}
