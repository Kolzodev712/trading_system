//! One-shot gRPC control for demo/test runs (not the TUI).
use std::time::Duration;
use tonic::metadata::{Ascii, MetadataValue};
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::{Request, Status};

use trading_proto::trading_cli;

use trading_cli::trading_cli_service_client::TradingCliServiceClient;
use trading_cli::{
    DeleteLevelRequest, GetLevelsRequest, GetMonitoringDataRequest, GetStrategyConfigRequest,
    GetTradingStatusRequest, OperatorCommandRequest, RequestRestartRequest,
    SetTradingEnabledRequest, ToggleLevelRequest, ToggleStrategyRequest, UpdateLevelRequest,
    UpdateStrategyCapRequest,
};

fn usage() -> ! {
    eprintln!(
        "usage: grpc_ctl <enable|disable|status|cap|config|restart|pause|cancel|flatten|resume|levels|toggle-level|delete-level|update-level> [args]
  enable [StrategyName]
  disable [StrategyName]
  status
  cap <StrategyName> <BASE/QUOTE> <qty>
  config [StrategyName] [BASE/QUOTE]
  restart [cancel_working|flatten|keep_working]
  pause [request_id] [symbol]
  cancel [request_id] [symbol]
  flatten [request_id] [symbol]
  resume [request_id] [symbol]
  levels
  toggle-level <level_id> <true|false>
  delete-level <level_id>
  update-level <level_id> price=Fixed(N)

env: TRADING_GRPC (default http://127.0.0.1:50051)
     TRADING_STRATEGY (default PropHold)
     TRADING_SYMBOL (default BTC/USDT)
     TRADING_OPERATOR_TOKEN (sent as x-operator-token; required off Paper)"
    );
    std::process::exit(2);
}

#[derive(Clone)]
struct OperatorAuth {
    token: Option<MetadataValue<Ascii>>,
}

impl Interceptor for OperatorAuth {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(ref token) = self.token {
            request
                .metadata_mut()
                .insert("x-operator-token", token.clone());
        }
        Ok(request)
    }
}

