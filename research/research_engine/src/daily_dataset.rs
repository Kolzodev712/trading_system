//! Offline Bybit BTCUSDT daily candles and shared decision-log types.
//!
//! The public kline row layout matches `runner::cex::klines`: start, open, high,
//! low, close, volume. Download talks only to the public market-data host selected
//! by [`MarketDataEnvironment`]. Candidate-specific replay lives with each model.

pub const DAILY_INTERVAL: &str = "D";
pub const DAY_MS: u64 = 86_400_000;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;

const FORMAT_NAME: &str = "donchian_daily_v1_candles";
const FORMAT_VERSION: u32 = 1;
const VENUE: &str = "bybit";
const PRODUCT: &str = "linear";
const SYMBOL: &str = "BTCUSDT";
const PAGE_LIMIT: u32 = 1000;
const MAX_PAGES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct OfflineError {
    message: String,
}

impl OfflineError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketDataEnvironment {
    Mainnet,
    Testnet,
    Demo,
}

impl MarketDataEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Demo => "demo",
        }
    }

    /// Public market-data host. This is not read from trading configuration.
    pub fn public_rest_base(self) -> &'static str {
        match self {
            Self::Mainnet => "https://api.bybit.com",
            Self::Testnet => "https://api-testnet.bybit.com",
            Self::Demo => "https://api-demo.bybit.com",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub environment: MarketDataEnvironment,
    pub symbol: String,
    pub start_ms: u64,
    pub end_exclusive_ms: u64,
    /// Recorded on the dataset. Candle finality uses this value, not a later clock read.
    pub retrieved_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetCandle {
    pub start_time_ms: u64,
    pub end_time_exclusive_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GapRecord {
    pub missing_start_ms: u64,
    pub missing_end_exclusive_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub start_time_ms: u64,
    pub closes: Vec<Decimal>,
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
    pub actual_end_exclusive_ms: Option<u64>,
    pub retrieved_at_ms: u64,
    pub completed_cutoff_ms: u64,
    pub candle_count: usize,
    pub completed_candle_count: usize,
    /// Public kline responses fetched for this file. Zero for a dataset built in a test.
    #[serde(default)]
    pub pages_fetched: u32,
    pub gaps: Vec<GapRecord>,
    pub conflicts: Vec<ConflictRecord>,
    pub dataset_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalDataset {
    pub metadata: DatasetMetadata,
    pub candles: Vec<DatasetCandle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmittedTarget {
    NotReady,
    NoChange,
    Quantity { quantity: Decimal },
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRow {
    pub venue: String,
    pub product: String,
    pub instrument: String,
    pub interval: String,
    pub start_time_ms: u64,
    pub end_time_exclusive_ms: u64,
    pub close: Decimal,
    pub entry_high: Option<Decimal>,
    pub entry_low: Option<Decimal>,
    pub exit_high: Option<Decimal>,
    pub exit_low: Option<Decimal>,
    pub previous_direction: String,
    pub direction: String,
    pub readiness: String,
    pub suspension: Option<String>,
    pub reason: String,
    pub target: EmittedTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMetadata {
    pub strategy_id: String,
    pub strategy_version: String,
    pub config_hash: String,
    pub dataset_hash: String,
    pub venue: String,
    pub environment: String,
    pub product: String,
    pub symbol: String,
    pub interval: String,
    pub evaluation_start_ms: u64,
    pub evaluation_end_exclusive_ms: u64,
    pub actual_start_ms: Option<u64>,
    pub actual_end_exclusive_ms: Option<u64>,
    pub dataset_candles: usize,
    pub completed_candles: usize,
    pub warmup_candles: usize,
    pub evaluated_candles: usize,
    pub entries: usize,
    pub exits: usize,
    pub validation: String,
    pub final_direction: String,
    pub initial_state_policy: String,
    pub allow_short: bool,
    pub position_qty: Decimal,
    pub entry_lookback: usize,
    pub exit_lookback: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionLog {
    pub run: RunMetadata,
    pub decisions: Vec<DecisionRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawKline {
    start_time_ms: u64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

#[derive(Serialize)]
struct HashPayload<'a> {
    format_version: u32,
    venue: &'a str,
    environment: &'a str,
    product: &'a str,
    symbol: &'a str,
    interval: &'a str,
    candles: &'a [DatasetCandle],
}

pub async fn download_daily_dataset(
    request: &DownloadRequest,
) -> Result<HistoricalDataset, OfflineError> {
    download_donchian_daily_dataset(request).await
}

/// Historical name kept for existing Candidate #2 CLI wiring.
pub async fn download_donchian_daily_dataset(
    request: &DownloadRequest,
) -> Result<HistoricalDataset, OfflineError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| OfflineError::new(format!("http client: {err}")))?;
    download_with(request, PAGE_LIMIT, |url| {
        let client = client.clone();
        async move { fetch_public_body(&client, &url).await }
    })
    .await
}

pub fn save_historical_dataset(
    path: &Path,
    dataset: &HistoricalDataset,
) -> Result<(), OfflineError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| OfflineError::new(format!("create {}: {err}", parent.display())))?;
        }
    }
    let body = serde_json::to_vec_pretty(dataset)
        .map_err(|err| OfflineError::new(format!("encode dataset: {err}")))?;
    std::fs::write(path, body)
        .map_err(|err| OfflineError::new(format!("write {}: {err}", path.display())))
}

pub fn load_historical_dataset(path: &Path) -> Result<HistoricalDataset, OfflineError> {
    let body = std::fs::read(path)
        .map_err(|err| OfflineError::new(format!("read {}: {err}", path.display())))?;
    serde_json::from_slice(&body).map_err(|err| OfflineError::new(format!("decode dataset: {err}")))
}

async fn download_with<F, Fut>(
    request: &DownloadRequest,
    page_limit: u32,
    mut fetch: F,
) -> Result<HistoricalDataset, OfflineError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, OfflineError>>,
{
    validate_download_request(request)?;
    if page_limit == 0 {
        return Err(OfflineError::new("page limit must be positive"));
    }
    let mut end_inclusive = request.end_exclusive_ms.checked_sub(1).ok_or_else(|| {
        OfflineError::new("requested end must be greater than the start of the timestamp range")
    })?;
    let mut rows = Vec::new();
    let mut pages_fetched = 0u32;
    for _ in 0..MAX_PAGES {
        pages_fetched += 1;
        let url = kline_url(request, end_inclusive, page_limit);
        let body = fetch(url).await?;
        let mut page = parse_bybit_kline_page(&body)?;
        page.retain(|row| {
            row.start_time_ms >= request.start_ms && row.start_time_ms < request.end_exclusive_ms
        });
        if page.is_empty() {
            return finish_download(request, rows, pages_fetched);
        }
        let oldest = page
            .iter()
            .map(|row| row.start_time_ms)
            .min()
            .ok_or_else(|| OfflineError::new("kline page was empty after filtering"))?;
        let page_len = page.len();
        rows.extend(page);
        if oldest <= request.start_ms || page_len < page_limit as usize {
            return finish_download(request, rows, pages_fetched);
        }
        let next_end = oldest.checked_sub(1).ok_or_else(|| {
            OfflineError::new("pagination reached the start of the timestamp range")
        })?;
        if next_end >= end_inclusive {
            return Err(OfflineError::new(
                "pagination did not move backward; refusing to truncate the requested range",
            ));
        }
        end_inclusive = next_end;
    }
    Err(OfflineError::new(
        "pagination limit reached before the requested start; refusing to truncate coverage",
    ))
}

fn finish_download(
    request: &DownloadRequest,
    rows: Vec<RawKline>,
    pages_fetched: u32,
) -> Result<HistoricalDataset, OfflineError> {
    let mut dataset = assemble_dataset(request, rows)?;
    dataset.metadata.pages_fetched = pages_fetched;
    Ok(dataset)
}

async fn fetch_public_body(client: &reqwest::Client, url: &str) -> Result<String, OfflineError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| OfflineError::new(format!("kline request failed: {err}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| OfflineError::new(format!("kline body: {err}")))?;
    if !status.is_success() {
        return Err(OfflineError::new(format!("kline http {status}: {body}")));
    }
    Ok(body)
}

fn kline_url(request: &DownloadRequest, end_inclusive: u64, limit: u32) -> String {
    format!(
        "{}/v5/market/kline?category=linear&symbol={}&interval=D&start={}&end={}&limit={}",
        request.environment.public_rest_base(),
        request.symbol,
        request.start_ms,
        end_inclusive,
        limit
    )
}

fn parse_bybit_kline_page(body: &str) -> Result<Vec<RawKline>, OfflineError> {
    let page = crate::bybit_public_klines::parse_bybit_public_kline_page(body)
        .map_err(|err| OfflineError::new(err.to_string()))?;
    if let Some(category) = &page.category {
        if category != PRODUCT {
            return Err(OfflineError::new(format!(
                "expected category {PRODUCT}, got {category}"
            )));
        }
    }
    if let Some(symbol) = &page.symbol {
        if symbol != SYMBOL {
            return Err(OfflineError::new(format!(
                "expected symbol {SYMBOL}, got {symbol}"
            )));
        }
    }
    Ok(page
        .rows
        .into_iter()
        .map(|row| RawKline {
            start_time_ms: row.start_time_ms,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
        })
        .collect())
}

fn assemble_dataset(
    request: &DownloadRequest,
    rows: Vec<RawKline>,
) -> Result<HistoricalDataset, OfflineError> {
    validate_download_request(request)?;
    let mut parsed = Vec::new();
    for row in rows {
        if let Some(candle) = candle_from_raw(request, &row)? {
            parsed.push(candle);
        }
    }
    parsed.sort_by_key(|candle| candle.start_time_ms);

    let mut grouped: BTreeMap<u64, Vec<DatasetCandle>> = BTreeMap::new();
    for candle in parsed {
        grouped
            .entry(candle.start_time_ms)
            .or_default()
            .push(candle);
    }
    let mut candles = Vec::new();
    let mut conflicts = Vec::new();
    for (start, group) in grouped {
        let first = &group[0];
        if group.iter().all(|candle| same_candle(candle, first)) {
            candles.push(first.clone());
        } else {
            let mut closes: Vec<Decimal> = group.iter().map(|candle| candle.close).collect();
            closes.sort();
            closes.dedup();
            conflicts.push(ConflictRecord {
                start_time_ms: start,
                closes,
            });
        }
    }

    let completed: Vec<DatasetCandle> = candles
        .iter()
        .filter(|candle| candle.end_time_exclusive_ms <= request.retrieved_at_ms)
        .cloned()
        .collect();
    let coverage_end = completed_coverage_end(request.retrieved_at_ms, request.end_exclusive_ms);
    let in_window: Vec<DatasetCandle> = completed
        .iter()
        .filter(|candle| {
            candle.start_time_ms >= request.start_ms && candle.start_time_ms < coverage_end
        })
        .cloned()
        .collect();
    let gaps = find_gaps(&in_window, request.start_ms, coverage_end);
    let actual_start_ms = in_window.first().map(|candle| candle.start_time_ms);
    let actual_end_exclusive_ms = in_window.last().map(|candle| candle.end_time_exclusive_ms);

    let mut dataset = HistoricalDataset {
        metadata: DatasetMetadata {
            format: FORMAT_NAME.to_string(),
            format_version: FORMAT_VERSION,
            venue: VENUE.to_string(),
            environment: request.environment.as_str().to_string(),
            product: PRODUCT.to_string(),
            symbol: request.symbol.clone(),
            interval: DAILY_INTERVAL.to_string(),
            requested_start_ms: request.start_ms,
            requested_end_exclusive_ms: request.end_exclusive_ms,
            actual_start_ms,
            actual_end_exclusive_ms,
            retrieved_at_ms: request.retrieved_at_ms,
            completed_cutoff_ms: request.retrieved_at_ms,
            candle_count: candles.len(),
            completed_candle_count: in_window.len(),
            pages_fetched: 0,
            gaps,
            conflicts,
            dataset_hash: String::new(),
        },
        candles,
    };
    dataset.metadata.dataset_hash = compute_hash(&dataset)?;
    Ok(dataset)
}

fn candle_from_raw(
    request: &DownloadRequest,
    row: &RawKline,
) -> Result<Option<DatasetCandle>, OfflineError> {
    if row.start_time_ms < request.start_ms || row.start_time_ms >= request.end_exclusive_ms {
        return Ok(None);
    }
    if !row.start_time_ms.is_multiple_of(DAY_MS) {
        return Err(OfflineError::new(format!(
            "daily candle start {} is not UTC midnight",
            row.start_time_ms
        )));
    }
    let candle = DatasetCandle {
        start_time_ms: row.start_time_ms,
        end_time_exclusive_ms: row.start_time_ms.saturating_add(DAY_MS),
        open: row.open,
        high: row.high,
        low: row.low,
        close: row.close,
        volume: row.volume,
    };
    validate_daily_ohlc(&candle)?;
    Ok(Some(candle))
}

fn completed_coverage_end(cutoff_ms: u64, requested_end_ms: u64) -> u64 {
    let midnight = cutoff_ms - (cutoff_ms % DAY_MS);
    let completed_end = if cutoff_ms.is_multiple_of(DAY_MS) {
        cutoff_ms
    } else {
        midnight
    };
    requested_end_ms.min(completed_end)
}

fn find_gaps(completed: &[DatasetCandle], from: u64, to_exclusive: u64) -> Vec<GapRecord> {
    if to_exclusive <= from {
        return Vec::new();
    }
    let present: BTreeSet<u64> = completed
        .iter()
        .map(|candle| candle.start_time_ms)
        .filter(|start| *start >= from && *start < to_exclusive)
        .collect();
    let mut gaps = Vec::new();
    let mut expected = from;
    let mut gap_start = None;
    while expected < to_exclusive {
        if present.contains(&expected) {
            if let Some(start) = gap_start.take() {
                gaps.push(GapRecord {
                    missing_start_ms: start,
                    missing_end_exclusive_ms: expected,
                });
            }
        } else if gap_start.is_none() {
            gap_start = Some(expected);
        }
        expected = expected.saturating_add(DAY_MS);
    }
    if let Some(start) = gap_start {
        gaps.push(GapRecord {
            missing_start_ms: start,
            missing_end_exclusive_ms: to_exclusive,
        });
    }
    gaps
}

pub fn completed_series(dataset: &HistoricalDataset) -> Result<Vec<DatasetCandle>, OfflineError> {
    let mut candles = dataset.candles.clone();
    candles.sort_by_key(|candle| candle.start_time_ms);
    let mut unique: Vec<DatasetCandle> = Vec::new();
    for candle in candles {
        if let Some(previous) = unique.last() {
            if previous.start_time_ms == candle.start_time_ms {
                if same_candle(previous, &candle) {
                    continue;
                }
                return Err(OfflineError::new(format!(
                    "conflicting duplicate candle at {}",
                    candle.start_time_ms
                )));
            }
        }
        if candle.end_time_exclusive_ms > dataset.metadata.completed_cutoff_ms {
            continue;
        }
        unique.push(candle);
    }
    Ok(unique)
}

pub fn window_series(
    completed: &[DatasetCandle],
    evaluation_start: u64,
    evaluation_end: u64,
    lookback: usize,
) -> Result<Vec<DatasetCandle>, OfflineError> {
    let last_warmup = completed
        .iter()
        .rposition(|candle| candle.end_time_exclusive_ms == evaluation_start)
        .ok_or_else(|| {
            OfflineError::new(format!(
                "insufficient preceding warm-up: no completed candle ends at {evaluation_start}"
            ))
        })?;
    if last_warmup + 1 < lookback {
        return Err(OfflineError::new(format!(
            "insufficient preceding warm-up: found {} candles before {evaluation_start}; {lookback} are required",
            last_warmup + 1
        )));
    }
    let first = last_warmup + 1 - lookback;
    let mut series = Vec::new();
    for index in first..=last_warmup {
        if index > first
            && completed[index].start_time_ms != completed[index - 1].end_time_exclusive_ms
        {
            return Err(OfflineError::new(format!(
                "insufficient preceding warm-up: missing daily candle at {}",
                completed[index - 1].end_time_exclusive_ms
            )));
        }
        series.push(completed[index].clone());
    }

    let mut expected = evaluation_start;
    for candle in completed.iter().skip(last_warmup + 1) {
        if candle.start_time_ms >= evaluation_end {
            break;
        }
        if candle.start_time_ms != expected {
            return Err(OfflineError::new(format!(
                "missing daily candle at {expected}"
            )));
        }
        series.push(candle.clone());
        expected = candle.end_time_exclusive_ms;
    }
    if expected < evaluation_end {
        return Err(OfflineError::new(format!(
            "missing daily candle at {expected}"
        )));
    }
    if series.len() <= lookback {
        return Err(OfflineError::new(
            "evaluation range does not contain a completed candle",
        ));
    }
    Ok(series)
}

fn same_candle(left: &DatasetCandle, right: &DatasetCandle) -> bool {
    left.start_time_ms == right.start_time_ms
        && left.end_time_exclusive_ms == right.end_time_exclusive_ms
        && left.open == right.open
        && left.high == right.high
        && left.low == right.low
        && left.close == right.close
        && left.volume == right.volume
}

fn validate_daily_ohlc(candle: &DatasetCandle) -> Result<(), OfflineError> {
    if candle.end_time_exclusive_ms != candle.start_time_ms.saturating_add(DAY_MS) {
        return Err(OfflineError::new(
            "daily candle must cover [start, start+1d)",
        ));
    }
    if candle.open <= Decimal::ZERO
        || candle.high <= Decimal::ZERO
        || candle.low <= Decimal::ZERO
        || candle.close <= Decimal::ZERO
    {
        return Err(OfflineError::new("prices must be positive"));
    }
    if candle.volume < Decimal::ZERO {
        return Err(OfflineError::new("volume must be nonnegative"));
    }
    if candle.high < candle.low
        || candle.high < candle.open
        || candle.high < candle.close
        || candle.low > candle.open
        || candle.low > candle.close
    {
        return Err(OfflineError::new("ohlc is inconsistent"));
    }
    Ok(())
}

fn validate_download_request(request: &DownloadRequest) -> Result<(), OfflineError> {
    if request.symbol != SYMBOL {
        return Err(OfflineError::new(format!(
            "V1 download is {SYMBOL} linear daily candles, got {}",
            request.symbol
        )));
    }
    if request.start_ms >= request.end_exclusive_ms {
        return Err(OfflineError::new(
            "requested end must be after the requested start",
        ));
    }
    if !request.start_ms.is_multiple_of(DAY_MS) || !request.end_exclusive_ms.is_multiple_of(DAY_MS)
    {
        return Err(OfflineError::new(
            "requested coverage must start and end on UTC midnights",
        ));
    }
    if request.retrieved_at_ms == 0 {
        return Err(OfflineError::new("retrieval timestamp is required"));
    }
    Ok(())
}

fn compute_hash(dataset: &HistoricalDataset) -> Result<String, OfflineError> {
    let payload = HashPayload {
        format_version: dataset.metadata.format_version,
        venue: &dataset.metadata.venue,
        environment: &dataset.metadata.environment,
        product: &dataset.metadata.product,
        symbol: &dataset.metadata.symbol,
        interval: &dataset.metadata.interval,
        candles: &dataset.candles,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| OfflineError::new(format!("hash payload: {err}")))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}
