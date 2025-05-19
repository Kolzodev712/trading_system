use super::{
    data::public_data::{Orderbook, Trade},
    error::EngineError,
};

use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use tracing::debug;

pub const ORDERBOOK_CHANNEL: &str = "orderbook";
pub const TRADE_CHANNEL: &str = "trade";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum RedisMessage {
    OrderbookMessage(Orderbook),
    TradeMessage(Trade),
}

#[derive(Debug, Clone)]
pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    pub fn new(url: &str) -> Result<Self, EngineError> {
        let client = redis::Client::open(url)?;
        Ok(Self { client })
    }

    pub async fn publish(&self, channel: &str, message: RedisMessage) -> Result<(), EngineError> {
        let mut connection = self.client.get_multiplexed_tokio_connection().await?;

        let serialized_message = serde_json::to_string(&message)?;

        let _: () = connection.publish(channel, serialized_message).await?;

        Ok(())
    }

    pub async fn process_stream<F>(&self, channel: &str, mut handler: F) -> Result<(), EngineError>
    where
        F: FnMut(RedisMessage) -> Result<(), EngineError> + Send + 'static,
    {
        let mut pubsub_conn = self.client.get_async_pubsub().await?;
        pubsub_conn.subscribe(channel).await?;

        let mut stream = pubsub_conn.on_message();
        while let Some(message) = stream.next().await {
            let payload: String = message.get_payload().map_err(EngineError::RedisError)?;
            let redis_message: RedisMessage =
                serde_json::from_str(&payload).map_err(EngineError::JsonError)?;

            debug!("Received message on {}: {:?}", channel, redis_message);

            if let Err(e) = handler(redis_message) {
                debug!("Error in message handler: {}", e);
            }
        }

        Ok(())
    }
}
