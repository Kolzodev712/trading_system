use crate::error::NetworkError;
use crate::events::{ColdEvent, DexColdEvent, HotEvent};
use crate::gateway::transports::EventTransport;
use crate::zenoh_bridge;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ZenohKeyspace {
    pub prefix: String,
    pub commands_key: String,
}

impl Default for ZenohKeyspace {
    fn default() -> Self {
        Self {
            prefix: "trading".to_string(),
            commands_key: "trading/cmd".to_string(),
        }
    }
}

impl ZenohKeyspace {
    pub fn hot_key(&self, event: &HotEvent) -> String {
        match event {
            HotEvent::Orderbook { pair, .. } => {
                format!(
                    "{}/hot/orderbook/{}",
                    self.prefix,
                    pair.convert_to_smolstr()
                )
            }
            HotEvent::Trade { pair, .. } => {
                format!("{}/hot/trade/{}", self.prefix, pair.convert_to_smolstr())
            }
        }
    }

    pub fn cold_key(&self, event: &ColdEvent) -> &'static str {
        match event {
            ColdEvent::Order { .. } => "cold/order",
            ColdEvent::Balance { .. } => "cold/balance",
            ColdEvent::Position { .. } => "cold/position",
            ColdEvent::Trade { .. } => "cold/trade",
        }
    }

    pub fn cold_full_key(&self, event: &ColdEvent) -> String {
        format!(
            "{}/{}/",
            self.prefix,
            self.cold_key(event).trim_start_matches('/')
        )
    }

    pub fn dex_cold_key(&self, event: &DexColdEvent) -> &'static str {
        match event {
            DexColdEvent::WalletBalance { .. } => "dex_cold/wallet_balance",
            DexColdEvent::OpenOrders { .. } => "dex_cold/open_orders",
            DexColdEvent::PendingTransaction { .. } => "dex_cold/pending_transaction",
            DexColdEvent::ExecutionResult { .. } => "dex_cold/execution_result",
        }
    }
}

#[derive(Clone)]
pub struct ZenohEventTransport {
    session: zenoh::Session,
    keyspace: ZenohKeyspace,
}

impl ZenohEventTransport {
    pub fn new(session: zenoh::Session, keyspace: ZenohKeyspace) -> Self {
        Self { session, keyspace }
    }
}

#[async_trait]
impl EventTransport for ZenohEventTransport {
    async fn publish_hot(&self, event: HotEvent) -> Result<(), String> {
        let key = self.keyspace.hot_key(&event);
        zenoh_bridge::publish_hot_event(&self.session, &key, &event)
            .await
            .map_err(|e| e.to_string())
    }

    async fn publish_cold(&self, event: Arc<ColdEvent>) -> Result<(), String> {
        let key = format!(
            "{}/{}",
            self.keyspace.prefix,
            self.keyspace.cold_key(&event)
        );
        zenoh_bridge::publish_cold_event(&self.session, &key, event.as_ref())
            .await
            .map_err(|e| e.to_string())
    }

    async fn publish_dex_cold(&self, event: Arc<DexColdEvent>) -> Result<(), String> {
        let key = format!(
            "{}/{}",
            self.keyspace.prefix,
            self.keyspace.dex_cold_key(&event)
        );
        zenoh_bridge::publish_dex_cold_event(&self.session, &key, event.as_ref())
            .await
            .map_err(|e| e.to_string())
    }
}

pub async fn open_default_session() -> Result<zenoh::Session, NetworkError> {
    let cfg = zenoh::Config::default();
    zenoh_bridge::open_session(cfg).await
}
