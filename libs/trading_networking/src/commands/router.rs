//! Command router: single-consumer MPSC channel with optional request/response.

use crate::commands::{Command, CommandPayload, CommandResponse};
use std::future::Future;
use tokio::sync::{mpsc, oneshot};

/// Command router trait (single-consumer, routes to handlers).
pub trait CommandRouter: Send + Sync + 'static {
    /// Submit a command (non-blocking). Reply is sent via command.reply if present.
    fn submit(&self, command: Command) -> Result<(), CommandRouterError>;

    /// Submit a command and wait for result.
    fn submit_and_wait(
        &self,
        payload: CommandPayload,
        correlation_id: u64,
        trace_context: Option<trading_trace::ColdTraceContext>,
    ) -> impl Future<Output = Result<CommandResponse, CommandRouterError>> + Send;
}

/// Command router implementation (MPSC channel).
#[derive(Debug, Clone)]
pub struct MpscCommandRouter {
    tx: mpsc::Sender<Command>,
}

impl MpscCommandRouter {
    /// Create a new router and the receiver (to be passed to CommandDispatcher).
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }
}

impl CommandRouter for MpscCommandRouter {
    fn submit(&self, command: Command) -> Result<(), CommandRouterError> {
        self.tx.try_send(command).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => CommandRouterError::ChannelFull,
            mpsc::error::TrySendError::Closed(_) => CommandRouterError::ChannelClosed,
        })
    }

    async fn submit_and_wait(
        &self,
        payload: CommandPayload,
        correlation_id: u64,
        trace_context: Option<trading_trace::ColdTraceContext>,
    ) -> Result<CommandResponse, CommandRouterError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = Command {
            correlation_id,
            payload,
            reply: Some(reply_tx),
            trace_context,
        };
        self.tx
            .send(command)
            .await
            .map_err(|_| CommandRouterError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| CommandRouterError::HandlerDropped)?
            .map_err(CommandRouterError::ExecutionFailed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandRouterError {
    #[error("command channel full")]
    ChannelFull,
    #[error("command channel closed")]
    ChannelClosed,
    #[error("handler dropped reply")]
    HandlerDropped,
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("timeout")]
    Timeout,
}
