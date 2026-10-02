//! Hourly Bybit BTCUSDT trade candles for BTCLiquidationShockV1.
//!
//! A gap, a duplicate conflict, or a non-positive close rejects the dataset.
//! This module does not interpolate.

use crate::bybit_public_klines::parse_bybit_public_kline_page;
use crate::donchian_daily_v1::offline::MarketDataEnvironment;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use strategy_core::btc_liquidation_shock_v1::CLOSES_REQUIRED;

use super::ShockError;

pub const HOUR_MS: u64 = 3_600_000;
pub const PERFORMANCE_START_MS: u64 = 1_674_259_200_000;
pub const PERFORMANCE_END_EXCLUSIVE_MS: u64 = 1_767_225_600_000;
const FORMAT_NAME: &str = "btc_liquidation_shock_v1_trade_60";
const FORMAT_VERSION: u32 = 1;
const VENUE: &str = "bybit";
const PRODUCT: &str = "linear";
const SYMBOL: &str = "BTCUSDT";
const INTERVAL: &str = "60";
const PAGE_LIMIT: u32 = 1000;
const MAX_PAGES: usize = 40;

pub fn first_eligible_open_ms() -> u64 {
    PERFORMANCE_START_MS - HOUR_MS
}

pub fn dataset_start_ms() -> u64 {
    first_eligible_open_ms() - (CLOSES_REQUIRED as u64 - 1) * HOUR_MS
}

pub fn last_eligible_open_ms() -> u64 {
    PERFORMANCE_END_EXCLUSIVE_MS - 2 * HOUR_MS
}

pub fn dataset_end_exclusive_ms() -> u64 {
    last_eligible_open_ms() + HOUR_MS
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub environment: MarketDataEnvironment,
    pub start_ms: u64,
    pub end_exclusive_ms: u64,
    pub retrieved_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HourlyCandle {
    pub start_time_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetMetadata {
    pub format: String,
    pub format_version: u32,
    pub venue: String,
    pub environment: String,
    pub product: String,
    pub symbol: String,
    pub interval: String,
    pub requested_start_ms: u64,
    pub requested_end_exclusive_ms: u64,
    pub actual_start_ms: Option<u64>,
    pub actual_end_ms: Option<u64>,
    pub retrieved_at_ms: u64,
    pub candle_count: usize,
    pub pages_fetched: u32,
    pub dataset_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HourlyDataset {
    pub metadata: DatasetMetadata,
    pub candles: Vec<HourlyCandle>,
}

#[derive(Clone)]
struct RawCandle {
    start_time_ms: u64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

pub fn frozen_download_request(retrieved_at_ms: u64) -> DownloadRequest {
    DownloadRequest {
        environment: MarketDataEnvironment::Mainnet,
        start_ms: dataset_start_ms(),
        end_exclusive_ms: dataset_end_exclusive_ms(),
        retrieved_at_ms,
    }
}

pub async fn download_hourly_dataset(
    request: &DownloadRequest,
) -> Result<HourlyDataset, ShockError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| ShockError::new(format!("http client: {err}")))?;
    download_hourly_with(request, PAGE_LIMIT, |url| {
        let client = client.clone();
        async move { fetch_public_body(&client, &url).await }
    })
    .await
}

pub async fn download_hourly_with<F, Fut>(
    request: &DownloadRequest,
    page_limit: u32,
    mut fetch: F,
) -> Result<HourlyDataset, ShockError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, ShockError>>,
{
    if page_limit == 0 {
        return Err(ShockError::new("page limit must be positive"));
    }
    if request.start_ms >= request.end_exclusive_ms {
        return Err(ShockError::new("hourly request window is empty"));
    }
    let mut end_inclusive = request.end_exclusive_ms - 1;
    let mut rows = Vec::new();
    let mut pages_fetched = 0u32;
    for _ in 0..MAX_PAGES {
        pages_fetched += 1;
        let url = kline_url(request, end_inclusive, page_limit);
        let body = fetch(url).await?;
        let mut page = parse_page(&body)?;
        page.retain(|row| {
            row.start_time_ms >= request.start_ms && row.start_time_ms < request.end_exclusive_ms
        });
        if page.is_empty() {
            return assemble(request, rows, pages_fetched);
        }
        let oldest = page
            .iter()
            .map(|row| row.start_time_ms)
            .min()
            .ok_or_else(|| ShockError::new("kline page was empty after filtering"))?;
        let page_len = page.len();
        rows.extend(page);
        if oldest <= request.start_ms || page_len < page_limit as usize {
            return assemble(request, rows, pages_fetched);
        }
        let next_end = oldest.checked_sub(1).ok_or_else(|| {
            ShockError::new("pagination reached the start of the timestamp range")
        })?;
        if next_end >= end_inclusive {
            return Err(ShockError::new(
                "pagination did not move backward; refusing to truncate the requested range",
            ));
        }
        end_inclusive = next_end;
    }
    Err(ShockError::new(
        "pagination limit reached before the requested start; refusing to truncate coverage",
    ))
}

pub fn save_hourly_dataset(path: &Path, dataset: &HourlyDataset) -> Result<(), ShockError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| ShockError::new(format!("create {}: {err}", parent.display())))?;
        }
    }
    let body = serde_json::to_vec_pretty(dataset)
        .map_err(|err| ShockError::new(format!("encode hourly dataset: {err}")))?;
    std::fs::write(path, &body)
        .map_err(|err| ShockError::new(format!("write {}: {err}", path.display())))
}

