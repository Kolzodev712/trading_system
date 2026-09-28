//! Parser for Bybit's public `/v5/market/kline` response.
//!
//! Rows stay in the order Bybit returns, which is newest candle first.
//! Callers that need chronological order reverse the rows themselves.

use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitPublicKline {
    pub start_time_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitPublicKlinePage {
    pub symbol: Option<String>,
    pub category: Option<String>,
    /// API order: newest start time first.
    pub rows: Vec<BybitPublicKline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BybitPublicKlineError {
    message: String,
}

impl BybitPublicKlineError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BybitPublicKlineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BybitPublicKlineError {}

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    #[serde(default)]
    result: Option<ResultBody>,
}

#[derive(Deserialize)]
struct ResultBody {
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    list: Vec<Vec<String>>,
}

pub fn parse_bybit_public_kline_page(
    body: &str,
) -> Result<BybitPublicKlinePage, BybitPublicKlineError> {
    let parsed: Response = serde_json::from_str(body)
        .map_err(|err| BybitPublicKlineError::new(format!("invalid kline response: {err}")))?;
    if parsed.ret_code != 0 {
        return Err(BybitPublicKlineError::new(format!(
            "bybit kline error: {} {}",
            parsed.ret_code, parsed.ret_msg
        )));
    }
    let result = parsed
        .result
        .ok_or_else(|| BybitPublicKlineError::new("kline response missing result"))?;
    let mut rows = Vec::with_capacity(result.list.len());
    for (index, row) in result.list.iter().enumerate() {
        rows.push(parse_row(index, row)?);
    }
    Ok(BybitPublicKlinePage {
        symbol: result.symbol,
        category: result.category,
        rows,
    })
}

fn parse_row(index: usize, row: &[String]) -> Result<BybitPublicKline, BybitPublicKlineError> {
    if row.len() < 6 {
        return Err(BybitPublicKlineError::new(format!(
            "kline row {index} has {} fields",
            row.len()
        )));
    }
    Ok(BybitPublicKline {
        start_time_ms: row[0].parse::<u64>().map_err(|_| {
            BybitPublicKlineError::new(format!("kline row {index} has an invalid start time"))
        })?,
        open: parse_decimal(index, "open", &row[1])?,
        high: parse_decimal(index, "high", &row[2])?,
        low: parse_decimal(index, "low", &row[3])?,
        close: parse_decimal(index, "close", &row[4])?,
        volume: parse_decimal(index, "volume", &row[5])?,
    })
}

fn parse_decimal(index: usize, field: &str, value: &str) -> Result<Decimal, BybitPublicKlineError> {
    value.parse().map_err(|_| {
        BybitPublicKlineError::new(format!("kline row {index} has an invalid {field}: {value}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_page_keeps_bybit_newest_first_order() {
        let body = r#"{"retCode":0,"retMsg":"OK","result":{"symbol":"BTCUSDT","category":"linear","list":[["1704326400000","42871.9","44875.5","42637","44156.3","164677.973"],["1704067200000","42324.8","44284.9","42211.2","44235.5","79123.439"]]}}"#;
        let page = parse_bybit_public_kline_page(body).unwrap();
        assert_eq!(page.symbol.as_deref(), Some("BTCUSDT"));
        assert_eq!(page.category.as_deref(), Some("linear"));
        assert_eq!(page.rows[0].start_time_ms, 1_704_326_400_000);
        assert_eq!(page.rows[1].start_time_ms, 1_704_067_200_000);
        assert!(page.rows[0].start_time_ms > page.rows[1].start_time_ms);
    }

    #[test]
    fn api_error_and_short_row_fail() {
        let err = parse_bybit_public_kline_page(
            r#"{"retCode":10001,"retMsg":"params error","result":{}}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("10001"));
        let err = parse_bybit_public_kline_page(
            r#"{"retCode":0,"retMsg":"OK","result":{"list":[["1","2"]]}}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("kline row 0"));
    }
}
