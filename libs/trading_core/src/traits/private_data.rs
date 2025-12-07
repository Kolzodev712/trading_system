//! Private data client trait (for authenticated streams)
use crate::error::EngineError;
use crate::types::common::Exchange;
use async_trait::async_trait;
use std::fmt::Debug;

/// Trait for authenticated private data streams (execution reports, account updates)
#[async_trait]
pub trait PrivateDataClient: Debug + Send + Sync + Clone + 'static {
    /// Universal identifier of the exchange this client serves
    fn exchange_id(&self) -> Exchange;

    /// Initiate an authenticated WS stream
    async fn stream(&mut self) -> Result<(), EngineError>;

    /// Tell the client to gracefully stop its background task
    fn shutdown(&self);
}
