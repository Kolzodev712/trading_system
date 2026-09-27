//! Public Bybit klines for warmup / replay. Not an order path.
//!
//! The latest-N helper always uses the public mainnet host. Ranged downloads
//! for DonchianDailyV1 choose the host explicitly. Both parse the response
//! with [`trading_engine::cex::bybit_public_klines`].

use trading_core::types::common::Pair;
use trading_engine::cex::bybit_public_klines::parse_bybit_public_kline_page;
use trading_engine::cex::strategies::replay::Candle;

pub async fn fetch_bybit_klines(
    pair: &Pair,
    interval: &str,
    limit: u32,
) -> Result<Vec<Candle>, String> {
    let symbol = format!("{}{}", pair.base, pair.quote);
    let url = format!(
        "https://api.bybit.com/v5/market/kline?category=linear&symbol={symbol}&interval={interval}&limit={limit}"
    );
    let body = reqwest::get(&url)
        .await
        .map_err(|err| err.to_string())?
        .text()
        .await
        .map_err(|err| err.to_string())?;
    candles_from_bybit_body(&body)
}

pub fn candles_from_bybit_body(body: &str) -> Result<Vec<Candle>, String> {
    let page = parse_bybit_public_kline_page(body).map_err(|err| err.to_string())?;
    let mut candles: Vec<Candle> = page
        .rows
        .into_iter()
        .map(|row| Candle {
            timestamp_ms: row.start_time_ms,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
        })
        .collect();
    // Bybit returns the newest candle first.
    candles.reverse();
    Ok(candles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_is_returned_oldest_first() {
        let body = r#"{"retCode":0,"retMsg":"OK","result":{"list":[["2000","2","4","1","3","1"],["1000","2","4","1","3","1"]]}}"#;
        let candles = candles_from_bybit_body(body).unwrap();
        assert_eq!(candles[0].timestamp_ms, 1000);
        assert_eq!(candles[1].timestamp_ms, 2000);
    }
}
