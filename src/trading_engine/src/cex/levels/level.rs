//! Level runtime types
use super::config::LevelConfig;
use super::identity::LevelIdentity;
use rust_decimal::Decimal;
use trading_core::types::common::{OrderId, OrderType, Pair};

/// Runtime level representation with resolved values
#[derive(Debug, Clone)]
pub struct Level {
    pub config: LevelConfig,

    /// Trading pair this level instance is for
    pub pair: Pair,

    pub identity: LevelIdentity,

    /// Current state
    pub state: LevelState,

    /// Active orders for this level
    pub active_orders: Vec<OrderId>,

    /// Client order IDs owned by this instance (durable with catalog instances).
    pub owned_client_ids: Vec<smol_str::SmolStr>,

    /// Completed fill quantity owned by this instance (survives working-order retirement).
    pub filled_qty: Decimal,

    /// Terminal fills observed for this instance (not repost_count).
    pub fill_count: usize,

    /// Bumped on disable/delete so an in-flight submit can refuse to send.
    pub generation: u64,

    /// Repost count
    pub repost_count: usize,

    /// Last posted timestamp
    pub last_posted_at: Option<u64>,

    /// Last filled timestamp
    pub last_filled_at: Option<u64>,
}

impl Level {
    /// Create a new level from config and pair (test / Paper-spot identity).
    pub fn new(config: LevelConfig, pair: Pair) -> Self {
        let identity = LevelIdentity::for_tests(pair.clone(), config.id.clone());
        Self::from_identity(config, identity)
    }

    pub fn from_identity(config: LevelConfig, identity: LevelIdentity) -> Self {
        Self {
            config,
            pair: identity.pair.clone(),
            identity,
            state: LevelState::Created,
            active_orders: Vec::new(),
            owned_client_ids: Vec::new(),
            filled_qty: Decimal::ZERO,
            fill_count: 0,
            generation: 0,
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

    /// Persist-before-send in progress; not eligible to post another order
    Submitting,

    /// Order filled, waiting to repost
    Filled,

    /// Order cancelled, waiting to repost
    Cancelled,

    /// Level paused (manual or condition)
    Paused,

    /// Delete latched; owned orders not REST-verified gone yet.
    Deleting,

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
    use crate::cex::levels::config::{
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
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: None,
        };

        let pair = Pair::new("BTC", "USDT");
        let level = Level::new(config, pair);
        assert_eq!(level.state, LevelState::Created);
        assert!(level.active_orders.is_empty());
        assert_eq!(level.repost_count, 0);
    }

    #[test]
    fn test_level_is_ready_to_post() {
        let config = LevelConfig {
            id: SmolStr::new("test"),
            name: None,
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: None,
        };

        let pair = Pair::new("BTC", "USDT");
        let level = Level::new(config, pair);
        assert!(level.is_ready_to_post());

        let mut disabled_level = level.clone();
        disabled_level.config.enabled = false;
        assert!(!disabled_level.is_ready_to_post());
    }
}