fn operator_auth() -> OperatorAuth {
    let token = std::env::var("TRADING_OPERATOR_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .and_then(|s| MetadataValue::try_from(s).ok());
    OperatorAuth { token }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "status".into());
    let endpoint =
        std::env::var("TRADING_GRPC").unwrap_or_else(|_| "http://127.0.0.1:50051".into());

    let channel = Channel::from_shared(endpoint.clone())?
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(5))
        .connect()
        .await?;
    let mut client = TradingCliServiceClient::with_interceptor(channel, operator_auth());

    let default_name = std::env::var("TRADING_STRATEGY").unwrap_or_else(|_| "PropHold".into());
    let default_symbol = std::env::var("TRADING_SYMBOL").unwrap_or_else(|_| "BTC/USDT".into());

    match cmd.as_str() {
        "enable" => {
            let strategy_name = args.next().unwrap_or(default_name);
            let symbol = args.next().unwrap_or(default_symbol);
            let resp = client
                .set_trading_enabled(SetTradingEnabledRequest { enabled: true })
                .await?
                .into_inner();
            println!("trading enabled={} msg={}", resp.success, resp.message);
            match client
                .toggle_strategy(ToggleStrategyRequest {
                    strategy_name,
                    symbol,
                    enabled: true,
                })
                .await
            {
                Ok(resp) => {
                    let resp = resp.into_inner();
                    println!("strategy enabled={} msg={}", resp.success, resp.message);
                }
                Err(status) => {
                    println!(
                        "strategy toggle skipped (levels mode or unavailable): {}",
                        status.message()
                    );
                }
            }
        }
        "disable" => {
            let strategy_name = args.next().unwrap_or(default_name);
            let symbol = args.next().unwrap_or(default_symbol);
            let resp = client
                .toggle_strategy(ToggleStrategyRequest {
                    strategy_name,
                    symbol,
                    enabled: false,
                })
                .await?
                .into_inner();
            println!("strategy disabled={} msg={}", resp.success, resp.message);
            let resp = client
                .set_trading_enabled(SetTradingEnabledRequest { enabled: false })
                .await?
                .into_inner();
            println!("trading disabled={} msg={}", resp.success, resp.message);
        }
        "status" => {
            let status = client
                .get_trading_status(GetTradingStatusRequest {})
                .await?
                .into_inner();
            let data = client
                .get_monitoring_data(GetMonitoringDataRequest {})
                .await?
                .into_inner()
                .data
                .unwrap_or_default();
            println!("global_trading={}", status.enabled);
            println!(
                "entries_paused={} flatten_latched={}",
                status.entries_paused, status.flatten_latched
            );
            println!("operator_id={}", status.operator_id);
            println!(
                "exposure_halt_reason={} last_block_reason_code={}",
                status.exposure_halt_reason, status.last_block_reason_code
            );
            println!("global_toggle_semantics={}", status.global_toggle_semantics);
            println!(
                "runner_mode={} venue={} environment={} execution_mode={} snapshot_unix_ms={}",
                data.runner_mode,
                data.venue,
                data.environment,
                data.execution_mode,
                data.snapshot_unix_ms
            );
            println!(
                "instrument={} exchange={} orders={} positions={} orderbooks={} indicators={}",
                data.instrument_type,
                data.exchange,
                data.orders.len(),
                data.positions.len(),
                data.orderbooks.len(),
                data.indicators.len()
            );
            for s in data.strategies {
                println!(
                    "strategy {} enabled={} pos={} cap={} orders={}/{} signal={}",
                    s.strategy_id,
                    s.enabled,
                    s.position_quantity,
                    s.max_position,
                    s.active_orders,
                    s.max_active_orders,
                    s.last_signal
                );
                if !s.readiness.is_empty() {
                    println!(
                        "strategy {} readiness={} reason={}",
                        s.strategy_id, s.readiness, s.readiness_reason
                    );
                }
                println!(
                    "strategy {} model_reconstructed={} fresh_entry_eligible={} entry_freshness={} last_flat_to_long_age_secs={} previous_direction={} desired_target={} filled_position={}",
                    s.strategy_id,
                    s.model_reconstructed,
                    s.fresh_entry_eligible,
                    s.entry_freshness,
                    s.last_flat_to_long_age_secs,
                    s.previous_direction,
                    s.desired_target,
                    s.filled_position
                );
            }
            for p in data.positions {
                println!(
                    "position {} qty={} side={} entry={} mark={} upnl={}",
                    p.symbol, p.quantity, p.side, p.avg_price, p.mark_price, p.unrealized_pnl
                );
            }
            for o in data.orders {
                println!(
                    "order {} {} {} px={} qty={} status={} level={}",
                    o.order_id, o.symbol, o.side, o.price, o.quantity, o.status, o.level_id
                );
            }
            if let Ok(levels) = client.get_levels(GetLevelsRequest {}).await {
                for level in levels.into_inner().levels {
                    println!(
                        "level {} enabled={} status={} filled={} orders={}",
                        level.id,
                        level.enabled,
                        level.status,
                        level.filled_count,
                        level.active_orders.join(",")
                    );
                }
            }
        }
        "levels" => {
            let resp = client.get_levels(GetLevelsRequest {}).await?.into_inner();
            if resp.levels.is_empty() {
                println!("levels=0");
            }
            for level in resp.levels {
                println!(
                    "level {} enabled={} status={} filled={} orders={}",
                    level.id,
                    level.enabled,
                    level.status,
                    level.filled_count,
                    level.active_orders.join(",")
                );
            }
        }
        "toggle-level" => {
            let level_id = args.next().unwrap_or_else(|| usage());
            let enabled = match args.next().unwrap_or_else(|| usage()).as_str() {
                "true" | "on" | "1" | "enable" => true,
                "false" | "off" | "0" | "disable" => false,
                _ => usage(),
            };
            let resp = client
                .toggle_level(ToggleLevelRequest { level_id, enabled })
                .await?
                .into_inner();
            println!("toggle-level success={}", resp.success);
        }
        "delete-level" => {
            let level_id = args.next().unwrap_or_else(|| usage());
            let resp = client
                .delete_level(DeleteLevelRequest { level_id })
                .await?
                .into_inner();
            println!("delete-level success={}", resp.success);
        }
        "update-level" => {
            let level_id = args.next().unwrap_or_else(|| usage());
            let mut price_override = None;
            let mut qty_override = None;
            for raw in args {
                if let Some(v) = raw.strip_prefix("price=") {
                    price_override = Some(v.to_string());
                } else if let Some(v) = raw.strip_prefix("qty=") {
                    qty_override = Some(v.to_string());
                } else {
                    usage();
                }
            }
            if price_override.is_none() && qty_override.is_none() {
                usage();
            }
            let current = client
                .get_level(trading_cli::GetLevelRequest {
                    level_id: level_id.clone(),
                })
                .await?
                .into_inner()
                .level
                .and_then(|l| l.config)
                .ok_or("level config missing")?;
            let mut config = current;
            if let Some(price) = price_override {
                config.price = price;
            }
            if let Some(quantity) = qty_override {
                config.quantity = quantity;
            }
            let resp = client
                .update_level(UpdateLevelRequest {
                    level_id,
                    config: Some(config),
                })
                .await?
                .into_inner();
            println!("update-level success={}", resp.success);
        }
        "cap" => {
            let strategy_name = args.next().unwrap_or_else(|| usage());
            let symbol = args.next().unwrap_or_else(|| usage());
            let max_position = args.next().unwrap_or_else(|| usage());
            let resp = client
                .update_strategy_cap(UpdateStrategyCapRequest {
                    strategy_name,
                    symbol,
                    max_position,
                })
                .await?
                .into_inner();
            println!("cap success={} msg={}", resp.success, resp.message);
        }
        "config" => {
            let strategy_name = args.next().unwrap_or(default_name);
            let symbol = args.next().unwrap_or(default_symbol);
            let resp = client
                .get_strategy_config(GetStrategyConfigRequest {
                    strategy_name,
                    symbol,
                })
                .await?
                .into_inner();
            match resp.config {
                Some(c) => {
                    println!(
                        "type={} symbol={} size={} cap={}",
                        c.strategy_type, c.symbol, c.position_size, c.max_position
                    );
                    println!(
                        "buy_thr={} sell_thr={} sl={} tp={}",
                        c.buy_threshold_percent,
                        c.sell_threshold_percent,
                        c.stop_loss_percent,
                        c.take_profit_percent
                    );
                    println!(
                        "max_orders={} hist={} min_hist={}",
                        c.max_active_orders, c.price_history_size, c.min_price_history
                    );
                }
                None => println!("no config"),
            }
        }
        "restart" => {
            let on_stop = args.next().unwrap_or_else(|| "cancel_working".into());
            let resp = client
                .request_restart(RequestRestartRequest { on_stop })
                .await?
                .into_inner();
            println!("restart success={} msg={}", resp.success, resp.message);
        }
        "pause" | "cancel" | "flatten" | "resume" => {
            let request_id = args.next().unwrap_or_default();
            let symbol = args.next().unwrap_or_default();
            let req = OperatorCommandRequest { request_id, symbol };
            let resp = match cmd.as_str() {
                "pause" => client.pause_entries(req).await?.into_inner(),
                "cancel" => client.cancel_working(req).await?.into_inner(),
                "flatten" => client.flatten_and_pause(req).await?.into_inner(),
                _ => client.resume(req).await?.into_inner(),
            };
            println!(
                "accepted={} state={} venue={} environment={} scope={} msg={}",
                resp.accepted, resp.state, resp.venue, resp.environment, resp.scope, resp.message
            );
            if !resp.order_ids.is_empty() {
                println!("order_ids={}", resp.order_ids.join(","));
            }
        }
        _other => usage(),
    }
    Ok(())
}
