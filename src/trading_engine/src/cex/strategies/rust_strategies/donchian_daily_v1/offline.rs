//! Offline Bybit BTCUSDT daily candles and DonchianDailyV1 decision replay.
//!
//! The public kline row layout matches `runner::cex::klines`: start, open, high,
//! low, close, volume. Download talks only to the public market-data host selected
//! by [`MarketDataEnvironment`]. Replay calls [`super::on_candle`] and does not
//! price fills or measure performance.

use super::{
    on_candle, validate_candle, CandleId, ClosedCandle, Direction, ModelConfig, ModelError,
    ModelState, Step, SuspendReason, DAILY_INTERVAL, DAY_MS, ENTRY_LOOKBACK, EXIT_LOOKBACK,
    STRATEGY_NAME, STRATEGY_VERSION,
};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRequest {
    pub evaluation_start_ms: u64,
    pub evaluation_end_exclusive_ms: u64,
    pub position_qty: Decimal,
    pub allow_short: bool,
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

#[derive(Serialize)]
struct ConfigHashPayload<'a> {
    strategy_id: &'a str,
    strategy_version: &'a str,
    venue: &'a str,
    product: &'a str,
    instrument: &'a str,
    interval: &'a str,
    entry_lookback: usize,
    exit_lookback: usize,
    position_qty: Decimal,
    allow_short: bool,
}

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

/// Replay decisions from a local dataset. This function performs no network I/O.
pub fn replay_donchian_daily_decisions(
    dataset: &HistoricalDataset,
    request: &ReplayRequest,
) -> Result<DecisionLog, OfflineError> {
    validate_replay_request(request)?;
    validate_dataset_identity(dataset)?;
    verify_hash(dataset)?;
    if !dataset.metadata.conflicts.is_empty() {
        return Err(OfflineError::new(
            "dataset records conflicting candles; refusing to replay",
        ));
    }

    let completed = completed_series(dataset)?;
    let config = model_config(&dataset.metadata, request.position_qty, request.allow_short)?;
    let lookback = config.entry_lookback;
    let series = window_series(
        &completed,
        request.evaluation_start_ms,
        request.evaluation_end_exclusive_ms,
        lookback,
    )?;

    let mut state = ModelState::new();
    let mut decisions = Vec::with_capacity(series.len());
    for (index, candle) in series.iter().enumerate() {
        let closed = to_closed(&dataset.metadata, candle)?;
        let previous = direction_name(state.direction);
        let (next, step) = on_candle(&state, &config, &closed).map_err(model_error)?;
        if index < lookback && !matches!(step, Step::NotReady { .. }) {
            return Err(OfflineError::new(format!(
                "warm-up candle {} produced a decision; candles before the evaluation boundary are not evaluated",
                candle.start_time_ms
            )));
        }
        if let Some(observed) = step_close(&step) {
            if observed != candle.close {
                return Err(OfflineError::new(
                    "model diagnostics close does not match the dataset candle",
                ));
            }
        }
        if index + 1 == lookback && next.preceding_len() != lookback {
            return Err(OfflineError::new(
                "warm-up did not leave the required number of preceding candles",
            ));
        }
        decisions.push(decision_row(
            &dataset.metadata,
            candle,
            &previous,
            &next,
            &step,
        ));
        state = next;
    }
    if state.direction != Direction::Flat && decisions.len() == lookback {
        return Err(OfflineError::new(
            "warm-up changed model direction before evaluation",
        ));
    }
    if state.entries_suspended.is_some() {
        return Err(OfflineError::new(
            "model suspended during a contiguous replay window",
        ));
    }

    let evaluated = &decisions[lookback..];
    let entries = evaluated.iter().filter(|row| is_entry(row)).count();
    let exits = evaluated.iter().filter(|row| is_exit(row)).count();
    let policy = initial_state_policy(lookback);
    Ok(DecisionLog {
        run: RunMetadata {
            strategy_id: STRATEGY_NAME.to_string(),
            strategy_version: STRATEGY_VERSION.to_string(),
            config_hash: config_hash(&config)?,
            dataset_hash: dataset.metadata.dataset_hash.clone(),
            venue: dataset.metadata.venue.clone(),
            environment: dataset.metadata.environment.clone(),
            product: dataset.metadata.product.clone(),
            symbol: dataset.metadata.symbol.clone(),
            interval: dataset.metadata.interval.clone(),
            evaluation_start_ms: request.evaluation_start_ms,
            evaluation_end_exclusive_ms: request.evaluation_end_exclusive_ms,
            actual_start_ms: dataset.metadata.actual_start_ms,
            actual_end_exclusive_ms: dataset.metadata.actual_end_exclusive_ms,
            dataset_candles: dataset.metadata.candle_count,
            completed_candles: dataset.metadata.completed_candle_count,
            warmup_candles: lookback,
            evaluated_candles: evaluated.len(),
            entries,
            exits,
            validation: "ok".to_string(),
            final_direction: direction_name(state.direction),
            initial_state_policy: policy,
            allow_short: request.allow_short,
            position_qty: request.position_qty,
            entry_lookback: config.entry_lookback,
            exit_lookback: config.exit_lookback,
        },
        decisions,
    })
}

