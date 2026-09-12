//! Offline candle replay. Public Bybit klines or a JSON file. Does not place orders.
//!
//!   cargo run -p runner --bin prop_replay -- --pair BTC/USDT --interval 60 --limit 200
//!   cargo run -p runner --bin prop_replay -- --file candles.json

use clap::Parser;
use rust_decimal::Decimal;
use serde::Deserialize;
use trading_core::types::common::{Exchange, InstrumentType, Pair};
use trading_engine::cex::state::TradingState;
use trading_engine::cex::strategies::replay::{replay_candles, Candle};
use trading_engine::cex::strategies::{build_strategy, StrategyFactoryArgs};

#[derive(Parser, Debug)]
#[command(about = "Replay a CEX strategy over candles (no live orders)")]
struct Args {
    /// Strategy type (PropHold or BuyLowSellHigh)
    #[arg(long, default_value = "PropHold")]
    strategy: String,

    /// Pair, e.g. BTC/USDT
    #[arg(long, default_value = "BTC/USDT")]
    pair: String,

    /// Bybit kline interval: 1, 5, 15, 60, 240, D
    #[arg(long, default_value = "60")]
    interval: String,

    /// Number of public klines to fetch
    #[arg(long, default_value_t = 200)]
    limit: u32,

    /// Optional JSON array of {timestamp_ms, open, high, low, close, volume}
    #[arg(long)]
    file: Option<String>,

    /// Starting signed qty (use to test flatten)
    #[arg(long, default_value = "0")]
    start_qty: String,
}

#[derive(Deserialize)]
struct BybitKlineResponse {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: BybitKlineResult,
}

#[derive(Deserialize)]
struct BybitKlineResult {
    list: Vec<Vec<String>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let pair = parse_pair(&args.pair)?;
    let candles = if let Some(path) = &args.file {
        load_candles_file(path)?
    } else {
        fetch_bybit_klines(&pair, &args.interval, args.limit).await?
    };
    if candles.is_empty() {
        return Err("no candles".into());
    }

    let strategy = build_strategy(factory_args(&args.strategy, pair.clone()))?;
    let mut account = TradingState::new();
    let start_qty: Decimal = args.start_qty.parse()?;
    if start_qty.abs() > Decimal::ZERO {
        account.set_position(pair.clone(), start_qty, candles[0].close);
    }

    let result = {
        let mut guard = strategy.write().await;
        replay_candles(
            &mut *guard,
            &candles,
            &pair,
            InstrumentType::Futures,
            &mut account,
        )
        .await?
    };

    println!(
        "strategy={} pair={} candles={} holds={} targets={} trades={} final_qty={}",
        args.strategy,
        pair,
        candles.len(),
        result.holds,
        result.targets,
        result.trades.len(),
        result.final_qty
    );
    for trade in result.trades.iter().take(20) {
        println!(
            "  {:?} qty={} @ {} ts={}",
            trade.side, trade.quantity, trade.price, trade.timestamp_ms
        );
    }
    if result.trades.len() > 20 {
        println!("  ... {} more trades", result.trades.len() - 20);
    }
    Ok(())
}

fn parse_pair(s: &str) -> Result<Pair, Box<dyn std::error::Error>> {
    let (base, quote) = s
        .split_once('/')
        .ok_or_else(|| format!("pair must be BASE/QUOTE, got {s}"))?;
    Ok(Pair::new(base, quote))
}

fn factory_args(strategy_type: &str, symbol: Pair) -> StrategyFactoryArgs {
    StrategyFactoryArgs {
        strategy_type: strategy_type.to_string(),
        symbol,
        base_position_size: Decimal::new(1, 3),
        max_position: Decimal::new(1, 3),
        base_buy_threshold: Decimal::new(5, 1),
        base_sell_threshold: Decimal::new(5, 1),
        stop_loss_percent: None,
        take_profit_percent: None,
        max_active_orders: 2,
        price_history_size: 20,
        min_price_history: 5,
        asset_service: None,
        exchange: Exchange::Bybit,
        config_min_order_size: Decimal::new(1, 5),
        config_step_size: Decimal::new(1, 5),
        config_tick_size: Decimal::new(1, 2),
        config_min_notional: Decimal::new(5, 0),
    }
}

fn load_candles_file(path: &str) -> Result<Vec<Candle>, Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

async fn fetch_bybit_klines(
    pair: &Pair,
    interval: &str,
    limit: u32,
) -> Result<Vec<Candle>, Box<dyn std::error::Error>> {
    let symbol = format!("{}{}", pair.base, pair.quote);
    let url = format!(
        "https://api.bybit.com/v5/market/kline?category=linear&symbol={symbol}&interval={interval}&limit={limit}"
    );
    let resp: BybitKlineResponse = reqwest::get(url).await?.json().await?;
    if resp.ret_code != 0 {
        return Err(format!("bybit kline error: {} {}", resp.ret_code, resp.ret_msg).into());
    }
    let mut candles: Vec<Candle> = resp
        .result
        .list
        .into_iter()
        .filter_map(|row| {
            let timestamp_ms = row.first()?.parse().ok()?;
            Some(Candle {
                timestamp_ms,
                open: row.get(1)?.parse().ok()?,
                high: row.get(2)?.parse().ok()?,
                low: row.get(3)?.parse().ok()?,
                close: row.get(4)?.parse().ok()?,
                volume: row.get(5)?.parse().ok()?,
            })
        })
        .collect();
    // Bybit returns newest first.
    candles.reverse();
    Ok(candles)
}
