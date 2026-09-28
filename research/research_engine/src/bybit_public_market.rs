//! Parsers for Bybit public mark-price candles and funding history.
//!
//! Mark-price rows have five fields and no volume, so they do not go through
//! the trade-kline parser. Funding history is a different endpoint.

use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitMarkCandle {
    pub start_time_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitMarkPage {
    pub symbol: Option<String>,
    pub category: Option<String>,
    /// API order: newest start time first.
    pub rows: Vec<BybitMarkCandle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitFundingRecord {
    pub symbol: Option<String>,
    pub funding_rate: Decimal,
    pub funding_rate_timestamp_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitFundingPage {
    pub category: Option<String>,
    /// API order: newest settlement first.
    pub rows: Vec<BybitFundingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitPublicMarketError {
    message: String,
}

impl BybitPublicMarketError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BybitPublicMarketError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BybitPublicMarketError {}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
}

pub fn parse_bybit_mark_price_kline_page(
    body: &str,
) -> Result<BybitMarkPage, BybitPublicMarketError> {
    let result = success_result(body, "mark-price kline")?;
    let mut rows = Vec::new();
    if let Some(list) = result.get("list").and_then(|value| value.as_array()) {
        for (index, row) in list.iter().enumerate() {
            let fields = row.as_array().ok_or_else(|| {
                BybitPublicMarketError::new(format!("mark row {index} is not an array"))
            })?;
            if fields.len() < 5 {
                return Err(BybitPublicMarketError::new(format!(
                    "mark row {index} has {} fields",
                    fields.len()
                )));
            }
            rows.push(BybitMarkCandle {
                start_time_ms: json_u64(index, "start", &fields[0])?,
                open: json_decimal(index, "open", &fields[1])?,
                high: json_decimal(index, "high", &fields[2])?,
                low: json_decimal(index, "low", &fields[3])?,
                close: json_decimal(index, "close", &fields[4])?,
            });
        }
    }
    Ok(BybitMarkPage {
        symbol: result
            .get("symbol")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        category: result
            .get("category")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        rows,
    })
}

pub fn parse_bybit_funding_history(body: &str) -> Result<BybitFundingPage, BybitPublicMarketError> {
    let result = success_result(body, "funding history")?;
    let mut rows = Vec::new();
    if let Some(list) = result.get("list").and_then(|value| value.as_array()) {
        for (index, row) in list.iter().enumerate() {
            let symbol = row
                .get("symbol")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let rate = row.get("fundingRate").ok_or_else(|| {
                BybitPublicMarketError::new(format!("funding row {index} is missing fundingRate"))
            })?;
            let timestamp = row.get("fundingRateTimestamp").ok_or_else(|| {
                BybitPublicMarketError::new(format!(
                    "funding row {index} is missing fundingRateTimestamp"
                ))
            })?;
            rows.push(BybitFundingRecord {
                symbol,
                funding_rate: json_decimal(index, "fundingRate", rate)?,
                funding_rate_timestamp_ms: json_u64(index, "fundingRateTimestamp", timestamp)?,
            });
        }
    }
    Ok(BybitFundingPage {
        category: result
            .get("category")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        rows,
    })
}

fn success_result(body: &str, label: &str) -> Result<serde_json::Value, BybitPublicMarketError> {
    let parsed: Envelope = serde_json::from_str(body)
        .map_err(|err| BybitPublicMarketError::new(format!("invalid {label} response: {err}")))?;
    if parsed.ret_code != 0 {
        return Err(BybitPublicMarketError::new(format!(
            "bybit {label} error: {} {}",
            parsed.ret_code, parsed.ret_msg
        )));
    }
    parsed
        .result
        .ok_or_else(|| BybitPublicMarketError::new(format!("{label} response missing result")))
}

fn json_u64(
    index: usize,
    field: &str,
    value: &serde_json::Value,
) -> Result<u64, BybitPublicMarketError> {
    if let Some(text) = value.as_str() {
        return text.parse().map_err(|_| {
            BybitPublicMarketError::new(format!("row {index} has an invalid {field}: {text}"))
        });
    }
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(number) = value.as_i64() {
        return u64::try_from(number).map_err(|_| {
            BybitPublicMarketError::new(format!("row {index} has an invalid {field}"))
        });
    }
    Err(BybitPublicMarketError::new(format!(
        "row {index} has an invalid {field}"
    )))
}

fn json_decimal(
    index: usize,
    field: &str,
    value: &serde_json::Value,
) -> Result<Decimal, BybitPublicMarketError> {
    let text = value.as_str().ok_or_else(|| {
        BybitPublicMarketError::new(format!(
            "row {index} {field} must be a decimal string, not {value}"
        ))
    })?;
    text.parse().map_err(|_| {
        BybitPublicMarketError::new(format!("row {index} has an invalid {field}: {text}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_page_keeps_five_field_rows_newest_first() {
        let body = r#"{"retCode":0,"retMsg":"OK","result":{"symbol":"BTCUSDT","category":"linear","list":[["1674345600000","22810","22820","22800","22805"],["1674342000000","22790","22810","22780","22800"]]}}"#;
        let page = parse_bybit_mark_price_kline_page(body).unwrap();
        assert_eq!(page.symbol.as_deref(), Some("BTCUSDT"));
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].start_time_ms, 1_674_345_600_000);
        assert_eq!(page.rows[0].open, Decimal::from(22810));
        assert!(page.rows[0].start_time_ms > page.rows[1].start_time_ms);
    }

    #[test]
    fn funding_page_preserves_signed_rate_and_settlement_time() {
        let body = r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[{"symbol":"BTCUSDT","fundingRate":"-0.00000966","fundingRateTimestamp":"1674288000000"},{"symbol":"BTCUSDT","fundingRate":"0.0001","fundingRateTimestamp":"1674259200000"}]}}"#;
        let page = parse_bybit_funding_history(body).unwrap();
        assert_eq!(page.rows[0].funding_rate_timestamp_ms, 1_674_288_000_000);
        assert_eq!(page.rows[0].funding_rate, Decimal::new(-966, 8));
        assert_eq!(page.rows[1].funding_rate, Decimal::new(1, 4));
        assert!(page.rows[0].funding_rate.is_sign_negative());
    }

    #[test]
    fn api_error_and_short_mark_row_fail() {
        let err = parse_bybit_mark_price_kline_page(
            r#"{"retCode":10001,"retMsg":"params error","result":{}}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("10001"));
        let err = parse_bybit_mark_price_kline_page(
            r#"{"retCode":0,"retMsg":"OK","result":{"list":[["1","2"]]}}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("mark row 0"));
    }
}
