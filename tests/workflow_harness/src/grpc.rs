use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::time::Duration;
use tonic::transport::Channel;
use tonic::Request;

pub mod trading_cli {
    tonic::include_proto!("trading_cli");
}

use trading_cli::trading_cli_service_client::TradingCliServiceClient;
use trading_cli::{
    DeleteLevelRequest, GetLevelRequest, GetLevelsRequest, GetMonitoringDataRequest,
    GetTradingStatusRequest, OperatorCommandRequest, SetTradingEnabledRequest, ToggleLevelRequest,
    ToggleStrategyRequest, UpdateLevelRequest,
};

use crate::venue::RunnerSnapshot;

pub struct GrpcCtl {
    inner: TradingCliServiceClient<Channel>,
}

impl GrpcCtl {
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
            .with_context(|| format!("grpc connect {endpoint}"))?;
        Ok(Self {
            inner: TradingCliServiceClient::new(channel),
        })
    }

    pub async fn enable(&mut self) -> Result<()> {
        let resp = self
            .inner
            .set_trading_enabled(Request::new(SetTradingEnabledRequest { enabled: true }))
            .await?
            .into_inner();
        if !resp.success {
            anyhow::bail!("enable failed: {}", resp.message);
        }
        Ok(())
    }

    pub async fn toggle_level(&mut self, level_id: &str, enabled: bool) -> Result<()> {
        let resp = self
            .inner
            .toggle_level(Request::new(ToggleLevelRequest {
                level_id: level_id.into(),
                enabled,
            }))
            .await?
            .into_inner();
        if !resp.success {
            anyhow::bail!("toggle-level {level_id} failed");
        }
        Ok(())
    }

    pub async fn delete_level(&mut self, level_id: &str) -> Result<()> {
        let resp = self
            .inner
            .delete_level(Request::new(DeleteLevelRequest {
                level_id: level_id.into(),
            }))
            .await?
            .into_inner();
        if !resp.success {
            anyhow::bail!("delete-level {level_id} failed");
        }
        Ok(())
    }

    pub async fn update_level_price(&mut self, level_id: &str, price: &str) -> Result<()> {
        let current = self
            .inner
            .get_level(Request::new(GetLevelRequest {
                level_id: level_id.into(),
            }))
            .await?
            .into_inner()
            .level
            .and_then(|l| l.config)
            .ok_or_else(|| anyhow::anyhow!("level config missing"))?;
        let mut config = current;
        config.price = format!("Fixed({price})");
        let resp = self
            .inner
            .update_level(Request::new(UpdateLevelRequest {
                level_id: level_id.into(),
                config: Some(config),
            }))
            .await?
            .into_inner();
        if !resp.success {
            anyhow::bail!("update-level {level_id} failed");
        }
        Ok(())
    }

    pub async fn pause_entries(&mut self) -> Result<()> {
        let resp = self
            .inner
            .pause_entries(Request::new(OperatorCommandRequest {
                request_id: "wf-pause".into(),
                symbol: String::new(),
            }))
            .await?
            .into_inner();
        if !resp.accepted {
            anyhow::bail!("pause_entries not accepted: {}", resp.message);
        }
        Ok(())
    }

    pub async fn resume_entries(&mut self) -> Result<()> {
        let resp = self
            .inner
            .resume(Request::new(OperatorCommandRequest {
                request_id: "wf-resume".into(),
                symbol: String::new(),
            }))
            .await?
            .into_inner();
        if !resp.accepted {
            anyhow::bail!("resume not accepted: {}", resp.message);
        }
        Ok(())
    }

    pub async fn flatten_and_pause(&mut self, request_id: &str) -> Result<String> {
        let resp = self
            .inner
            .flatten_and_pause(Request::new(OperatorCommandRequest {
                request_id: request_id.into(),
                symbol: String::new(),
            }))
            .await?
            .into_inner();
        if !resp.accepted {
            anyhow::bail!("flatten_and_pause not accepted: {}", resp.message);
        }
        Ok(resp.message)
    }

    pub async fn toggle_strategy(&mut self, name: &str, symbol: &str, enabled: bool) -> Result<()> {
        let resp = self
            .inner
            .toggle_strategy(Request::new(ToggleStrategyRequest {
                strategy_name: name.into(),
                symbol: symbol.into(),
                enabled,
            }))
            .await?
            .into_inner();
        if !resp.success {
            anyhow::bail!("toggle_strategy {name} failed: {}", resp.message);
        }
        Ok(())
    }

    pub async fn snapshot(&mut self) -> Result<RunnerSnapshot> {
        let status = self
            .inner
            .get_trading_status(Request::new(GetTradingStatusRequest {}))
            .await?
            .into_inner();
        let data = self
            .inner
            .get_monitoring_data(Request::new(GetMonitoringDataRequest {}))
            .await?
            .into_inner()
            .data
            .unwrap_or_default();
        let levels = self
            .inner
            .get_levels(Request::new(GetLevelsRequest {}))
            .await
            .map(|r| r.into_inner().levels)
            .unwrap_or_default();
        let mut avg_entry = String::new();
        let positions = data
            .positions
            .iter()
            .map(|p| {
                if p.symbol.contains("BTC") {
                    avg_entry = p.avg_price.clone();
                }
                (
                    p.symbol.clone(),
                    Decimal::from_str_exact(&p.quantity).unwrap_or(Decimal::ZERO),
                    p.avg_price.clone(),
                )
            })
            .collect();
        Ok(RunnerSnapshot {
            venue: data.venue,
            environment: data.environment,
            execution_mode: data.execution_mode,
            trading_enabled: status.enabled,
            entries_paused: status.entries_paused || data.entries_paused,
            flatten_latched: status.flatten_latched || data.flatten_latched,
            positions,
            orders: data
                .orders
                .into_iter()
                .map(|o| (o.order_id, o.status, o.level_id))
                .collect(),
            levels: levels
                .into_iter()
                .map(|l| (l.id, l.enabled, l.status))
                .collect(),
            exposure_halt_reason: if status.exposure_halt_reason.is_empty() {
                data.exposure_halt_reason
            } else {
                status.exposure_halt_reason
            },
            last_block_reason_code: if status.last_block_reason_code.is_empty() {
                data.last_block_reason_code
            } else {
                status.last_block_reason_code
            },
            avg_entry,
        })
    }
}

pub fn parse_dec(v: &serde_json::Value) -> Decimal {
    match v {
        serde_json::Value::Number(n) => Decimal::from_str_exact(&n.to_string()).unwrap_or_default(),
        serde_json::Value::String(s) => Decimal::from_str_exact(s).unwrap_or_default(),
        _ => Decimal::ZERO,
    }
}

pub fn parse_dec_str(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap_or(Decimal::ZERO)
}
