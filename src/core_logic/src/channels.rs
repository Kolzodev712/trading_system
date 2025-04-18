use super::error::EngineError;
use async_trait::async_trait;
use smol_str::ToSmolStr;
use std::fmt::Debug;
use tokio::sync::mpsc::{self, Receiver, Sender};

// THIS IS COMPLETE CRAP and if it's to be used, it needs to be re-done! 

// it needs to offer the creation and management of mpsc, watch, oneshot, broadcast and also the message flow management

#[async_trait]
pub trait ChannelHandler<T: Send + Sync + Debug + 'static> {
    type SenderType;
    type ReceiverType;

    fn create_channel(&self) -> (Self::SenderType, Self::ReceiverType);
    async fn send(&self, sender: &Self::SenderType, message: T) -> Result<(), EngineError>;
    async fn receive(&self, receiver: &mut Self::ReceiverType) -> Result<T, EngineError>;
}
#[derive(Debug, Clone)]
pub struct ChannelManager {
    buffer_size: usize,
}

impl ChannelManager {
    pub fn new(buffer_size: usize) -> Self {
        Self { buffer_size }
    }

    pub fn buffer(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size
    }
}

#[async_trait]
impl<T: Send + Sync + Debug + ToSmolStr + 'static> ChannelHandler<T> for ChannelManager {
    type SenderType = Sender<T>;
    type ReceiverType = Receiver<T>;

    fn create_channel(&self) -> (Self::SenderType, Self::ReceiverType) {
        mpsc::channel(self.buffer_size)
    }

    async fn send(&self, sender: &Self::SenderType, message: T) -> Result<(), EngineError> {
        sender
            .send(message)
            .await
            .map_err(|e| EngineError::ChannelSendError(e.0.to_smolstr()))
    }

    async fn receive(&self, receiver: &mut Self::ReceiverType) -> Result<T, EngineError> {
        receiver
            .recv()
            .await
            .ok_or_else(|| EngineError::ChannelClosedError)
    }
}