pub fn load_hourly_dataset(path: &Path) -> Result<HourlyDataset, ShockError> {
    let body = std::fs::read(path)
        .map_err(|err| ShockError::new(format!("read {}: {err}", path.display())))?;
    let dataset: HourlyDataset = serde_json::from_slice(&body)
        .map_err(|err| ShockError::new(format!("decode {}: {err}", path.display())))?;
    verify_hourly_dataset(&dataset)?;
    Ok(dataset)
}

pub fn verify_hourly_dataset(dataset: &HourlyDataset) -> Result<(), ShockError> {
    if dataset.metadata.format != FORMAT_NAME || dataset.metadata.format_version != FORMAT_VERSION {
        return Err(ShockError::new("hourly dataset format is not v1 trade-60"));
    }
    if dataset.metadata.venue != VENUE
        || dataset.metadata.environment != "mainnet"
        || dataset.metadata.product != PRODUCT
        || dataset.metadata.symbol != SYMBOL
        || dataset.metadata.interval != INTERVAL
    {
        return Err(ShockError::new(
            "hourly dataset is not mainnet Bybit linear BTCUSDT interval 60",
        ));
    }
    let actual = dataset_hash(&dataset.candles)?;
    if actual != dataset.metadata.dataset_hash {
        return Err(ShockError::new(
            "hourly dataset hash does not match the stored candles",
        ));
    }
    if dataset.metadata.requested_start_ms != dataset_start_ms()
        || dataset.metadata.requested_end_exclusive_ms != dataset_end_exclusive_ms()
    {
        return Err(ShockError::new(
            "hourly dataset was not requested for the frozen warmup plus performance window",
        ));
    }
    require_exact_grid(&dataset.candles)?;
    Ok(())
}

