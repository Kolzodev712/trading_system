pub mod health;
#[cfg(feature = "zenoh")]
pub mod zenoh;

use crate::events::{ColdEvent, DexColdEvent, HotEvent};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait EventTransport: Send + Sync + 'static {
    async fn publish_hot(&self, event: HotEvent) -> Result<(), String>;
    async fn publish_cold(&self, event: Arc<ColdEvent>) -> Result<(), String>;
    async fn publish_dex_cold(&self, event: Arc<DexColdEvent>) -> Result<(), String>;
}
