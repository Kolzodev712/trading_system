use super::{
    config::BinanceConfig,
    websocket::{
        client::{BinanceChannel, BinanceWSClient},
        types::common::{BinanceWsEventType, ParsedBinanceMessage},
    },
};
use async_trait::async_trait;
use core_logic::{
    channels::{ChannelHandler, ChannelManager},
    error::EngineError,
    traits::PrivateDataClient,
    Exchange,
};
use std::fmt::Debug;
use tracing::{debug, error, info, warn};

// To be removed when client if fixed
use anyhow::{Context, Result};
use reqwest::Client;
async fn create_listen_key(api_key: &str) -> Result<String> {
    let client = Client::new();
    let url = "https://api.binance.com/api/v3/userDataStream";
    let res = client
        .post(url)
        .header("X-MBX-APIKEY", api_key)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let listen_key = res["listenKey"]
        .as_str()
        .context("Missing listenKey in response")?
        .to_string();

    Ok(listen_key)
}

#[derive(Debug, Clone)]
pub struct BinanceClient {
    pub config: BinanceConfig,
    pub channel_manager: ChannelManager,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig, buffer_size: usize) -> Self {
        let channel_manager = ChannelManager::new(buffer_size);
        BinanceClient {
            config,
            channel_manager,
        }
    }

    async fn start_stream(&mut self, data_type: BinanceWsEventType) -> Result<(), EngineError> {
        let (tx, mut rx) = self.channel_manager.create_channel();

        let channel = BinanceChannel {
            channel: data_type.clone(),
            base: self.config.symbol.base.clone().to_lowercase().into(),
            quote: self.config.symbol.quote.clone().to_lowercase().into(),
            limit: self.config.limit.unwrap_or_default(),
        };

        let listen_key = create_listen_key(&self.config.api_key).await.unwrap();

        let client = BinanceWSClient::new(listen_key.into(), channel);
        client.run(tx).await?;
        info!("✅ WebSocket stream started for {:?}", data_type);

        while let Ok(message) = self.channel_manager.receive(&mut rx).await {
            debug!("📩 Received message: {:?}", message);

            match message {
                ParsedBinanceMessage::OutboundAccountPosition(position) => {
                    info!("Account position update: {:?}", position);
                }
                ParsedBinanceMessage::BalanceUpdate(balance) => {
                    info!("Balance update: {:?}", balance);
                }
                ParsedBinanceMessage::ExecutionReport(exec_report) => {
                    info!("Execution report update: {:?}", exec_report);
                }
                ParsedBinanceMessage::Raw(raw) => {
                    info!("Received raw message: {:?}", raw);
                }
                _ => {
                    warn!("❌ Unhandled message type: {:?}", message);
                }
            }
        }

        error!(
            "❌ WebSocket stream unexpectedly stopped for {:?}",
            data_type
        );
        Err(EngineError::ConnectionClosed)
    }
}

#[async_trait]
impl PrivateDataClient for BinanceClient {
    fn exchange_id(&self) -> Exchange {
        Exchange::Binance
    }

    async fn balances(&mut self) -> Result<(), EngineError> {
        self.start_stream(BinanceWsEventType::BalanceUpdate).await
    }
}
