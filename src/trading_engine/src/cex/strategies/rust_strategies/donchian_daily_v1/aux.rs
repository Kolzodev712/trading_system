//! Public Bybit execution, funding, and mark datasets for the DonchianDailyV1 backtest.
//!
//! Download is separate from evaluation. After these files exist, the simulator
//! reads only the local cache.

use super::backtest::{
    required_execution_minutes_for_experiment, CoverageNote, FundingRecord, MarketInputs, HOUR_MS,
    MINUTE_MS,
};
use super::offline::{DecisionRow, MarketDataEnvironment};
use crate::cex::bybit_public_klines::parse_bybit_public_kline_page;
use crate::cex::bybit_public_market::{
    parse_bybit_funding_history, parse_bybit_mark_price_kline_page,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;

const FORMAT_VERSION: u32 = 1;
const VENUE: &str = "bybit";
const PRODUCT: &str = "linear";
const SYMBOL: &str = "BTCUSDT";
const TRADE_ENDPOINT: &str = "/v5/market/kline";
const MARK_ENDPOINT: &str = "/v5/market/mark-price-kline";
const FUNDING_ENDPOINT: &str = "/v5/market/funding/history";
const FUNDING_MARGIN_MS: u64 = 86_400_000;
const PAGE_LIMIT_KLINE: usize = 1000;
const PAGE_LIMIT_FUNDING: usize = 200;
const MAX_PAGES: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxError {
    message: String,
}

impl AuxError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AuxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AuxError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxDownloadRequest {
    pub environment: MarketDataEnvironment,
    pub performance_start_ms: u64,
    pub performance_end_exclusive_ms: u64,
    pub retrieved_at_ms: u64,
    pub position_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuxGap {
    pub missing_start_ms: u64,
    pub missing_end_exclusive_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesMetadata {
    pub format: String,
    pub format_version: u32,
    pub venue: String,
    pub environment: String,
    pub product: String,
    pub symbol: String,
    pub series: String,
    pub source_endpoint: String,
    pub requested_start_ms: u64,
    pub requested_end_exclusive_ms: u64,
    pub actual_start_ms: Option<u64>,
    pub actual_end_ms: Option<u64>,
    pub retrieved_at_ms: u64,
    pub count: usize,
    pub pages_fetched: u32,
    pub pagination_complete: bool,
    pub gaps: Vec<AuxGap>,
    pub observed_intervals_ms: Vec<u64>,
    pub notes: Vec<String>,
    pub dataset_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricePoint {
    pub time_ms: u64,
    pub open: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceSeriesFile {
    pub metadata: SeriesMetadata,
    pub points: Vec<PricePoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingPoint {
    pub settlement_time_ms: u64,
    pub funding_rate: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingFile {
    pub metadata: SeriesMetadata,
    pub records: Vec<FundingPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxBundle {
    pub trade_minutes: PriceSeriesFile,
    pub funding: FundingFile,
    pub hourly_marks: PriceSeriesFile,
    pub finer_marks: Option<PriceSeriesFile>,
}

struct Timed<T> {
    time_ms: u64,
    value: T,
}

pub async fn download_aux_bundle(
    request: &AuxDownloadRequest,
    decisions: &[DecisionRow],
) -> Result<AuxBundle, AuxError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| AuxError::new(format!("http client: {err}")))?;
    download_aux_with(request, decisions, |url| {
        let client = client.clone();
        async move { fetch_public_body(&client, &url).await }
    })
    .await
}

pub async fn download_aux_with<F, Fut>(
    request: &AuxDownloadRequest,
    decisions: &[DecisionRow],
    mut fetch: F,
) -> Result<AuxBundle, AuxError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, AuxError>>,
{
    validate_request(request)?;
    let required = required_execution_minutes_for_experiment(
        decisions,
        request.performance_start_ms,
        request.performance_end_exclusive_ms,
        request.position_qty,
    )
    .map_err(|err| AuxError::new(err.to_string()))?;
    let windows = merge_minute_windows(&required, 10 * MINUTE_MS);
    let mut trade_points = BTreeMap::new();
    let mut trade_pages = 0u32;
    for (start, end) in &windows {
        let url = format!(
            "{}/v5/market/kline?category=linear&symbol={SYMBOL}&interval=1&start={start}&end={end}&limit={PAGE_LIMIT_KLINE}",
            request.environment.public_rest_base()
        );
        let body = fetch(url).await?;
        trade_pages += 1;
        let page =
            parse_bybit_public_kline_page(&body).map_err(|err| AuxError::new(err.to_string()))?;
        check_identity(page.category.as_deref(), page.symbol.as_deref())?;
        for row in page.rows {
            insert_price(&mut trade_points, row.start_time_ms, row.open)?;
        }
    }
    let trade_gaps = missing_hours_as_gaps(&required, &trade_points);
    let trade_minutes = seal_prices(
        request,
        SeriesSpec {
            format: "donchian_daily_v1_trade_1m",
            series: "trade_1m",
            endpoint: TRADE_ENDPOINT,
            requested_start_ms: request.performance_start_ms,
            requested_end_exclusive_ms: request.performance_end_exclusive_ms,
            pages_fetched: trade_pages,
            pagination_complete: true,
            gaps: trade_gaps,
            notes: vec![format!(
                "one-minute trade opens for {} execution minutes across {} merged windows; not a full three-year tape",
                required.len(),
                windows.len()
            )],
        },
        trade_points,
    )?;

    let funding_start = request
        .performance_start_ms
        .saturating_sub(FUNDING_MARGIN_MS);
    let funding_end = request
        .performance_end_exclusive_ms
        .saturating_add(FUNDING_MARGIN_MS);
    let (funding_rows, funding_pages, funding_complete) = walk_backward(
        funding_start,
        funding_end,
        PAGE_LIMIT_FUNDING,
        MAX_PAGES,
        |end_inclusive| {
            let url = format!(
                "{}/v5/market/funding/history?category=linear&symbol={SYMBOL}&startTime={funding_start}&endTime={end_inclusive}&limit={PAGE_LIMIT_FUNDING}",
                request.environment.public_rest_base()
            );
            let fetch_future = fetch(url);
            async move {
                let body = fetch_future.await?;
                let page = parse_bybit_funding_history(&body)
                    .map_err(|err| AuxError::new(err.to_string()))?;
                if let Some(category) = &page.category {
                    if category != PRODUCT {
                        return Err(AuxError::new(format!(
                            "expected category {PRODUCT}, got {category}"
                        )));
                    }
                }
                page.rows
                    .into_iter()
                    .map(|row| {
                        if let Some(symbol) = &row.symbol {
                            if symbol != SYMBOL {
                                return Err(AuxError::new(format!(
                                    "expected symbol {SYMBOL}, got {symbol}"
                                )));
                            }
                        }
                        Ok(Timed {
                            time_ms: row.funding_rate_timestamp_ms,
                            value: row.funding_rate,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            }
        },
    )
    .await?;
    let funding_points = dedupe_values(funding_rows)?;
    let funding_intervals = observed_intervals(funding_points.keys().copied());
    let funding = seal_funding(
        request,
        funding_start,
        funding_end.saturating_add(1),
        funding_points,
        funding_pages,
        funding_complete,
        funding_intervals,
    )?;

    let (mark_rows, mark_pages, mark_complete) = walk_backward(
        request.performance_start_ms,
        request.performance_end_exclusive_ms,
        PAGE_LIMIT_KLINE,
        MAX_PAGES,
        |end_inclusive| {
            let start = request.performance_start_ms;
            let url = format!(
                "{}/v5/market/mark-price-kline?category=linear&symbol={SYMBOL}&interval=60&start={start}&end={end_inclusive}&limit={PAGE_LIMIT_KLINE}",
                request.environment.public_rest_base()
            );
            let fetch_future = fetch(url);
            async move {
                let body = fetch_future.await?;
                parse_mark_page(&body)
            }
        },
    )
    .await?;
    let mark_points = dedupe_values(mark_rows)?;
    let mark_gaps = hourly_gaps(
        request.performance_start_ms,
        request.performance_end_exclusive_ms,
        &mark_points,
    );
    let hourly_marks = seal_prices(
        request,
        SeriesSpec {
            format: "donchian_daily_v1_mark_60",
            series: "mark_60",
            endpoint: MARK_ENDPOINT,
            requested_start_ms: request.performance_start_ms,
            requested_end_exclusive_ms: request.performance_end_exclusive_ms.saturating_add(1),
            pages_fetched: mark_pages,
            pagination_complete: mark_complete,
            gaps: mark_gaps,
            notes: vec![
                "hourly mark candle open is the regular PnL mark".to_string(),
                "hourly_mark_open_at_settlement uses this open when a funding timestamp is on the hour".to_string(),
            ],
        },
        mark_points.clone(),
    )?;

    let finer_needed = funding
        .records
        .iter()
        .filter(|row| {
            row.settlement_time_ms >= request.performance_start_ms
                && row.settlement_time_ms < request.performance_end_exclusive_ms
        })
        .filter(|row| {
            !row.settlement_time_ms.is_multiple_of(HOUR_MS)
                || !mark_points.contains_key(&row.settlement_time_ms)
        })
        .map(|row| row.settlement_time_ms)
        .collect::<BTreeSet<_>>();
    let finer_marks = if finer_needed.is_empty() {
        None
    } else {
        Some(download_finer_marks(request, &finer_needed, &mut fetch).await?)
    };

    Ok(AuxBundle {
        trade_minutes,
        funding,
        hourly_marks,
        finer_marks,
    })
}

pub async fn download_trade_minutes_for(
    request: &AuxDownloadRequest,
    required: std::collections::BTreeSet<u64>,
    format_label: &str,
) -> Result<PriceSeriesFile, AuxError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| AuxError::new(format!("http client: {err}")))?;
    download_trade_minutes_with(request, required, format_label, |url| {
        let client = client.clone();
        async move { fetch_public_body(&client, &url).await }
    })
    .await
}

pub async fn download_trade_minutes_with<F, Fut>(
    request: &AuxDownloadRequest,
    required: std::collections::BTreeSet<u64>,
    format_label: &str,
    mut fetch: F,
) -> Result<PriceSeriesFile, AuxError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, AuxError>>,
{
    validate_request(request)?;
    let windows = merge_minute_windows(&required, 10 * MINUTE_MS);
    let mut trade_points = BTreeMap::new();
    let mut trade_pages = 0u32;
    for (start, end) in &windows {
        let url = format!(
            "{}/v5/market/kline?category=linear&symbol={SYMBOL}&interval=1&start={start}&end={end}&limit={PAGE_LIMIT_KLINE}",
            request.environment.public_rest_base()
        );
        let body = fetch(url).await?;
        trade_pages += 1;
        let page =
            parse_bybit_public_kline_page(&body).map_err(|err| AuxError::new(err.to_string()))?;
        check_identity(page.category.as_deref(), page.symbol.as_deref())?;
        for row in page.rows {
            insert_price(&mut trade_points, row.start_time_ms, row.open)?;
        }
    }
    let trade_gaps = missing_hours_as_gaps(&required, &trade_points);
    seal_prices(
        request,
        SeriesSpec {
            format: format_label,
            series: "trade_1m",
            endpoint: TRADE_ENDPOINT,
            requested_start_ms: request.performance_start_ms,
            requested_end_exclusive_ms: request.performance_end_exclusive_ms,
            pages_fetched: trade_pages,
            pagination_complete: true,
            gaps: trade_gaps,
            notes: vec![format!(
                "one-minute trade opens for {} execution minutes across {} merged windows; not a full three-year tape",
                required.len(),
                windows.len()
            )],
        },
        trade_points,
    )
}

pub fn save_bundle(dir: &Path, bundle: &AuxBundle) -> Result<(), AuxError> {
    std::fs::create_dir_all(dir)
        .map_err(|err| AuxError::new(format!("create {}: {err}", dir.display())))?;
    write_json(&dir.join("trade-1m.json"), &bundle.trade_minutes)?;
    write_json(&dir.join("funding.json"), &bundle.funding)?;
    write_json(&dir.join("mark-60.json"), &bundle.hourly_marks)?;
    if let Some(finer) = &bundle.finer_marks {
        write_json(&dir.join("mark-1.json"), finer)?;
    }
    Ok(())
}

pub fn load_price_series(path: &Path) -> Result<PriceSeriesFile, AuxError> {
    let file: PriceSeriesFile = read_json(path)?;
    verify_price_hash(&file)?;
    Ok(file)
}

pub fn load_funding_file(path: &Path) -> Result<FundingFile, AuxError> {
    let file: FundingFile = read_json(path)?;
    verify_funding_hash(&file)?;
    Ok(file)
}

pub fn market_inputs_from_files(
    trades: &PriceSeriesFile,
    funding: &FundingFile,
    marks: &PriceSeriesFile,
    finer: Option<&PriceSeriesFile>,
) -> Result<MarketInputs, AuxError> {
    verify_price_hash(trades)?;
    verify_funding_hash(funding)?;
    verify_price_hash(marks)?;
    if let Some(finer) = finer {
        verify_price_hash(finer)?;
    }
    for (label, meta) in [
        ("trade_1m", &trades.metadata),
        ("funding", &funding.metadata),
        ("mark_60", &marks.metadata),
    ] {
        check_cached_identity(label, meta)?;
    }
    if let Some(finer) = finer {
        check_cached_identity("mark_1", &finer.metadata)?;
    }
    let mut minutes = BTreeMap::new();
    for point in &trades.points {
        insert_price(&mut minutes, point.time_ms, point.open)?;
    }
    let mut hourly = BTreeMap::new();
    for point in &marks.points {
        insert_price(&mut hourly, point.time_ms, point.open)?;
    }
    let mut finer_marks = BTreeMap::new();
    if let Some(finer) = finer {
        for point in &finer.points {
            insert_price(&mut finer_marks, point.time_ms, point.open)?;
        }
    }
    let funding_records = funding
        .records
        .iter()
        .map(|row| FundingRecord {
            settlement_time_ms: row.settlement_time_ms,
            funding_rate: row.funding_rate,
        })
        .collect();
    let mut coverage = vec![
        coverage_note(&trades.metadata),
        coverage_note(&funding.metadata),
        coverage_note(&marks.metadata),
    ];
    if let Some(finer) = finer {
        coverage.push(coverage_note(&finer.metadata));
    }
    Ok(MarketInputs {
        minutes,
        funding: funding_records,
        hourly_marks: hourly,
        finer_marks,
        coverage,
    })
}

pub fn merge_minute_windows(minutes: &BTreeSet<u64>, max_gap_ms: u64) -> Vec<(u64, u64)> {
    let mut windows = Vec::new();
    let mut iter = minutes.iter().copied();
    let Some(first) = iter.next() else {
        return windows;
    };
    let mut start = first;
    let mut end = first;
    for minute in iter {
        if minute.saturating_sub(end) <= max_gap_ms {
            end = minute;
        } else {
            windows.push((start, end));
            start = minute;
            end = minute;
        }
    }
    windows.push((start, end));
    windows
}

async fn download_finer_marks<F, Fut>(
    request: &AuxDownloadRequest,
    needed: &BTreeSet<u64>,
    fetch: &mut F,
) -> Result<PriceSeriesFile, AuxError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, AuxError>>,
{
    let windows = merge_minute_windows(needed, 10 * MINUTE_MS);
    let mut points = BTreeMap::new();
    let mut pages = 0u32;
    for (start, end) in &windows {
        let window_start = start.saturating_sub(MINUTE_MS);
        let window_end = end.saturating_add(MINUTE_MS);
        let url = format!(
            "{}/v5/market/mark-price-kline?category=linear&symbol={SYMBOL}&interval=1&start={window_start}&end={window_end}&limit={PAGE_LIMIT_KLINE}",
            request.environment.public_rest_base()
        );
        let body = fetch(url).await?;
        pages += 1;
        for row in parse_mark_page(&body)? {
            insert_price(&mut points, row.time_ms, row.value)?;
        }
    }
    let gaps = missing_hours_as_gaps(needed, &points);
    seal_prices(
        request,
        SeriesSpec {
            format: "donchian_daily_v1_mark_1",
            series: "mark_1",
            endpoint: MARK_ENDPOINT,
            requested_start_ms: request.performance_start_ms,
            requested_end_exclusive_ms: request.performance_end_exclusive_ms,
            pages_fetched: pages,
            pagination_complete: true,
            gaps,
            notes: vec![
                "finer mark opens for funding timestamps that are not on an hourly mark"
                    .to_string(),
            ],
        },
        points,
    )
}

async fn walk_backward<T, F, Fut>(
    start_ms: u64,
    end_inclusive_ms: u64,
    page_limit: usize,
    max_pages: usize,
    mut fetch_page: F,
) -> Result<(Vec<Timed<T>>, u32, bool), AuxError>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = Result<Vec<Timed<T>>, AuxError>>,
{
    if page_limit == 0 {
        return Err(AuxError::new("page limit must be positive"));
    }
    let mut end_inclusive = end_inclusive_ms;
    let mut kept = Vec::new();
    for page_index in 0..max_pages {
        let page = fetch_page(end_inclusive).await?;
        if page.is_empty() {
            return Ok((kept, page_index as u32 + 1, true));
        }
        let oldest = page
            .iter()
            .map(|row| row.time_ms)
            .min()
            .ok_or_else(|| AuxError::new("page was empty"))?;
        let page_len = page.len();
        for row in page {
            if row.time_ms >= start_ms && row.time_ms <= end_inclusive_ms {
                kept.push(row);
            }
        }
        if oldest <= start_ms || page_len < page_limit {
            return Ok((kept, page_index as u32 + 1, true));
        }
        let next_end = oldest
            .checked_sub(1)
            .ok_or_else(|| AuxError::new("pagination reached the start of the timestamp range"))?;
        if next_end >= end_inclusive {
            return Err(AuxError::new(
                "pagination did not move backward; refusing to truncate the requested range",
            ));
        }
        end_inclusive = next_end;
    }
    Ok((kept, max_pages as u32, false))
}

fn parse_mark_page(body: &str) -> Result<Vec<Timed<Decimal>>, AuxError> {
    let page =
        parse_bybit_mark_price_kline_page(body).map_err(|err| AuxError::new(err.to_string()))?;
    check_identity(page.category.as_deref(), page.symbol.as_deref())?;
    Ok(page
        .rows
        .into_iter()
        .map(|row| Timed {
            time_ms: row.start_time_ms,
            value: row.open,
        })
        .collect())
}

fn check_identity(category: Option<&str>, symbol: Option<&str>) -> Result<(), AuxError> {
    if let Some(category) = category {
        if category != PRODUCT {
            return Err(AuxError::new(format!(
                "expected category {PRODUCT}, got {category}"
            )));
        }
    }
    if let Some(symbol) = symbol {
        if symbol != SYMBOL {
            return Err(AuxError::new(format!(
                "expected symbol {SYMBOL}, got {symbol}"
            )));
        }
    }
    Ok(())
}

fn check_cached_identity(label: &str, metadata: &SeriesMetadata) -> Result<(), AuxError> {
    if metadata.format_version != FORMAT_VERSION {
        return Err(AuxError::new(format!("{label} has an unsupported format")));
    }
    if metadata.venue != VENUE
        || metadata.environment != "mainnet"
        || metadata.product != PRODUCT
        || metadata.symbol != SYMBOL
    {
        return Err(AuxError::new(format!(
            "{label} is not mainnet Bybit linear BTCUSDT"
        )));
    }
    if metadata.series != label {
        return Err(AuxError::new(format!(
            "expected series {label}, got {}",
            metadata.series
        )));
    }
    Ok(())
}

fn insert_price(
    points: &mut BTreeMap<u64, Decimal>,
    time_ms: u64,
    open: Decimal,
) -> Result<(), AuxError> {
    if let Some(existing) = points.get(&time_ms) {
        if *existing != open {
            return Err(AuxError::new(format!(
                "conflicting open at {time_ms}: {existing} and {open}"
            )));
        }
        return Ok(());
    }
    points.insert(time_ms, open);
    Ok(())
}

fn dedupe_values(rows: Vec<Timed<Decimal>>) -> Result<BTreeMap<u64, Decimal>, AuxError> {
    let mut points = BTreeMap::new();
    for row in rows {
        insert_price(&mut points, row.time_ms, row.value)?;
    }
    Ok(points)
}

fn observed_intervals(times: impl Iterator<Item = u64>) -> Vec<u64> {
    let times: Vec<_> = times.collect();
    let mut diffs = BTreeSet::new();
    for pair in times.windows(2) {
        diffs.insert(pair[1].saturating_sub(pair[0]));
    }
    diffs.into_iter().collect()
}

fn hourly_gaps(start: u64, end_inclusive: u64, points: &BTreeMap<u64, Decimal>) -> Vec<AuxGap> {
    let mut gaps = Vec::new();
    let mut cursor = start;
    while cursor <= end_inclusive {
        if !points.contains_key(&cursor) {
            gaps.push(AuxGap {
                missing_start_ms: cursor,
                missing_end_exclusive_ms: cursor.saturating_add(HOUR_MS),
            });
        }
        if cursor == end_inclusive {
            break;
        }
        cursor = cursor.saturating_add(HOUR_MS);
    }
    gaps
}

fn missing_hours_as_gaps(required: &BTreeSet<u64>, points: &BTreeMap<u64, Decimal>) -> Vec<AuxGap> {
    required
        .iter()
        .filter(|time| !points.contains_key(time))
        .map(|time| AuxGap {
            missing_start_ms: *time,
            missing_end_exclusive_ms: time.saturating_add(MINUTE_MS),
        })
        .collect()
}

struct SeriesSpec<'a> {
    format: &'a str,
    series: &'a str,
    endpoint: &'a str,
    requested_start_ms: u64,
    requested_end_exclusive_ms: u64,
    pages_fetched: u32,
    pagination_complete: bool,
    gaps: Vec<AuxGap>,
    notes: Vec<String>,
}

fn seal_prices(
    request: &AuxDownloadRequest,
    spec: SeriesSpec<'_>,
    points: BTreeMap<u64, Decimal>,
) -> Result<PriceSeriesFile, AuxError> {
    let stored: Vec<_> = points
        .into_iter()
        .map(|(time_ms, open)| PricePoint { time_ms, open })
        .collect();
    let actual_start_ms = stored.first().map(|point| point.time_ms);
    let actual_end_ms = stored.last().map(|point| point.time_ms);
    let intervals = observed_intervals(stored.iter().map(|point| point.time_ms));
    let mut metadata = SeriesMetadata {
        format: spec.format.to_string(),
        format_version: FORMAT_VERSION,
        venue: VENUE.to_string(),
        environment: request.environment.as_str().to_string(),
        product: PRODUCT.to_string(),
        symbol: SYMBOL.to_string(),
        series: spec.series.to_string(),
        source_endpoint: spec.endpoint.to_string(),
        requested_start_ms: spec.requested_start_ms,
        requested_end_exclusive_ms: spec.requested_end_exclusive_ms,
        actual_start_ms,
        actual_end_ms,
        retrieved_at_ms: request.retrieved_at_ms,
        count: stored.len(),
        pages_fetched: spec.pages_fetched,
        pagination_complete: spec.pagination_complete,
        gaps: spec.gaps,
        observed_intervals_ms: intervals,
        notes: spec.notes,
        dataset_hash: String::new(),
    };
    metadata.dataset_hash = price_hash(&metadata, &stored)?;
    Ok(PriceSeriesFile {
        metadata,
        points: stored,
    })
}

fn seal_funding(
    request: &AuxDownloadRequest,
    requested_start_ms: u64,
    requested_end_exclusive_ms: u64,
    points: BTreeMap<u64, Decimal>,
    pages_fetched: u32,
    pagination_complete: bool,
    intervals: Vec<u64>,
) -> Result<FundingFile, AuxError> {
    let records: Vec<_> = points
        .into_iter()
        .map(|(settlement_time_ms, funding_rate)| FundingPoint {
            settlement_time_ms,
            funding_rate,
        })
        .collect();
    let mut metadata = SeriesMetadata {
        format: "donchian_daily_v1_funding".to_string(),
        format_version: FORMAT_VERSION,
        venue: VENUE.to_string(),
        environment: request.environment.as_str().to_string(),
        product: PRODUCT.to_string(),
        symbol: SYMBOL.to_string(),
        series: "funding".to_string(),
        source_endpoint: FUNDING_ENDPOINT.to_string(),
        requested_start_ms,
        requested_end_exclusive_ms,
        actual_start_ms: records.first().map(|row| row.settlement_time_ms),
        actual_end_ms: records.last().map(|row| row.settlement_time_ms),
        retrieved_at_ms: request.retrieved_at_ms,
        count: records.len(),
        pages_fetched,
        pagination_complete,
        gaps: Vec::new(),
        observed_intervals_ms: intervals,
        notes: vec![
            "settlement timestamps and signed rates are stored as returned; no fixed interval was assumed and no missing settlement was manufactured".to_string(),
            "the requested range includes one day before the performance start and one day after the performance end so coverage can be checked at the boundaries".to_string(),
        ],
        dataset_hash: String::new(),
    };
    metadata.dataset_hash = funding_hash(&metadata, &records)?;
    Ok(FundingFile { metadata, records })
}

fn coverage_note(metadata: &SeriesMetadata) -> CoverageNote {
    CoverageNote {
        series: metadata.series.clone(),
        dataset_hash: metadata.dataset_hash.clone(),
        source_endpoint: metadata.source_endpoint.clone(),
        environment: metadata.environment.clone(),
        count: metadata.count,
        actual_start_ms: metadata.actual_start_ms,
        actual_end_ms: metadata.actual_end_ms,
        pages_fetched: metadata.pages_fetched,
        gaps: metadata.gaps.len(),
        pagination_complete: metadata.pagination_complete,
        observed_intervals_ms: metadata.observed_intervals_ms.clone(),
    }
}

fn price_hash(metadata: &SeriesMetadata, points: &[PricePoint]) -> Result<String, AuxError> {
    hash_payload(&PriceHash {
        format_version: metadata.format_version,
        venue: &metadata.venue,
        environment: &metadata.environment,
        product: &metadata.product,
        symbol: &metadata.symbol,
        series: &metadata.series,
        points,
    })
}

fn funding_hash(metadata: &SeriesMetadata, records: &[FundingPoint]) -> Result<String, AuxError> {
    hash_payload(&FundingHash {
        format_version: metadata.format_version,
        venue: &metadata.venue,
        environment: &metadata.environment,
        product: &metadata.product,
        symbol: &metadata.symbol,
        series: &metadata.series,
        records,
    })
}

fn verify_price_hash(file: &PriceSeriesFile) -> Result<(), AuxError> {
    let actual = price_hash(&file.metadata, &file.points)?;
    if actual != file.metadata.dataset_hash {
        return Err(AuxError::new(format!(
            "{} hash does not match the stored points",
            file.metadata.series
        )));
    }
    Ok(())
}

fn verify_funding_hash(file: &FundingFile) -> Result<(), AuxError> {
    let actual = funding_hash(&file.metadata, &file.records)?;
    if actual != file.metadata.dataset_hash {
        return Err(AuxError::new(
            "funding hash does not match the stored records",
        ));
    }
    Ok(())
}

#[derive(Serialize)]
struct PriceHash<'a> {
    format_version: u32,
    venue: &'a str,
    environment: &'a str,
    product: &'a str,
    symbol: &'a str,
    series: &'a str,
    points: &'a [PricePoint],
}

#[derive(Serialize)]
struct FundingHash<'a> {
    format_version: u32,
    venue: &'a str,
    environment: &'a str,
    product: &'a str,
    symbol: &'a str,
    series: &'a str,
    records: &'a [FundingPoint],
}

fn hash_payload<T: Serialize>(payload: &T) -> Result<String, AuxError> {
    let bytes =
        serde_json::to_vec(payload).map_err(|err| AuxError::new(format!("hash payload: {err}")))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

fn validate_request(request: &AuxDownloadRequest) -> Result<(), AuxError> {
    if request.performance_start_ms >= request.performance_end_exclusive_ms {
        return Err(AuxError::new("performance window is empty"));
    }
    if request.position_qty <= Decimal::ZERO {
        return Err(AuxError::new("position quantity must be positive"));
    }
    Ok(())
}

async fn fetch_public_body(client: &reqwest::Client, url: &str) -> Result<String, AuxError> {
    for attempt in 0..6 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(200 * attempt)).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
        eprintln!("fetch {url}");
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|err| AuxError::new(format!("public market request failed: {err}")))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| AuxError::new(format!("public market body: {err}")))?;
        if status.as_u16() == 429
            || status.as_u16() == 503
            || body.contains("\"retCode\":10006")
            || body.contains("\"retCode\":10016")
        {
            continue;
        }
        if !status.is_success() {
            return Err(AuxError::new(format!(
                "public market http {status}: {}",
                body.chars().take(240).collect::<String>()
            )));
        }
        return Ok(body);
    }
    Err(AuxError::new(format!(
        "public market request was rate limited: {url}"
    )))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AuxError> {
    let body = serde_json::to_vec_pretty(value)
        .map_err(|err| AuxError::new(format!("encode {}: {err}", path.display())))?;
    std::fs::write(path, body)
        .map_err(|err| AuxError::new(format!("write {}: {err}", path.display())))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AuxError> {
    let body = std::fs::read(path)
        .map_err(|err| AuxError::new(format!("read {}: {err}", path.display())))?;
    serde_json::from_slice(&body)
        .map_err(|err| AuxError::new(format!("decode {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::super::backtest::{changes_for, Book};
    use super::*;

    #[test]
    fn minute_windows_merge_nearby_opens_and_keep_distant_days_apart() {
        let minutes = BTreeSet::from([0, 60_000, 300_000, 86_400_000]);
        let windows = merge_minute_windows(&minutes, 10 * MINUTE_MS);
        assert_eq!(windows, vec![(0, 300_000), (86_400_000, 86_400_000)]);
        let span: u64 = windows.iter().map(|(start, end)| end - start).sum();
        assert!(span < 86_400_000);
    }

    #[test]
    fn backward_walk_keeps_every_page_until_the_start() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (rows, pages, complete) =
                walk_backward(0, 1_000, 2, 10, |end_inclusive| async move {
                    let values: &[u64] = if end_inclusive >= 800 {
                        &[800, 600]
                    } else if end_inclusive >= 400 {
                        &[400, 200]
                    } else {
                        &[0]
                    };
                    Ok(values
                        .iter()
                        .copied()
                        .map(|time_ms| Timed {
                            time_ms,
                            value: Decimal::from(time_ms),
                        })
                        .collect())
                })
                .await
                .unwrap();
            assert!(complete);
            assert_eq!(pages, 3);
            let times: BTreeSet<_> = rows.iter().map(|row| row.time_ms).collect();
            assert_eq!(times, BTreeSet::from([0, 200, 400, 600, 800]));
        });
    }

    #[test]
    fn donchian_changes_are_not_required_to_build_the_benchmark_minute() {
        let _ = changes_for(Book::FixedLongBenchmark, &[], 0, Decimal::new(1, 3)).unwrap();
    }
}
