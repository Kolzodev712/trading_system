//! Public Bybit klines for warmup / replay. Not an order path.

use trading_core::types::common::Pair;
use trading_engine::cex::strategies::replay::Candle;

#[derive(serde::Deserialize)]
struct BybitKlineResponse {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: BybitKlineResult,
}

#[derive(serde::Deserialize)]
struct BybitKlineResult {
    list: Vec<Vec<String>>,
}

pub async fn fetch_bybit_klines(
    pair: &Pair,
    interval: &str,
    limit: u32,
) -> Result<Vec<Candle>, String> {
    let symbol = format!("{}{}", pair.base, pair.quote);
    let url = format!(
        "https://api.bybit.com/v5/market/kline?category=linear&symbol={symbol}&interval={interval}&limit={limit}"
    );
    let resp: BybitKlineResponse = reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ret_code != 0 {
        return Err(format!(
            "bybit kline error: {} {}",
            resp.ret_code, resp.ret_msg
        ));
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
    candles.reverse();
    Ok(candles)
}
