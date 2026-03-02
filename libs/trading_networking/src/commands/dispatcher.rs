//! Command dispatcher: consumes from command router and dispatches to a handler.

use crate::commands::{Command, CommandResponse};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Handler that processes commands and returns a response.
#[async_trait]
pub trait CommandHandler: Send + Sync + 'static {
    /// Handle a command. Return Ok(CommandResponse) or Err(String).
    async fn handle(&self, command: Command) -> Result<CommandResponse, String>;
}

/// Dispatcher that receives commands from the router and calls the handler.
pub struct CommandDispatcher {
    rx: mpsc::Receiver<Command>,
    handler: Box<dyn CommandHandler>,
}

impl CommandDispatcher {
    /// Create a dispatcher with the command receiver and handler.
    pub fn new(rx: mpsc::Receiver<Command>, handler: Box<dyn CommandHandler>) -> Self {
        Self { rx, handler }
    }

    /// Run the dispatcher loop. Returns when the channel is closed.
    pub async fn run(&mut self) {
        while let Some(mut command) = self.rx.recv().await {
            let reply_tx = command.reply.take();
            let result = self.handler.handle(command).await;
            if let Some(tx) = reply_tx {
                let _ = tx.send(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandPayload, CommandRouter, MpscCommandRouter};
    use std::sync::Arc;

    struct EchoHandler;

    #[async_trait]
    impl CommandHandler for EchoHandler {
        async fn handle(&self, command: Command) -> Result<CommandResponse, String> {
            match &command.payload {
                CommandPayload::SetTradingEnabled { enabled } => Ok(CommandResponse {
                    success: true,
                    message: Some(format!("enabled={}", enabled)),
                    data: None,
                }),
                _ => Ok(CommandResponse {
                    success: true,
                    message: None,
                    data: None,
                }),
            }
        }
    }

    #[tokio::test]
    async fn test_dispatcher_reply() {
        let (router, rx) = MpscCommandRouter::new(4);
        let mut dispatcher = CommandDispatcher::new(rx, Box::new(EchoHandler));
        let router = Arc::new(router);
        tokio::spawn(async move {
            dispatcher.run().await;
        });
        let response = router
            .submit_and_wait(CommandPayload::SetTradingEnabled { enabled: true }, 1, None)
            .await
            .unwrap();
        assert!(response.success);
        assert_eq!(response.message.as_deref(), Some("enabled=true"));
    }
}