fn initial_state_policy(lookback: usize) -> String {
    format!(
        "Model starts flat with no target. Only the {lookback} completed daily candles immediately before evaluation_start are applied, and only as warm-up. Earlier candles are not passed to the model, so breakouts before the evaluation boundary do not create a carried position. Decision evaluation begins at evaluation_start."
    )
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
    let page = crate::cex::bybit_public_klines::parse_bybit_public_kline_page(body)
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
    let config = model_config_for(
        VENUE,
        PRODUCT,
        &request.symbol,
        DAILY_INTERVAL,
        Decimal::ONE,
        false,
    )?;
    let closed = closed_candle(VENUE, PRODUCT, &request.symbol, DAILY_INTERVAL, &candle);
    validate_candle(&config, &closed).map_err(model_error)?;
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

fn decision_row(
    metadata: &DatasetMetadata,
    candle: &DatasetCandle,
    previous_direction: &str,
    next: &ModelState,
    step: &Step,
) -> DecisionRow {
    let view = view_step(step);
    DecisionRow {
        venue: metadata.venue.clone(),
        product: metadata.product.clone(),
        instrument: metadata.symbol.clone(),
        interval: metadata.interval.clone(),
        start_time_ms: candle.start_time_ms,
        end_time_exclusive_ms: candle.end_time_exclusive_ms,
        close: candle.close,
        entry_high: view.entry_high,
        entry_low: view.entry_low,
        exit_high: view.exit_high,
        exit_low: view.exit_low,
        previous_direction: previous_direction.to_string(),
        direction: direction_name(next.direction),
        readiness: view.readiness.to_string(),
        suspension: view.suspension.map(str::to_string),
        reason: view.reason,
        target: view.target,
    }
}

struct StepView {
    readiness: &'static str,
    suspension: Option<&'static str>,
    reason: String,
    target: EmittedTarget,
    entry_high: Option<Decimal>,
    entry_low: Option<Decimal>,
    exit_high: Option<Decimal>,
    exit_low: Option<Decimal>,
}

fn view_step(step: &Step) -> StepView {
    match step {
        Step::NotReady {
            preceding,
            required,
        } => StepView {
            readiness: "not_ready",
            suspension: None,
            reason: format!("not ready: {preceding} of {required} preceding candles"),
            target: EmittedTarget::NotReady,
            entry_high: None,
            entry_low: None,
            exit_high: None,
            exit_low: None,
        },
        Step::NoChange { diagnostics } => StepView {
            readiness: "ready",
            suspension: None,
            reason: diagnostics.reason.to_string(),
            target: EmittedTarget::NoChange,
            entry_high: diagnostics.entry_high,
            entry_low: diagnostics.entry_low,
            exit_high: diagnostics.exit_high,
            exit_low: diagnostics.exit_low,
        },
        Step::Transition {
            target_qty,
            diagnostics,
            ..
        } => StepView {
            readiness: "ready",
            suspension: None,
            reason: diagnostics.reason.to_string(),
            target: EmittedTarget::Quantity {
                quantity: *target_qty,
            },
            entry_high: diagnostics.entry_high,
            entry_low: diagnostics.entry_low,
            exit_high: diagnostics.exit_high,
            exit_low: diagnostics.exit_low,
        },
        Step::Suspended { reason } => StepView {
            readiness: "suspended",
            suspension: Some(suspend_name(*reason)),
            reason: "suspended".to_string(),
            target: EmittedTarget::Suspended,
            entry_high: None,
            entry_low: None,
            exit_high: None,
            exit_low: None,
        },
        Step::IgnoredDuplicate => StepView {
            readiness: "ignored",
            suspension: None,
            reason: "identical duplicate".to_string(),
            target: EmittedTarget::NoChange,
            entry_high: None,
            entry_low: None,
            exit_high: None,
            exit_low: None,
        },
        Step::IgnoredForming => StepView {
            readiness: "ignored",
            suspension: None,
            reason: "forming candle".to_string(),
            target: EmittedTarget::NotReady,
            entry_high: None,
            entry_low: None,
            exit_high: None,
            exit_low: None,
        },
    }
}

fn step_close(step: &Step) -> Option<Decimal> {
    match step {
        Step::NoChange { diagnostics } | Step::Transition { diagnostics, .. } => {
            Some(diagnostics.close)
        }
        _ => None,
    }
}

fn is_entry(row: &DecisionRow) -> bool {
    row.readiness == "ready"
        && row.previous_direction == "flat"
        && matches!(row.direction.as_str(), "long" | "short")
        && matches!(row.target, EmittedTarget::Quantity { quantity } if quantity != Decimal::ZERO)
}

fn is_exit(row: &DecisionRow) -> bool {
    row.previous_direction != "flat"
        && row.direction == "flat"
        && matches!(row.target, EmittedTarget::Quantity { quantity } if quantity == Decimal::ZERO)
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

fn to_closed(
    metadata: &DatasetMetadata,
    candle: &DatasetCandle,
) -> Result<ClosedCandle, OfflineError> {
    Ok(closed_candle(
        &metadata.venue,
        &metadata.product,
        &metadata.symbol,
        &metadata.interval,
        candle,
    ))
}

fn closed_candle(
    venue: &str,
    product: &str,
    symbol: &str,
    interval: &str,
    candle: &DatasetCandle,
) -> ClosedCandle {
    ClosedCandle {
        id: CandleId {
            venue: venue.to_string(),
            product: product.to_string(),
            instrument: symbol.to_string(),
            interval: interval.to_string(),
            start_time_ms: candle.start_time_ms,
        },
        end_time_exclusive_ms: candle.end_time_exclusive_ms,
        open: candle.open,
        high: candle.high,
        low: candle.low,
        close: candle.close,
        volume: candle.volume,
        confirmed: true,
    }
}

fn model_config(
    metadata: &DatasetMetadata,
    position_qty: Decimal,
    allow_short: bool,
) -> Result<ModelConfig, OfflineError> {
    model_config_for(
        &metadata.venue,
        &metadata.product,
        &metadata.symbol,
        &metadata.interval,
        position_qty,
        allow_short,
    )
}

fn model_config_for(
    venue: &str,
    product: &str,
    symbol: &str,
    interval: &str,
    position_qty: Decimal,
    allow_short: bool,
) -> Result<ModelConfig, OfflineError> {
    let config = ModelConfig {
        venue: venue.to_string(),
        product: product.to_string(),
        instrument: symbol.to_string(),
        interval: interval.to_string(),
        entry_lookback: ENTRY_LOOKBACK,
        exit_lookback: EXIT_LOOKBACK,
        position_qty,
        allow_short,
    };
    config.validate().map_err(model_error)?;
    Ok(config)
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

fn validate_replay_request(request: &ReplayRequest) -> Result<(), OfflineError> {
    if request.position_qty <= Decimal::ZERO {
        return Err(OfflineError::new("position quantity must be positive"));
    }
    if request.evaluation_start_ms >= request.evaluation_end_exclusive_ms {
        return Err(OfflineError::new("evaluation range is empty"));
    }
    Ok(())
}

fn validate_dataset_identity(dataset: &HistoricalDataset) -> Result<(), OfflineError> {
    if dataset.metadata.format != FORMAT_NAME || dataset.metadata.format_version != FORMAT_VERSION {
        return Err(OfflineError::new("unsupported dataset format"));
    }
    if dataset.metadata.venue.is_empty()
        || dataset.metadata.product.is_empty()
        || dataset.metadata.symbol.is_empty()
        || dataset.metadata.interval.is_empty()
    {
        return Err(OfflineError::new(
            "dataset identity is missing venue, product, symbol, or interval",
        ));
    }
    Ok(())
}

fn verify_hash(dataset: &HistoricalDataset) -> Result<(), OfflineError> {
    let actual = compute_hash(dataset)?;
    if actual != dataset.metadata.dataset_hash {
        return Err(OfflineError::new(
            "dataset hash does not match the stored candles",
        ));
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

fn config_hash(config: &ModelConfig) -> Result<String, OfflineError> {
    let payload = ConfigHashPayload {
        strategy_id: STRATEGY_NAME,
        strategy_version: STRATEGY_VERSION,
        venue: &config.venue,
        product: &config.product,
        instrument: &config.instrument,
        interval: &config.interval,
        entry_lookback: config.entry_lookback,
        exit_lookback: config.exit_lookback,
        position_qty: config.position_qty,
        allow_short: config.allow_short,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| OfflineError::new(format!("config hash: {err}")))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

fn direction_name(direction: Direction) -> String {
    match direction {
        Direction::Flat => "flat",
        Direction::Long => "long",
        Direction::Short => "short",
    }
    .to_string()
}

fn suspend_name(reason: SuspendReason) -> &'static str {
    match reason {
        SuspendReason::Gap => "gap",
        SuspendReason::Correction => "correction",
    }
}

fn model_error(err: ModelError) -> OfflineError {
    match err {
        ModelError::Config(message) | ModelError::Candle(message) => OfflineError::new(message),
    }
}

#[cfg(test)]
fn seal(dataset: &mut HistoricalDataset) -> Result<(), OfflineError> {
    dataset.metadata.candle_count = dataset.candles.len();
    dataset.metadata.dataset_hash = compute_hash(dataset)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(end_day: u64, cutoff_ms: u64) -> DownloadRequest {
        DownloadRequest {
            environment: MarketDataEnvironment::Mainnet,
            symbol: SYMBOL.to_string(),
            start_ms: 0,
            end_exclusive_ms: end_day * DAY_MS,
            retrieved_at_ms: cutoff_ms,
        }
    }

    fn raw(day: u64, high: i64, low: i64, close: i64) -> RawKline {
        RawKline {
            start_time_ms: day * DAY_MS,
            open: Decimal::new(close, 0),
            high: Decimal::new(high, 0),
            low: Decimal::new(low, 0),
            close: Decimal::new(close, 0),
            volume: Decimal::ONE,
        }
    }

    fn flat_days(days: impl Iterator<Item = u64>) -> Vec<RawKline> {
        days.map(|day| raw(day, 100, 90, 95)).collect()
    }

    fn eval_request(start_day: u64, end_day: u64) -> ReplayRequest {
        ReplayRequest {
            evaluation_start_ms: start_day * DAY_MS,
            evaluation_end_exclusive_ms: end_day * DAY_MS,
            position_qty: Decimal::new(1, 3),
            allow_short: true,
        }
    }

    fn hand_checked_rows() -> Vec<RawKline> {
        let mut rows = flat_days(0..20);
        rows.push(raw(20, 110, 100, 105));
        rows.push(raw(21, 112, 100, 108));
        rows.push(raw(22, 80, 70, 75));
        rows
    }

    #[test]
    fn hand_checked_fixture_records_entry_hold_and_exit() {
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        let log = replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap();
        assert_eq!(log.run.validation, "ok");
        assert_eq!(log.run.warmup_candles, 20);
        assert_eq!(log.run.evaluated_candles, 3);
        assert_eq!(log.run.entries, 1);
        assert_eq!(log.run.exits, 1);
        assert_eq!(log.run.final_direction, "flat");
        assert!(log.decisions[..20]
            .iter()
            .all(|row| row.readiness == "not_ready" && row.entry_high.is_none()));
        assert!(log.decisions[..20]
            .iter()
            .all(|row| row.target == EmittedTarget::NotReady));

        let entry = &log.decisions[20];
        assert_eq!(entry.start_time_ms, 20 * DAY_MS);
        assert_eq!(entry.close, Decimal::new(105, 0));
        assert_eq!(entry.entry_high, Some(Decimal::new(100, 0)));
        assert_eq!(entry.entry_low, Some(Decimal::new(90, 0)));
        assert_eq!(entry.exit_high, Some(Decimal::new(100, 0)));
        assert_eq!(entry.exit_low, Some(Decimal::new(90, 0)));
        assert_eq!(entry.previous_direction, "flat");
        assert_eq!(entry.direction, "long");
        assert_eq!(entry.reason, "close above entry high");
        assert_eq!(
            entry.target,
            EmittedTarget::Quantity {
                quantity: Decimal::new(1, 3)
            }
        );

        let held = &log.decisions[21];
        assert_eq!(held.entry_high, Some(Decimal::new(110, 0)));
        assert_eq!(held.previous_direction, "long");
        assert_eq!(held.direction, "long");
        assert_eq!(held.target, EmittedTarget::NoChange);
        assert_eq!(held.reason, "long retained");

        let exit = &log.decisions[22];
        assert_eq!(exit.exit_low, Some(Decimal::new(90, 0)));
        assert_eq!(exit.previous_direction, "long");
        assert_eq!(exit.direction, "flat");
        assert_eq!(exit.reason, "close below exit low");
        assert_eq!(
            exit.target,
            EmittedTarget::Quantity {
                quantity: Decimal::ZERO
            }
        );
    }

    #[test]
    fn identical_inputs_produce_the_same_decision_log() {
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        let request = eval_request(20, 23);
        let first = replay_donchian_daily_decisions(&dataset, &request).unwrap();
        let second = replay_donchian_daily_decisions(&dataset, &request).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
    }

    #[test]
    fn future_candles_do_not_change_earlier_decisions() {
        let mut rows = hand_checked_rows();
        rows.extend(flat_days(23..30));
        let dataset = assemble_dataset(&request(30, 100 * DAY_MS), rows).unwrap();
        let through_exit =
            replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap();
        let further = replay_donchian_daily_decisions(&dataset, &eval_request(20, 26)).unwrap();
        assert_eq!(
            through_exit.decisions,
            further.decisions[..through_exit.decisions.len()]
        );

        let mut shorter_rows = hand_checked_rows();
        shorter_rows.extend(flat_days(23..26));
        let shorter = assemble_dataset(&request(26, 100 * DAY_MS), shorter_rows).unwrap();
        let without_tail =
            replay_donchian_daily_decisions(&shorter, &eval_request(20, 23)).unwrap();
        assert_eq!(without_tail.decisions, through_exit.decisions);
        assert_ne!(
            without_tail.run.dataset_hash, further.run.dataset_hash,
            "the extra candles change the dataset hash without changing the earlier decisions"
        );
    }

    #[test]
    fn evaluation_starts_flat_and_does_not_carry_an_earlier_breakout() {
        let mut rows = flat_days(0..20);
        rows.push(raw(20, 150, 100, 140));
        rows.extend(flat_days(21..41));
        rows.push(raw(41, 110, 100, 105));
        let dataset = assemble_dataset(&request(42, 100 * DAY_MS), rows).unwrap();
        let log = replay_donchian_daily_decisions(&dataset, &eval_request(41, 42)).unwrap();
        assert!(log
            .decisions
            .iter()
            .all(|row| row.start_time_ms != 20 * DAY_MS));
        assert!(log.decisions[..20]
            .iter()
            .all(|row| row.direction == "flat" && row.readiness == "not_ready"));
        let decision = log.decisions.last().unwrap();
        assert_eq!(decision.start_time_ms, 41 * DAY_MS);
        assert_eq!(decision.previous_direction, "flat");
        assert_eq!(decision.direction, "long");
        assert_eq!(decision.entry_high, Some(Decimal::new(100, 0)));
        assert_eq!(log.run.final_direction, "long");
    }

    #[test]
    fn identical_duplicate_is_removed_before_decisions() {
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        let clean = replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap();
        let mut duplicated = dataset.clone();
        duplicated.candles.push(dataset.candles[20].clone());
        seal(&mut duplicated).unwrap();
        let replayed = replay_donchian_daily_decisions(&duplicated, &eval_request(20, 23)).unwrap();
        assert_eq!(replayed.decisions, clean.decisions);
        assert_eq!(replayed.run.evaluated_candles, 3);
    }

    #[test]
    fn conflicting_duplicate_and_recorded_conflict_are_rejected() {
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        let mut conflict = dataset.clone();
        let mut changed = conflict.candles[21].clone();
        changed.close = Decimal::new(109, 0);
        changed.high = Decimal::new(112, 0);
        conflict.candles.push(changed);
        seal(&mut conflict).unwrap();
        let err = replay_donchian_daily_decisions(&conflict, &eval_request(20, 23)).unwrap_err();
        assert!(err.to_string().contains("conflicting duplicate"));

        let mut recorded = dataset.clone();
        recorded.metadata.conflicts.push(ConflictRecord {
            start_time_ms: 21 * DAY_MS,
            closes: vec![Decimal::new(108, 0), Decimal::new(109, 0)],
        });
        seal(&mut recorded).unwrap();
        let err = replay_donchian_daily_decisions(&recorded, &eval_request(20, 23)).unwrap_err();
        assert!(err.to_string().contains("conflicting candles"));
    }

    #[test]
    fn missing_daily_interval_is_rejected() {
        let mut rows = hand_checked_rows();
        rows.retain(|row| row.start_time_ms != 21 * DAY_MS);
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), rows).unwrap();
        assert!(!dataset.metadata.gaps.is_empty());
        let err = replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap_err();
        assert!(err.to_string().contains("missing daily candle"));
    }

    #[test]
    fn insufficient_warmup_is_rejected_without_a_flat_target() {
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        let err = replay_donchian_daily_decisions(&dataset, &eval_request(10, 23)).unwrap_err();
        assert!(err.to_string().contains("insufficient preceding warm-up"));
    }

    #[test]
    fn unfinished_candle_is_excluded_and_a_hole_in_the_window_is_rejected() {
        let mut rows = hand_checked_rows();
        rows.push(raw(23, 100, 90, 95));
        let dataset = assemble_dataset(&request(24, 24 * DAY_MS), rows.clone()).unwrap();
        assert_eq!(dataset.metadata.completed_candle_count, 24);
        let log = replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap();
        assert!(log
            .decisions
            .iter()
            .all(|row| row.start_time_ms < 23 * DAY_MS));

        let open = assemble_dataset(&request(24, 23 * DAY_MS + 1_000), rows).unwrap();
        assert!(open
            .candles
            .iter()
            .any(|candle| candle.start_time_ms == 23 * DAY_MS));
        assert_eq!(open.metadata.completed_candle_count, 23);
        let err = replay_donchian_daily_decisions(&open, &eval_request(20, 24)).unwrap_err();
        assert!(err.to_string().contains("missing daily candle"));
    }

    #[test]
    fn replay_reads_the_local_dataset_without_network_access() {
        let dataset = assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        let log = replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap();
        assert_eq!(log.decisions.len(), 23);
        assert!(log.run.dataset_hash.starts_with("sha256:"));
    }

    #[test]
    fn mainnet_download_url_is_the_public_market_host() {
        let request = request(23, 100 * DAY_MS);
        let url = kline_url(&request, request.end_exclusive_ms - 1, PAGE_LIMIT);
        assert!(url.starts_with("https://api.bybit.com/v5/market/kline?"));
        assert!(url.contains("category=linear"));
        assert!(url.contains("symbol=BTCUSDT"));
        assert!(url.contains("interval=D"));
        assert!(!url.contains("api-demo"));
        assert!(!url.contains("api-testnet"));
        assert_eq!(
            MarketDataEnvironment::Demo.public_rest_base(),
            "https://api-demo.bybit.com"
        );
    }

    #[test]
    fn captured_bybit_page_sorts_and_hashes_without_another_request() {
        let body = r#"{"retCode":0,"retMsg":"OK","result":{"symbol":"BTCUSDT","category":"linear","list":[["1704326400000","42871.9","44875.5","42637","44156.3","164677.973","7207270337.2467"],["1704240000000","44989","45567.8","40210","42871.9","308217.553","13228994107.3449"],["1704153600000","44235.5","45946.5","44208.7","44989","180530.083","8166222223.7952"],["1704067200000","42324.8","44284.9","42211.2","44235.5","79123.439","3416032597.2288"]]},"retExtInfo":{},"time":1790499487471}"#;
        let rows = parse_bybit_kline_page(body).unwrap();
        let request = DownloadRequest {
            environment: MarketDataEnvironment::Mainnet,
            symbol: SYMBOL.to_string(),
            start_ms: 1_704_067_200_000,
            end_exclusive_ms: 1_704_412_800_000,
            retrieved_at_ms: 1_790_499_487_471,
        };
        let dataset = assemble_dataset(&request, rows).unwrap();
        assert!(dataset.metadata.gaps.is_empty());
        assert!(dataset.metadata.conflicts.is_empty());
        assert_eq!(dataset.metadata.completed_candle_count, 4);
        assert_eq!(dataset.candles[0].start_time_ms, 1_704_067_200_000);
        assert_eq!(
            dataset.candles[0].close,
            Decimal::from_str_exact("44235.5").unwrap()
        );
        assert!(dataset.candles[0].start_time_ms < dataset.candles[1].start_time_ms);
        verify_hash(&dataset).unwrap();
    }

    #[test]
    fn api_error_and_pagination_do_not_silently_drop_days() {
        let err =
            parse_bybit_kline_page(r#"{"retCode":10001,"retMsg":"params error","result":{}}"#)
                .unwrap_err();
        assert!(err.to_string().contains("10001"));

        let stored: Vec<RawKline> = (0..7).map(|day| raw(day, 100, 90, 95)).collect();
        let request = request(7, 100 * DAY_MS);
        let dataset = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(download_with(&request, 3, |url| {
                let stored = stored.clone();
                async move {
                    assert!(url.starts_with("https://api.bybit.com/"));
                    assert!(!url.contains("demo"));
                    let end = query_u64(&url, "end");
                    let mut page: Vec<&RawKline> = stored
                        .iter()
                        .filter(|row| row.start_time_ms <= end)
                        .collect();
                    page.sort_by_key(|row| std::cmp::Reverse(row.start_time_ms));
                    page.truncate(3);
                    let list: Vec<Vec<String>> = page
                        .iter()
                        .map(|row| {
                            vec![
                                row.start_time_ms.to_string(),
                                row.open.to_string(),
                                row.high.to_string(),
                                row.low.to_string(),
                                row.close.to_string(),
                                row.volume.to_string(),
                            ]
                        })
                        .collect();
                    Ok(serde_json::json!({
                        "retCode": 0,
                        "retMsg": "OK",
                        "result": {"symbol": "BTCUSDT", "category": "linear", "list": list}
                    })
                    .to_string())
                }
            }))
            .unwrap();
        assert_eq!(dataset.metadata.completed_candle_count, 7);
        assert_eq!(dataset.metadata.pages_fetched, 3);
        assert!(dataset.metadata.gaps.is_empty());
        assert_eq!(dataset.candles[0].start_time_ms, 0);
        assert_eq!(dataset.candles[6].start_time_ms, 6 * DAY_MS);
    }

    #[test]
    fn replay_uses_the_identity_stored_in_the_dataset() {
        let mut dataset =
            assemble_dataset(&request(23, 100 * DAY_MS), hand_checked_rows()).unwrap();
        dataset.metadata.symbol = "ETHUSDT".to_string();
        seal(&mut dataset).unwrap();
        let log = replay_donchian_daily_decisions(&dataset, &eval_request(20, 23)).unwrap();
        assert!(log
            .decisions
            .iter()
            .all(|row| row.instrument == "ETHUSDT" && row.venue == "bybit"));
        assert_eq!(log.run.symbol, "ETHUSDT");

        let err = validate_download_request(&DownloadRequest {
            environment: MarketDataEnvironment::Mainnet,
            symbol: "ETHUSDT".to_string(),
            start_ms: 0,
            end_exclusive_ms: DAY_MS,
            retrieved_at_ms: 2 * DAY_MS,
        })
        .unwrap_err();
        assert!(err.to_string().contains("BTCUSDT"));
    }

    #[test]
    fn replay_follows_each_candle_end_instead_of_the_unix_midnight_grid() {
        let origin = 3_600_000u64;
        let mut candles = Vec::new();
        for day in 0..23 {
            let start = origin + day * DAY_MS;
            let close = match day {
                20 => 105,
                21 => 108,
                22 => 75,
                _ => 95,
            };
            let (high, low) = if day == 20 {
                (110, 100)
            } else if day == 22 {
                (80, 70)
            } else if day == 21 {
                (112, 100)
            } else {
                (100, 90)
            };
            candles.push(DatasetCandle {
                start_time_ms: start,
                end_time_exclusive_ms: start + DAY_MS,
                open: Decimal::new(close, 0),
                high: Decimal::new(high, 0),
                low: Decimal::new(low, 0),
                close: Decimal::new(close, 0),
                volume: Decimal::ONE,
            });
        }
        let mut dataset = assemble_dataset(&request(1, DAY_MS), Vec::new()).unwrap();
        dataset.candles = candles;
        dataset.metadata.completed_cutoff_ms = origin + 100 * DAY_MS;
        dataset.metadata.completed_candle_count = dataset.candles.len();
        dataset.metadata.gaps.clear();
        dataset.metadata.actual_start_ms = Some(origin);
        dataset.metadata.actual_end_exclusive_ms = Some(origin + 23 * DAY_MS);
        seal(&mut dataset).unwrap();
        let log = replay_donchian_daily_decisions(
            &dataset,
            &ReplayRequest {
                evaluation_start_ms: origin + 20 * DAY_MS,
                evaluation_end_exclusive_ms: origin + 23 * DAY_MS,
                position_qty: Decimal::new(1, 3),
                allow_short: true,
            },
        )
        .unwrap();
        assert_eq!(log.decisions[20].start_time_ms, origin + 20 * DAY_MS);
        assert_eq!(log.decisions[20].direction, "long");
        assert_eq!(log.decisions[22].direction, "flat");
        assert_eq!(log.run.entries, 1);
        assert_eq!(log.run.exits, 1);
    }

    fn query_u64(url: &str, key: &str) -> u64 {
        url.split_once('?')
            .unwrap()
            .1
            .split('&')
            .find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name == key).then(|| value.parse::<u64>().unwrap())
            })
            .unwrap()
    }
}