pub fn require_exact_grid(candles: &[HourlyCandle]) -> Result<(), ShockError> {
    if candles.is_empty() {
        return Err(ShockError::new("hourly dataset has no candles"));
    }
    let mut previous: Option<u64> = None;
    for candle in candles {
        if candle.close <= Decimal::ZERO || candle.open <= Decimal::ZERO {
            return Err(ShockError::new(format!(
                "non-positive trade price at {}",
                candle.start_time_ms
            )));
        }
        if let Some(previous) = previous {
            if candle.start_time_ms != previous + HOUR_MS {
                return Err(ShockError::new(format!(
                    "missing or off-grid hourly candle at {}",
                    previous + HOUR_MS
                )));
            }
        }
        previous = Some(candle.start_time_ms);
    }
    if candles[0].start_time_ms != dataset_start_ms() {
        return Err(ShockError::new(
            "hourly dataset does not start at the required warmup open",
        ));
    }
    if candles.last().map(|candle| candle.start_time_ms) != Some(last_eligible_open_ms()) {
        return Err(ShockError::new(
            "hourly dataset does not end on the last eligible open",
        ));
    }
    Ok(())
}

fn assemble(
    request: &DownloadRequest,
    rows: Vec<RawCandle>,
    pages_fetched: u32,
) -> Result<HourlyDataset, ShockError> {
    let mut grouped: BTreeMap<u64, Vec<RawCandle>> = BTreeMap::new();
    for row in rows {
        if row.start_time_ms + HOUR_MS > request.retrieved_at_ms {
            continue;
        }
        grouped.entry(row.start_time_ms).or_default().push(row);
    }
    let mut candles = Vec::new();
    for (start, group) in grouped {
        let first = &group[0];
        if group.iter().any(|row| !same_candle(row, first)) {
            return Err(ShockError::new(format!(
                "conflicting hourly candles at {start}"
            )));
        }
        candles.push(HourlyCandle {
            start_time_ms: start,
            open: first.open,
            high: first.high,
            low: first.low,
            close: first.close,
            volume: first.volume,
        });
    }
    let mut cursor = request.start_ms;
    let mut index = 0usize;
    while cursor < request.end_exclusive_ms {
        let Some(candle) = candles.get(index) else {
            return Err(ShockError::new(format!(
                "missing hourly trade candle at {cursor}"
            )));
        };
        if candle.start_time_ms != cursor {
            return Err(ShockError::new(format!(
                "missing hourly trade candle at {cursor}"
            )));
        }
        cursor += HOUR_MS;
        index += 1;
    }
    if index != candles.len() {
        return Err(ShockError::new(
            "hourly download contained candles outside the requested window",
        ));
    }
    let hash = dataset_hash(&candles)?;
    Ok(HourlyDataset {
        metadata: DatasetMetadata {
            format: FORMAT_NAME.to_string(),
            format_version: FORMAT_VERSION,
            venue: VENUE.to_string(),
            environment: request.environment.as_str().to_string(),
            product: PRODUCT.to_string(),
            symbol: SYMBOL.to_string(),
            interval: INTERVAL.to_string(),
            requested_start_ms: request.start_ms,
            requested_end_exclusive_ms: request.end_exclusive_ms,
            actual_start_ms: candles.first().map(|candle| candle.start_time_ms),
            actual_end_ms: candles.last().map(|candle| candle.start_time_ms),
            retrieved_at_ms: request.retrieved_at_ms,
            candle_count: candles.len(),
            pages_fetched,
            dataset_hash: hash,
        },
        candles,
    })
}

fn dataset_hash(candles: &[HourlyCandle]) -> Result<String, ShockError> {
    #[derive(Serialize)]
    struct Payload<'a> {
        format_version: u32,
        venue: &'a str,
        environment: &'a str,
        product: &'a str,
        symbol: &'a str,
        interval: &'a str,
        candles: &'a [HourlyCandle],
    }
    let payload = Payload {
        format_version: FORMAT_VERSION,
        venue: VENUE,
        environment: "mainnet",
        product: PRODUCT,
        symbol: SYMBOL,
        interval: INTERVAL,
        candles,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| ShockError::new(format!("hourly hash: {err}")))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

fn same_candle(left: &RawCandle, right: &RawCandle) -> bool {
    left.open == right.open
        && left.high == right.high
        && left.low == right.low
        && left.close == right.close
        && left.volume == right.volume
}

