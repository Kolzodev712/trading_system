use async_trait::async_trait;
use core_logic::{channels::ChannelHandler, error::EngineError};
use smol_str::ToSmolStr;
use std::fmt::Debug;
use tokio::sync::mpsc::{self, Receiver, Sender};

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
