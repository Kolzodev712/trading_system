//! One-shot gRPC control for demo/test runs (not the TUI).
use std::time::Duration;
use tonic::transport::Channel;

#[allow(clippy::all)]
mod trading_cli {
    include!(concat!(env!("OUT_DIR"), "/trading_cli.rs"));
}

use trading_cli::trading_cli_service_client::TradingCliServiceClient;
use trading_cli::{
    GetMonitoringDataRequest, GetStrategyConfigRequest, GetTradingStatusRequest,
    RequestRestartRequest, SetTradingEnabledRequest, ToggleStrategyRequest,
    UpdateStrategyCapRequest,
};

fn usage() -> ! {
    eprintln!(
        "usage: grpc_ctl <enable|disable|status|cap|config|restart> [args]
  enable [StrategyName]
  disable [StrategyName]
  status
  cap <StrategyName> <BASE/QUOTE> <qty>
  config [StrategyName] [BASE/QUOTE]
  restart [cancel_working]

env: TRADING_GRPC (default http://127.0.0.1:50051)
     TRADING_STRATEGY (default PropHold)
     TRADING_SYMBOL (default BTC/USDT)"
    );
    std::process::exit(2);
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
    let mut client = TradingCliServiceClient::new(channel);

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
            let resp = client
                .toggle_strategy(ToggleStrategyRequest {
                    strategy_name,
                    symbol,
                    enabled: true,
                })
                .await?
                .into_inner();
            println!("strategy enabled={} msg={}", resp.success, resp.message);
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
            let enabled = client
                .get_trading_status(GetTradingStatusRequest {})
                .await?
                .into_inner()
                .enabled;
            let data = client
                .get_monitoring_data(GetMonitoringDataRequest {})
                .await?
                .into_inner()
                .data
                .unwrap_or_default();
            println!("global_trading={enabled}");
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
            }
            for p in data.positions {
                println!(
                    "position {} qty={} side={} entry={} mark={} upnl={}",
                    p.symbol, p.quantity, p.side, p.avg_price, p.mark_price, p.unrealized_pnl
                );
            }
            for o in data.orders {
                println!(
                    "order {} {} {} px={} qty={} status={}",
                    o.order_id, o.symbol, o.side, o.price, o.quantity, o.status
                );
            }
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
        _other => usage(),
    }
    Ok(())
}