fn kline_url(request: &DownloadRequest, end_inclusive: u64, limit: u32) -> String {
    format!(
        "{}/v5/market/kline?category=linear&symbol={SYMBOL}&interval=60&start={}&end={end_inclusive}&limit={limit}",
        request.environment.public_rest_base(),
        request.start_ms
    )
}

fn parse_page(body: &str) -> Result<Vec<RawCandle>, ShockError> {
    let page =
        parse_bybit_public_kline_page(body).map_err(|err| ShockError::new(err.to_string()))?;
    if let Some(category) = &page.category {
        if category != PRODUCT {
            return Err(ShockError::new(format!(
                "expected category {PRODUCT}, got {category}"
            )));
        }
    }
    if let Some(symbol) = &page.symbol {
        if symbol != SYMBOL {
            return Err(ShockError::new(format!(
                "expected symbol {SYMBOL}, got {symbol}"
            )));
        }
    }
    Ok(page
        .rows
        .into_iter()
        .map(|row| RawCandle {
            start_time_ms: row.start_time_ms,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
        })
        .collect())
}

async fn fetch_public_body(client: &reqwest::Client, url: &str) -> Result<String, ShockError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| ShockError::new(format!("kline request failed: {err}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| ShockError::new(format!("kline body: {err}")))?;
    if !status.is_success() {
        return Err(ShockError::new(format!("kline http {status}: {body}")));
    }
    Ok(body)
}

pub fn from_candles(
    candles: Vec<HourlyCandle>,
    retrieved_at_ms: u64,
) -> Result<HourlyDataset, ShockError> {
    let request = frozen_download_request(retrieved_at_ms);
    let rows = candles
        .into_iter()
        .map(|candle| RawCandle {
            start_time_ms: candle.start_time_ms,
            open: candle.open,
            high: candle.high,
            low: candle.low,
            close: candle.close,
            volume: candle.volume,
        })
        .collect();
    assemble(&request, rows, 0)
}

pub fn sha256_file(path: &Path) -> Result<String, ShockError> {
    let bytes = std::fs::read(path)
        .map_err(|err| ShockError::new(format!("read {}: {err}", path.display())))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn candle(start: u64, close: i64) -> HourlyCandle {
        HourlyCandle {
            start_time_ms: start,
            open: dec!(100),
            high: dec!(101),
            low: dec!(99),
            close: Decimal::from(close),
            volume: dec!(1),
        }
    }

    #[test]
    fn frozen_window_has_warmup_before_the_first_eligible_close() {
        assert_eq!(first_eligible_open_ms() + HOUR_MS, PERFORMANCE_START_MS);
        assert_eq!(
            last_eligible_open_ms() + HOUR_MS,
            PERFORMANCE_END_EXCLUSIVE_MS - HOUR_MS
        );
        assert!(dataset_start_ms() < PERFORMANCE_START_MS);
        let span = last_eligible_open_ms() - dataset_start_ms();
        assert_eq!(span % HOUR_MS, 0);
    }

    #[test]
    fn a_missing_hour_fails_closed() {
        let mut candles = Vec::new();
        let mut cursor = dataset_start_ms();
        while cursor <= last_eligible_open_ms() {
            if cursor != dataset_start_ms() + 10 * HOUR_MS {
                candles.push(candle(cursor, 100));
            }
            cursor += HOUR_MS;
        }
        let err = require_exact_grid(&candles).unwrap_err();
        assert!(err.to_string().contains("missing or off-grid"));
    }

    #[test]
    fn mainnet_hourly_url_uses_interval_60() {
        let request = frozen_download_request(PERFORMANCE_END_EXCLUSIVE_MS);
        let url = kline_url(&request, request.end_exclusive_ms - 1, PAGE_LIMIT);
        assert!(url.starts_with("https://api.bybit.com/v5/market/kline?"));
        assert!(url.contains("interval=60"));
        assert!(url.contains("symbol=BTCUSDT"));
        assert!(!url.contains("api-demo"));
    }
}
