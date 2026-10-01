use anyhow::Result;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::types::Capability;

/// Test-only venue adapter. Production crates must not implement this.
#[async_trait::async_trait]
pub trait VenueHarness: Send {
    fn venue_name(&self) -> &'static str;
    fn capabilities(&self) -> Capability;
    async fn snapshot(&self) -> Result<RunnerSnapshot>;
    async fn rest_orders(&self) -> Result<Vec<RestOrder>>;
    async fn rest_balances(&self) -> Result<Vec<RestBalance>>;
    async fn rest_positions(&self) -> Result<Vec<RestPosition>>;
}

#[derive(Debug, Clone)]
pub struct RestOrder {
    pub order_id: String,
    pub client_order_id: String,
    pub instrument: String,
    pub price: Decimal,
    pub amount: Decimal,
    pub filled: Decimal,
    pub status: String,
    pub side: String,
}

#[derive(Debug, Clone)]
pub struct RestBalance {
    pub asset: String,
    pub total: Decimal,
}

#[derive(Debug, Clone)]
pub struct RestPosition {
    pub instrument: String,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerSnapshot {
    pub venue: String,
    pub environment: String,
    pub execution_mode: String,
    pub trading_enabled: bool,
    pub entries_paused: bool,
    pub flatten_latched: bool,
    pub positions: Vec<(String, Decimal, String)>,
    pub orders: Vec<(String, String, String)>,
    pub levels: Vec<(String, bool, String)>,
    /// False when GetLevels failed. An empty `levels` vec is not proof they are disabled.
    pub levels_verified: bool,
    pub exposure_halt_reason: String,
    pub last_block_reason_code: String,
    pub avg_entry: String,
}
