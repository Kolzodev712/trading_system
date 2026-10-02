//! Start gate for the committed BTC linear files.
//!
//! Mark and funding may pass. Hourly trade candles and a usable execution
//! tape are expected to fail until a new versioned dataset exists.

use std::collections::BTreeMap;
use std::path::Path;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::donchian_daily_v1::aux::{load_funding_file, load_price_series};
use crate::donchian_daily_v1::backtest::HOUR_MS;

use super::dataset::{load_hourly_dataset, PERFORMANCE_END_EXCLUSIVE_MS, PERFORMANCE_START_MS};
use super::ShockError;

pub const FROZEN_MARK_60_HASH: &str =
    "sha256:993ea03c9d15acf1b9c305e5686e4cd5c4e7b243b83d559a61d7cb96a3febccd";
pub const FROZEN_FUNDING_HASH: &str =
    "sha256:a461acf4b952fe2e59e2166610707ef21cbcee0c88b92eb474f816bfa24e361f";
pub const FROZEN_SPARSE_TRADE_1M_HASH: &str =
    "sha256:9f965e602437960fc54f5c6793193dd76fea2561eb253e6e0de32330fe7575ca";
pub const FUNDING_INTERVAL_MS: u64 = 28_800_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageReport {
    pub disposition: String,
    pub checks: Vec<CoverageCheck>,
}

pub struct CoveragePaths<'a> {
    pub mark_60: &'a Path,
    pub funding: &'a Path,
    pub trade_1m: &'a Path,
    pub hourly_trade: Option<&'a Path>,
}

pub fn assess_existing_sources(paths: CoveragePaths<'_>) -> Result<CoverageReport, ShockError> {
    let checks = vec![
        check_mark(paths.mark_60),
        check_funding(paths.funding),
        check_sparse_minutes(paths.trade_1m),
        check_hourly(paths.hourly_trade),
    ];
    let passed = checks.iter().all(|check| check.passed);
    Ok(CoverageReport {
        disposition: if passed { "pass" } else { "fail" }.to_string(),
        checks,
    })
}

pub fn write_coverage_report(path: &Path, report: &CoverageReport) -> Result<(), ShockError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| ShockError::new(format!("create {}: {err}", parent.display())))?;
        }
    }
    let body = serde_json::to_vec_pretty(report)
        .map_err(|err| ShockError::new(format!("encode coverage report: {err}")))?;
    std::fs::write(path, body)
        .map_err(|err| ShockError::new(format!("write {}: {err}", path.display())))
}

fn check_mark(path: &Path) -> CoverageCheck {
    match load_price_series(path) {
        Ok(file) if file.metadata.dataset_hash != FROZEN_MARK_60_HASH => CoverageCheck {
            name: "mark_60".to_string(),
            passed: false,
            detail: format!(
                "hash {} is not the frozen mark file {FROZEN_MARK_60_HASH}",
                file.metadata.dataset_hash
            ),
        },
        Ok(file) => {
            let marks: BTreeMap<u64, Decimal> = file
                .points
                .iter()
                .map(|point| (point.time_ms, point.open))
                .collect();
            match require_hourly_marks(&marks, PERFORMANCE_START_MS, PERFORMANCE_END_EXCLUSIVE_MS) {
                Ok(()) if file.metadata.gaps.is_empty() => CoverageCheck {
                    name: "mark_60".to_string(),
                    passed: true,
                    detail: format!("frozen hash, {} hourly marks, gaps 0", file.metadata.count),
                },
                Ok(()) => CoverageCheck {
                    name: "mark_60".to_string(),
                    passed: false,
                    detail: format!(
                        "hourly grid is complete but gaps={}",
                        file.metadata.gaps.len()
                    ),
                },
                Err(err) => CoverageCheck {
                    name: "mark_60".to_string(),
                    passed: false,
                    detail: err.to_string(),
                },
            }
        }
        Err(err) => CoverageCheck {
            name: "mark_60".to_string(),
            passed: false,
            detail: err.to_string(),
        },
    }
}

fn check_funding(path: &Path) -> CoverageCheck {
    match load_funding_file(path) {
        Ok(file) if file.metadata.dataset_hash != FROZEN_FUNDING_HASH => CoverageCheck {
            name: "funding".to_string(),
            passed: false,
            detail: format!(
                "hash {} is not the frozen funding file {FROZEN_FUNDING_HASH}",
                file.metadata.dataset_hash
            ),
        },
        Ok(file) if !file.metadata.gaps.is_empty() => CoverageCheck {
            name: "funding".to_string(),
            passed: false,
            detail: format!("funding file records {} gaps", file.metadata.gaps.len()),
        },
        Ok(file) => {
            let times: Vec<u64> = file
                .records
                .iter()
                .map(|row| row.settlement_time_ms)
                .collect();
            match require_funding_grid(&times, PERFORMANCE_START_MS, PERFORMANCE_END_EXCLUSIVE_MS) {
                Ok(()) => CoverageCheck {
                    name: "funding".to_string(),
                    passed: true,
                    detail: format!("frozen hash, {} settlements on an 8h grid", times.len()),
                },
                Err(err) => CoverageCheck {
                    name: "funding".to_string(),
                    passed: false,
                    detail: err.to_string(),
                },
            }
        }
        Err(err) => CoverageCheck {
            name: "funding".to_string(),
            passed: false,
            detail: err.to_string(),
        },
    }
}

