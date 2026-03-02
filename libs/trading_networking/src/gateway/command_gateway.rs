use crate::commands::{CommandPayload, CommandResponse, CommandRouter};
use crate::error::NetworkError;
use crate::zenoh_bridge::{run_command_queryable, CommandQueryHandler};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct ZenohCommandGateway<R> {
    router: Arc<R>,
    correlation: AtomicU64,
}

impl<R> ZenohCommandGateway<R> {
    pub fn new(router: Arc<R>) -> Self {
        Self {
            router,
            correlation: AtomicU64::new(1),
        }
    }
}

#[async_trait]
impl<R> CommandQueryHandler for ZenohCommandGateway<R>
where
    R: CommandRouter,
{
    async fn handle(&self, payload: CommandPayload) -> Result<CommandResponse, String> {
        let id = self.correlation.fetch_add(1, Ordering::Relaxed);
        self.router
            .submit_and_wait(payload, id, None)
            .await
            .map_err(|e| e.to_string())
    }
}

pub async fn serve_zenoh_commands<R>(
    session: &zenoh::Session,
    key_expr: &str,
    router: Arc<R>,
) -> Result<(), NetworkError>
where
    R: CommandRouter,
{
    let handler = Arc::new(ZenohCommandGateway::new(router));
    run_command_queryable(session, key_expr, handler).await
}
