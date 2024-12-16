use super::error::EngineError;
use async_trait::async_trait;
use smol_str::ToSmolStr;
use std::fmt::Debug;

#[async_trait]
pub trait ChannelHandler<T: Send + Sync + Debug + ToSmolStr + 'static> {
    type SenderType;
    type ReceiverType;

    fn create_channel(&self) -> (Self::SenderType, Self::ReceiverType);
    async fn send(&self, sender: &Self::SenderType, message: T) -> Result<(), EngineError>;
    async fn receive(&self, receiver: &mut Self::ReceiverType) -> Result<T, EngineError>;
}
