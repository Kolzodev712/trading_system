//! Channel-based API for LiveConfig - simple message passing without HTTP
use crate::service::LiveConfigService;
use crate::types::*;
use tokio::sync::{mpsc, oneshot};

/// Request message for LiveConfig updates with response channel
#[derive(Debug)]
pub enum LiveConfigRequest {
    UpdateLevel {
        request: LevelUpdateRequest,
        response_tx: oneshot::Sender<Result<UpdateResponse, String>>,
    },
    UpdateRiskLimit {
        request: RiskLimitUpdateRequest,
        response_tx: oneshot::Sender<Result<UpdateResponse, String>>,
    },
    UpdateConfig {
        request: ConfigUpdateRequest,
        response_tx: oneshot::Sender<Result<UpdateResponse, String>>,
    },
}

/// Channel-based API handler for LiveConfig
pub struct LiveConfigApi {
    service: LiveConfigService,
    request_rx: mpsc::Receiver<LiveConfigRequest>,
}

impl LiveConfigApi {
    /// Create a new channel-based API
    pub fn new(service: LiveConfigService, request_rx: mpsc::Receiver<LiveConfigRequest>) -> Self {
        Self {
            service,
            request_rx,
        }
    }

    /// Run the API handler (processes requests from channel)
    pub async fn run(mut self) {
        while let Some(request) = self.request_rx.recv().await {
            match request {
                LiveConfigRequest::UpdateLevel {
                    request,
                    response_tx,
                } => {
                    let result = self
                        .service
                        .update_level_value(request)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = response_tx.send(result);
                }
                LiveConfigRequest::UpdateRiskLimit {
                    request,
                    response_tx,
                } => {
                    let result = self
                        .service
                        .update_risk_limit(request)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = response_tx.send(result);
                }
                LiveConfigRequest::UpdateConfig {
                    request,
                    response_tx,
                } => {
                    let result = self
                        .service
                        .update_bot_config(request)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = response_tx.send(result);
                }
            }
        }
    }
}

/// Client for sending requests to LiveConfig API
pub struct LiveConfigClient {
    request_tx: mpsc::Sender<LiveConfigRequest>,
}

impl LiveConfigClient {
    /// Create a new client (returns client and API handler)
    pub fn new(service: LiveConfigService, request_capacity: usize) -> (Self, LiveConfigApi) {
        let (request_tx, request_rx) = mpsc::channel(request_capacity);

        let client = Self { request_tx };

        let api = LiveConfigApi::new(service, request_rx);

        (client, api)
    }

    /// Update a level value
    pub async fn update_level(
        &self,
        request: LevelUpdateRequest,
    ) -> Result<UpdateResponse, String> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send(LiveConfigRequest::UpdateLevel {
                request,
                response_tx,
            })
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        response_rx
            .await
            .map_err(|_| "Response channel closed".to_string())?
    }

    /// Update a risk limit
    pub async fn update_risk_limit(
        &self,
        request: RiskLimitUpdateRequest,
    ) -> Result<UpdateResponse, String> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send(LiveConfigRequest::UpdateRiskLimit {
                request,
                response_tx,
            })
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        response_rx
            .await
            .map_err(|_| "Response channel closed".to_string())?
    }

    /// Update bot configuration
    pub async fn update_config(
        &self,
        request: ConfigUpdateRequest,
    ) -> Result<UpdateResponse, String> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send(LiveConfigRequest::UpdateConfig {
                request,
                response_tx,
            })
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        response_rx
            .await
            .map_err(|_| "Response channel closed".to_string())?
    }
}

// Implement Clone for client (only the sender is cloned, which is fine)
impl Clone for LiveConfigClient {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
        }
    }
}
