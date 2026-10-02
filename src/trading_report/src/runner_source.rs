use chrono::{TimeZone, Utc};

use crate::config::RunnerConfig;
use crate::model::{
    Availability, BalanceView, CompletedCandle, LevelView, OrderView, PositionView, RiskView,
    SourceTime, StrategyView,
};

pub mod proto {
    #![allow(clippy::all)]
    tonic::include_proto!("trading_cli");
}

use proto::trading_cli_service_client::TradingCliServiceClient;
use proto::{
    GetLevelsRequest, GetMonitoringDataRequest, GetRiskSnapshotRequest, GetTradingStatusRequest,
};

#[derive(Debug, Clone)]
pub struct RunnerSnapshot {
    pub observed_at_utc: String,
    pub status: Result<proto::GetTradingStatusResponse, String>,
    pub monitoring: Result<proto::MonitoringData, String>,
    pub levels: Result<Option<Vec<LevelView>>, String>,
    pub risk: Result<RiskView, String>,
    pub sources: Vec<SourceTime>,
}

pub async fn snapshot_runner(cfg: &RunnerConfig) -> RunnerSnapshot {
    let observed_at_utc = Utc::now().to_rfc3339();
    let mut sources = Vec::new();
    let mut client = match TradingCliServiceClient::connect(cfg.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(err) => {
            let msg = format!("grpc connect: {err}");
            sources.push(SourceTime {
                source: "grpc".into(),
                observed_at_utc: Some(observed_at_utc.clone()),
                age_ms: Some(0),
                ok: false,
                error: Some(msg.clone()),
            });
            return RunnerSnapshot {
                observed_at_utc,
                status: Err(msg.clone()),
                monitoring: Err(msg.clone()),
                levels: Err(msg.clone()),
                risk: Err(msg),
                sources,
            };
        }
    };

    let status = match client.get_trading_status(GetTradingStatusRequest {}).await {
        Ok(resp) => {
            sources.push(ok_source("GetTradingStatus", &observed_at_utc));
            Ok(resp.into_inner())
        }
        Err(err) => {
            let msg = err.to_string();
            sources.push(err_source("GetTradingStatus", &observed_at_utc, &msg));
            Err(msg)
        }
    };

    let monitoring = match client
        .get_monitoring_data(GetMonitoringDataRequest {})
        .await
    {
        Ok(resp) => match resp.into_inner().data {
            Some(data) => {
                let observed = epoch_ms_to_utc(data.snapshot_unix_ms)
                    .unwrap_or_else(|| observed_at_utc.clone());
                sources.push(ok_source("GetMonitoringData", &observed));
                Ok(data)
            }
            None => {
                let msg = "GetMonitoringData returned no payload".to_string();
                sources.push(err_source("GetMonitoringData", &observed_at_utc, &msg));
                Err(msg)
            }
        },
        Err(err) => {
            let msg = err.to_string();
            sources.push(err_source("GetMonitoringData", &observed_at_utc, &msg));
            Err(msg)
        }
    };

    let levels = if cfg.mode.eq_ignore_ascii_case("levels") {
        match client.get_levels(GetLevelsRequest {}).await {
            Ok(resp) => {
                sources.push(ok_source("GetLevels", &observed_at_utc));
                Ok(Some(
                    resp.into_inner()
                        .levels
                        .into_iter()
                        .map(|level| {
                            let config = level.config;
                            LevelView {
                                id: level.id,
                                enabled: level.enabled,
                                price: config.as_ref().map(|c| c.price.clone()).unwrap_or_default(),
                                quantity: config
                                    .as_ref()
                                    .map(|c| c.quantity.clone())
                                    .unwrap_or_default(),
                                owned_orders: level.active_orders,
                                fill_count: level.filled_count,
                            }
                        })
                        .collect(),
                ))
            }
            Err(err) => {
                let msg = format!("GetLevels failed: {err}");
                sources.push(err_source("GetLevels", &observed_at_utc, &msg));
                Err(msg)
            }
        }
    } else {
        Ok(None)
    };

    let risk = match client
        .get_risk_snapshot(GetRiskSnapshotRequest {
            instrument: cfg.instrument.clone(),
        })
        .await
    {
        Ok(resp) => match resp.into_inner().snapshot {
            Some(snap) if snap.available => {
                let observed = if snap.observed_at_unix_ms > 0 {
                    epoch_ms_to_utc(snap.observed_at_unix_ms).unwrap_or_else(|| observed_at_utc.clone())
                } else {
                    observed_at_utc.clone()
                };
                sources.push(ok_source("GetRiskSnapshot", &observed));
                Ok(RiskView {
                    total_max_position: empty_to_none(snap.total_max_position),
                    max_total_orders: empty_to_none(snap.max_total_orders),
                    max_notional: empty_to_none(snap.max_notional),
                    max_order_qty: empty_to_none(snap.max_order_qty),
                    daily_loss_limit_all_pairs: empty_to_none(snap.daily_loss_limit_all_pairs),
                    position_qty: snap.position_qty,
                    working_qty: snap.working_qty,
                    reserved_qty: snap.reserved_qty,
                    daily_pnl: if snap.daily_pnl_source == "ledger" {
                        Availability::ok(snap.daily_pnl)
                    } else {
                        Availability::missing(snap.daily_pnl_source)
                    },
                    headroom: if snap.headroom.is_empty() {
                        "unknown".into()
                    } else {
                        snap.headroom
                    },
                    instrument: snap.instrument,
                })
            }
            Some(snap) => {
                let msg = if snap.unavailable_reason.is_empty() {
                    "GetRiskSnapshot unavailable".into()
                } else {
                    snap.unavailable_reason
                };
                sources.push(err_source("GetRiskSnapshot", &observed_at_utc, &msg));
                Err(msg)
            }
            None => {
                let msg = "GetRiskSnapshot returned no payload".to_string();
                sources.push(err_source("GetRiskSnapshot", &observed_at_utc, &msg));
                Err(msg)
            }
        },
        Err(err) => {
            let msg = err.to_string();
            sources.push(err_source("GetRiskSnapshot", &observed_at_utc, &msg));
            Err(msg)
        }
    };

    RunnerSnapshot {
        observed_at_utc,
        status,
        monitoring,
        levels,
        risk,
        sources,
    }
}