fn check_sparse_minutes(path: &Path) -> CoverageCheck {
    match load_price_series(path) {
        Ok(file) if file.metadata.dataset_hash != FROZEN_SPARSE_TRADE_1M_HASH => CoverageCheck {
            name: "trade_1m_execution_tape".to_string(),
            passed: false,
            detail: format!(
                "hash {} is not the known sparse Candidate #2 file, and it is not an hourly execution set",
                file.metadata.dataset_hash
            ),
        },
        Ok(file) => CoverageCheck {
            name: "trade_1m_execution_tape".to_string(),
            passed: false,
            detail: format!(
                "known sparse file has {} opens and is not a minute observation after every hourly close; {}",
                file.metadata.count,
                file.metadata.notes.first().map(String::as_str).unwrap_or("no note")
            ),
        },
        Err(err) => CoverageCheck {
            name: "trade_1m_execution_tape".to_string(),
            passed: false,
            detail: err.to_string(),
        },
    }
}

fn check_hourly(path: Option<&Path>) -> CoverageCheck {
    let Some(path) = path else {
        return CoverageCheck {
            name: "hourly_trade_ohlcv".to_string(),
            passed: false,
            detail: "no hourly trade-price OHLCV file is part of the committed BTC linear set"
                .to_string(),
        };
    };
    if !path.exists() {
        return CoverageCheck {
            name: "hourly_trade_ohlcv".to_string(),
            passed: false,
            detail: format!("hourly trade file {} is absent", path.display()),
        };
    }
    match load_hourly_dataset(path) {
        Ok(dataset) => CoverageCheck {
            name: "hourly_trade_ohlcv".to_string(),
            passed: true,
            detail: format!(
                "{} candles, hash {}",
                dataset.metadata.candle_count, dataset.metadata.dataset_hash
            ),
        },
        Err(err) => CoverageCheck {
            name: "hourly_trade_ohlcv".to_string(),
            passed: false,
            detail: err.to_string(),
        },
    }
}

fn require_hourly_marks(
    marks: &BTreeMap<u64, Decimal>,
    start: u64,
    end: u64,
) -> Result<(), ShockError> {
    if !start.is_multiple_of(HOUR_MS) || !end.is_multiple_of(HOUR_MS) || start >= end {
        return Err(ShockError::new("mark window is not an hourly range"));
    }
    let mut cursor = start;
    loop {
        if !marks.contains_key(&cursor) {
            return Err(ShockError::new(format!("missing hourly mark at {cursor}")));
        }
        if cursor == end {
            return Ok(());
        }
        cursor = cursor
            .checked_add(HOUR_MS)
            .ok_or_else(|| ShockError::new("mark cursor overflowed"))?;
    }
}

pub fn require_funding_grid(times: &[u64], start: u64, end: u64) -> Result<(), ShockError> {
    if times.len() < 2 {
        return Err(ShockError::new(
            "funding grid has fewer than two settlements",
        ));
    }
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for pair in sorted.windows(2) {
        if pair[1] - pair[0] != FUNDING_INTERVAL_MS {
            return Err(ShockError::new(format!(
                "funding interval {} is not 8h at {}",
                pair[1] - pair[0],
                pair[0]
            )));
        }
    }
    if sorted[0] > start {
        return Err(ShockError::new(
            "funding grid starts after the performance window",
        ));
    }
    let phase = sorted[0] % FUNDING_INTERVAL_MS;
    let mut cursor = if start % FUNDING_INTERVAL_MS == phase {
        start
    } else {
        let remainder = start % FUNDING_INTERVAL_MS;
        let delta = (phase + FUNDING_INTERVAL_MS - remainder) % FUNDING_INTERVAL_MS;
        start + delta
    };
    while cursor < end {
        if sorted.binary_search(&cursor).is_err() {
            return Err(ShockError::new(format!(
                "missing funding settlement at {cursor}"
            )));
        }
        cursor = cursor
            .checked_add(FUNDING_INTERVAL_MS)
            .ok_or_else(|| ShockError::new("funding cursor overflowed"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funding_grid_rejects_a_hole_inside_the_window() {
        let start = 0u64;
        let step = FUNDING_INTERVAL_MS;
        let times = vec![0, step, step * 3];
        let err = require_funding_grid(&times, start, step * 4).unwrap_err();
        assert!(err.to_string().contains("not 8h") || err.to_string().contains("missing"));
    }

    #[test]
    fn hourly_grid_constant_matches_the_simulator() {
        assert_eq!(HOUR_MS, 3_600_000);
    }
}