fn empty_to_none(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn ok_source(name: &str, at: &str) -> SourceTime {
    SourceTime {
        source: name.into(),
        observed_at_utc: Some(at.to_string()),
        age_ms: Some(0),
        ok: true,
        error: None,
    }
}

fn err_source(name: &str, at: &str, err: &str) -> SourceTime {
    SourceTime {
        source: name.into(),
        observed_at_utc: Some(at.to_string()),
        age_ms: None,
        ok: false,
        error: Some(err.to_string()),
    }
}

pub fn proto_orders(data: &proto::MonitoringData) -> Vec<OrderView> {
    data.orders
        .iter()
        .map(|o| OrderView {
            order_id: o.order_id.clone(),
            client_order_id: None,
            symbol: o.symbol.clone(),
            side: o.side.clone(),
            price: o.price.clone(),
            quantity: o.quantity.clone(),
            status: o.status.clone(),
            created_at: Some(o.timestamp.to_string()),
        })
        .collect()
}

pub fn proto_positions(data: &proto::MonitoringData) -> Vec<PositionView> {
    data.positions
        .iter()
        .map(|p| PositionView {
            symbol: p.symbol.clone(),
            quantity: p.quantity.clone(),
        })
        .collect()
}

pub fn proto_balances(data: &proto::MonitoringData) -> Vec<BalanceView> {
    data.balances
        .iter()
        .map(|b| BalanceView {
            asset: b.asset.clone(),
            total: b.total.clone(),
            free: Some(b.free.clone()),
        })
        .collect()
}

pub fn proto_strategies(data: &proto::MonitoringData) -> Vec<StrategyView> {
    data.strategies
        .iter()
        .map(|s| StrategyView {
            strategy_id: s.strategy_id.clone(),
            enabled: s.enabled,
            last_signal: s.last_signal.clone(),
            desired_target: s.desired_target.clone(),
            model_reconstructed: s.model_reconstructed,
            entry_freshness: s.entry_freshness.clone(),
            research_disposition: s.research_disposition.clone(),
            strategy_version: s.strategy_version.clone(),
            config_hash: s.config_hash.clone(),
        })
        .collect()
}

pub fn completed_candle(data: &proto::MonitoringData) -> Availability<CompletedCandle> {
    let Some(s) = data.strategies.first() else {
        return Availability::missing("no strategy status with candle fields");
    };
    if s.last_completed_candle_end_ms <= 0 {
        return Availability::missing("completed candle not exposed");
    }
    Availability::ok(CompletedCandle {
        start_ms: s.last_completed_candle_start_ms,
        end_ms: s.last_completed_candle_end_ms,
        close: s.last_close.clone(),
    })
}

pub fn epoch_ms_to_utc(ms: i64) -> Option<String> {
    let ms = normalize_epoch_ms(ms)?;
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.to_rfc3339())
}

pub fn normalize_epoch_ms(ts: i64) -> Option<i64> {
    if ts <= 0 {
        return None;
    }
    if ts < 1_000_000_000_000 {
        Some(ts.saturating_mul(1000))
    } else {
        Some(ts)
    }
}

pub fn age_ms(now_ms: i64, ts: i64) -> Option<i64> {
    let ts = normalize_epoch_ms(ts)?;
    Some(now_ms.saturating_sub(ts).max(0))
}
