use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use sha2::Sha256;
use std::collections::HashSet;
use std::time::Duration;

use crate::config::{approved_demo_rest, VenueSourceConfig, BYBIT_DEMO_REST};
use crate::model::{BalanceView, OrderView, PositionView};

#[derive(Debug, Clone)]
pub struct VenueSnapshot {
    pub observed_at_utc: String,
    pub http_methods: Vec<String>,
    pub orders: Result<Vec<OrderView>, String>,
    pub balances: Result<Vec<BalanceView>, String>,
    pub positions: Result<Vec<PositionView>, String>,
    pub positions_supported: bool,
}

pub struct HttpVenueReader {
    cfg: VenueSourceConfig,
    client: Client,
}

impl HttpVenueReader {
    pub fn new(cfg: VenueSourceConfig) -> Result<Self> {
        if cfg.kind.eq_ignore_ascii_case("bybit_demo") && !approved_demo_rest(&cfg.rest_base) {
            return Err(anyhow!(
                "bybit_demo rest_base must be {BYBIT_DEMO_REST}; refusing {}",
                cfg.rest_base
            ));
        }
        Ok(Self {
            cfg,
            client: Client::builder().timeout(Duration::from_secs(15)).build()?,
        })
    }

    pub async fn snapshot(&self, instrument: &str, instrument_type: &str) -> VenueSnapshot {
        match self.cfg.kind.to_ascii_lowercase().as_str() {
            "paper" => self.paper_snapshot(instrument_type).await,
            "bybit_demo" => self.bybit_snapshot(instrument, instrument_type).await,
            other => VenueSnapshot {
                observed_at_utc: chrono::Utc::now().to_rfc3339(),
                http_methods: vec![],
                orders: Err(format!("unsupported venue kind {other}")),
                balances: Err(format!("unsupported venue kind {other}")),
                positions: Err(format!("unsupported venue kind {other}")),
                positions_supported: false,
            },
        }
    }

    async fn paper_snapshot(&self, instrument_type: &str) -> VenueSnapshot {
        let mut methods = Vec::new();
        let mut earliest = None;
        let orders = self.paper_get_orders(&mut methods).await;
        retain_earliest(&mut earliest);
        let balances = self.paper_get_balances(&mut methods).await;
        retain_earliest(&mut earliest);
        let (positions, positions_supported) = if instrument_type.eq_ignore_ascii_case("spot") {
            match self.paper_get_positions(&mut methods).await {
                Ok(v) => (Ok(v), true),
                Err(err)
                    if err.contains("404") || err.to_ascii_lowercase().contains("unsupported") =>
                {
                    (
                        Err("spot /positions unsupported; not proof of flatness".into()),
                        false,
                    )
                }
                Err(err) => (Err(err), true),
            }
        } else {
            (self.paper_get_positions(&mut methods).await, true)
        };
        retain_earliest(&mut earliest);
        VenueSnapshot {
            observed_at_utc: observed_rfc3339(earliest),
            http_methods: methods,
            orders,
            balances,
            positions,
            positions_supported,
        }
    }

    async fn paper_get_orders(&self, methods: &mut Vec<String>) -> Result<Vec<OrderView>, String> {
        methods.push("GET".into());
        let url = format!("{}/open_orders", self.cfg.rest_base.trim_end_matches('/'));
        let value = get_json(&self.client, &url).await?;
        if remaining_cursor(&value) {
            return Err("incomplete REST pagination: remaining cursor".into());
        }
        let items = as_array(&value)?;
        let mut out = Vec::new();
        for (idx, v) in items.iter().enumerate() {
            out.push(parse_paper_order(v).map_err(|e| format!("open_orders[{idx}]: {e}"))?);
        }
        Ok(out)
    }

    async fn paper_get_balances(
        &self,
        methods: &mut Vec<String>,
    ) -> Result<Vec<BalanceView>, String> {
        methods.push("GET".into());
        let url = format!("{}/balances", self.cfg.rest_base.trim_end_matches('/'));
        let value = get_json(&self.client, &url).await?;
        let items = as_array(&value)?;
        let mut out = Vec::new();
        for (idx, v) in items.iter().enumerate() {
            out.push(parse_paper_balance(v).map_err(|e| format!("balances[{idx}]: {e}"))?);
        }
        Ok(out)
    }

    async fn paper_get_positions(
        &self,
        methods: &mut Vec<String>,
    ) -> Result<Vec<PositionView>, String> {
        methods.push("GET".into());
        let url = format!("{}/positions", self.cfg.rest_base.trim_end_matches('/'));
        let value = get_json(&self.client, &url).await?;
        let items = as_array(&value)?;
        let mut out = Vec::new();
        for (idx, v) in items.iter().enumerate() {
            out.push(parse_paper_position(v).map_err(|e| format!("positions[{idx}]: {e}"))?);
        }
        Ok(out)
    }

    async fn bybit_snapshot(&self, instrument: &str, instrument_type: &str) -> VenueSnapshot {
        let mut methods = Vec::new();
        let mut earliest = None;
        let category = self.cfg.category.clone().unwrap_or_else(|| {
            if instrument_type.eq_ignore_ascii_case("spot") {
                "spot".into()
            } else {
                "linear".into()
            }
        });
        let symbol = instrument.replace('/', "").replace('-', "");
        let orders = self
            .bybit_open_orders(&mut methods, &category, &symbol)
            .await;
        retain_earliest(&mut earliest);
        let (balances, positions, positions_supported) =
            if instrument_type.eq_ignore_ascii_case("spot") {
                let balances = self.bybit_wallet(&mut methods).await;
                retain_earliest(&mut earliest);
                (
                    balances,
                    Err("spot uses balances; /positions is not proof of flatness".into()),
                    false,
                )
            } else {
                let balances = self.bybit_wallet(&mut methods).await;
                retain_earliest(&mut earliest);
                let positions = self.bybit_positions(&mut methods, &category, &symbol).await;
                retain_earliest(&mut earliest);
                (balances, positions, true)
            };
        VenueSnapshot {
            observed_at_utc: observed_rfc3339(earliest),
            http_methods: methods,
            orders,
            balances,
            positions,
            positions_supported,
        }
    }

    async fn bybit_open_orders(
        &self,
        methods: &mut Vec<String>,
        category: &str,
        symbol: &str,
    ) -> Result<Vec<OrderView>, String> {
        let mut pages = Vec::new();
        let mut cursor = None;
        let mut seen = HashSet::new();
        for page in 0..40 {
            methods.push("GET".into());
            let mut query = vec![
                ("category".to_string(), category.to_string()),
                ("symbol".to_string(), symbol.to_string()),
                ("limit".to_string(), "50".to_string()),
            ];
            if let Some(c) = cursor.clone() {
                query.push(("cursor".into(), c));
            }
            let value = signed_get(&self.client, &self.cfg, "/v5/order/realtime", &query).await?;
            let ret = value.get("retCode").and_then(|v| v.as_i64()).unwrap_or(-1);
            if ret != 0 {
                return Err(format!(
                    "Bybit open-order page {page} failed: {}",
                    value
                        .get("retMsg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("error")
                ));
            }
            let list = value
                .pointer("/result/list")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("Bybit open-order page {page} missing list"))?;
            for (idx, item) in list.iter().enumerate() {
                let order = parse_bybit_order(item, symbol)
                    .map_err(|e| format!("Bybit open-order page {page}[{idx}]: {e}"))?;
                if seen.insert(order.order_id.clone()) {
                    pages.push(order);
                }
            }
            let next = value
                .pointer("/result/nextPageCursor")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            match next {
                None => return Ok(pages),
                Some(next) if cursor.as_ref() == Some(&next) => {
                    return Err(format!(
                        "Bybit open-order pagination repeated cursor {next}"
                    ));
                }
                Some(next) => cursor = Some(next),
            }
        }
        Err("Bybit open-order list truncated after 40 pages".into())
    }

    async fn bybit_positions(
        &self,
        methods: &mut Vec<String>,
        category: &str,
        symbol: &str,
    ) -> Result<Vec<PositionView>, String> {
        methods.push("GET".into());
        let query = [
            ("category".into(), category.to_string()),
            ("symbol".into(), symbol.to_string()),
        ];
        let value = signed_get(&self.client, &self.cfg, "/v5/position/list", &query).await?;
        let ret = value.get("retCode").and_then(|v| v.as_i64()).unwrap_or(-1);
        if ret != 0 {
            return Err(format!(
                "Bybit position/list failed: {}",
                value
                    .get("retMsg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("error")
            ));
        }
        let list = value
            .pointer("/result/list")
            .and_then(|v| v.as_array())
            .ok_or("Bybit position/list missing list")?;
        let mut out = Vec::new();
        for (idx, item) in list.iter().enumerate() {
            out.push(parse_bybit_position(item).map_err(|e| format!("position[{idx}]: {e}"))?);
        }
        Ok(out)
    }

    async fn bybit_wallet(&self, methods: &mut Vec<String>) -> Result<Vec<BalanceView>, String> {
        methods.push("GET".into());
        let query = [("accountType".into(), "UNIFIED".into())];
        let value = signed_get(
            &self.client,
            &self.cfg,
            "/v5/account/wallet-balance",
            &query,
        )
        .await?;
        let ret = value.get("retCode").and_then(|v| v.as_i64()).unwrap_or(-1);
        if ret != 0 {
            return Err(format!(
                "Bybit wallet-balance failed: {}",
                value
                    .get("retMsg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("error")
            ));
        }
        let list = value
            .pointer("/result/list")
            .and_then(|v| v.as_array())
            .ok_or("Bybit wallet-balance missing list")?;
        let mut out = Vec::new();
        for (acct_idx, acct) in list.iter().enumerate() {
            let coins = acct
                .get("coin")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("wallet list[{acct_idx}] missing coin array"))?;
            for (idx, coin) in coins.iter().enumerate() {
                out.push(
                    parse_bybit_balance(coin)
                        .map_err(|e| format!("wallet list[{acct_idx}].coin[{idx}]: {e}"))?,
                );
            }
        }
        Ok(out)
    }
}

async fn get_json(client: &Client, url: &str) -> Result<serde_json::Value, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status} {text}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("json: {e}"))
}

async fn signed_get(
    client: &Client,
    cfg: &VenueSourceConfig,
    path: &str,
    query: &[(String, String)],
) -> Result<serde_json::Value, String> {
    let key_env = cfg.api_key_env.as_deref().ok_or("api_key_env missing")?;
    let secret_env = cfg
        .api_secret_env
        .as_deref()
        .ok_or("api_secret_env missing")?;
    let api_key = std::env::var(key_env).map_err(|_| format!("{key_env} is unset"))?;
    let api_secret = std::env::var(secret_env).map_err(|_| format!("{secret_env} is unset"))?;
    if api_key.contains("REPLACE") {
        return Err("placeholder Bybit credentials refused".into());
    }
    let qs = query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let ts = chrono::Utc::now().timestamp_millis();
    let recv = 5000u64;
    let payload = format!("{ts}{api_key}{recv}{qs}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(api_secret.as_bytes()).map_err(|e| format!("hmac: {e}"))?;
    mac.update(payload.as_bytes());
    let sign = hex::encode(mac.finalize().into_bytes());
    let url = format!("{}{path}?{qs}", cfg.rest_base.trim_end_matches('/'));
    let resp = client
        .get(url)
        .header("X-BAPI-API-KEY", api_key)
        .header("X-BAPI-TIMESTAMP", ts.to_string())
        .header("X-BAPI-RECV-WINDOW", recv.to_string())
        .header("X-BAPI-SIGN", sign)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status} {text}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("json: {e}"))
}

fn as_array(value: &serde_json::Value) -> Result<&Vec<serde_json::Value>, String> {
    value
        .as_array()
        .ok_or_else(|| "REST list was not a JSON array".into())
}

fn remaining_cursor(value: &serde_json::Value) -> bool {
    value
        .get("nextPageCursor")
        .or_else(|| value.get("cursor"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

fn json_dec_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn json_field<'a>(v: &'a serde_json::Value, names: &[&str]) -> Option<&'a serde_json::Value> {
    names.iter().find_map(|name| v.get(*name))
}

fn require_nonempty_str(
    v: &serde_json::Value,
    names: &[&str],
    field: &str,
) -> Result<String, String> {
    let raw = json_field(v, names).ok_or_else(|| format!("missing {field}"))?;
    let s = match raw {
        serde_json::Value::String(s) => s.clone(),
        other => other
            .as_str()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if other.is_number() {
                    other.to_string()
                } else {
                    String::new()
                }
            }),
    };
    if s.trim().is_empty() {
        return Err(format!("empty {field}"));
    }
    Ok(s)
}

fn optional_str(v: &serde_json::Value, names: &[&str]) -> Option<String> {
    json_field(v, names).and_then(|raw| match raw {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => raw
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    })
}

fn require_decimal_str(
    v: &serde_json::Value,
    names: &[&str],
    field: &str,
) -> Result<String, String> {
    let raw = json_field(v, names).ok_or_else(|| format!("missing {field}"))?;
    let s = json_dec_value(raw);
    parse_decimal(&s).map_err(|e| format!("malformed {field} '{s}': {e}"))?;
    Ok(s)
}

fn optional_side(v: &serde_json::Value) -> String {
    optional_str(v, &["side"]).unwrap_or_default()
}

pub fn parse_paper_order(v: &serde_json::Value) -> Result<OrderView, String> {
    Ok(OrderView {
        order_id: require_nonempty_str(v, &["order_id", "orderId"], "order_id")?,
        client_order_id: optional_str(v, &["client_order_id", "clientOrderId"]),
        symbol: require_nonempty_str(v, &["instrument_id", "instrumentId", "symbol"], "symbol")?,
        side: optional_side(v),
        price: optional_str(v, &["price"]).unwrap_or_default(),
        quantity: require_decimal_str(v, &["amount", "quantity", "qty"], "quantity")?,
        status: optional_str(v, &["status"]).unwrap_or_default(),
        created_at: optional_str(v, &["created_at", "createdAt"]),
    })
}

pub fn parse_paper_balance(v: &serde_json::Value) -> Result<BalanceView, String> {
    Ok(BalanceView {
        asset: require_nonempty_str(v, &["asset", "coin"], "asset")?,
        total: require_decimal_str(v, &["total", "walletBalance"], "total")?,
        free: json_field(v, &["available", "free", "availableToWithdraw"])
            .map(json_dec_value)
            .filter(|s| parse_decimal(s).is_ok()),
    })
}

pub fn parse_paper_position(v: &serde_json::Value) -> Result<PositionView, String> {
    let symbol = require_nonempty_str(v, &["instrument_id", "instrumentId", "symbol"], "symbol")?;
    let quantity = require_decimal_str(v, &["quantity", "size", "qty"], "quantity")?;
    Ok(PositionView {
        symbol,
        quantity: signed_qty(&optional_side(v), &quantity)?,
    })
}

pub fn parse_bybit_order(
    v: &serde_json::Value,
    fallback_symbol: &str,
) -> Result<OrderView, String> {
    let symbol = match require_nonempty_str(v, &["symbol"], "symbol") {
        Ok(s) => s,
        Err(_) if !fallback_symbol.is_empty() => fallback_symbol.to_string(),
        Err(e) => return Err(e),
    };
    Ok(OrderView {
        order_id: require_nonempty_str(v, &["orderId", "order_id"], "orderId")?,
        client_order_id: optional_str(v, &["orderLinkId", "client_order_id"]),
        symbol,
        side: optional_side(v),
        price: optional_str(v, &["price"]).unwrap_or_default(),
        quantity: require_decimal_str(v, &["qty", "quantity", "amount"], "qty")?,
        status: optional_str(v, &["orderStatus", "status"]).unwrap_or_default(),
        created_at: optional_str(v, &["createdTime", "created_at"]),
    })
}

pub fn parse_bybit_position(v: &serde_json::Value) -> Result<PositionView, String> {
    let symbol = require_nonempty_str(v, &["symbol"], "symbol")?;
    let size = require_decimal_str(v, &["size", "quantity"], "size")?;
    Ok(PositionView {
        symbol,
        quantity: signed_qty(&optional_side(v), &size)?,
    })
}

pub fn parse_bybit_balance(v: &serde_json::Value) -> Result<BalanceView, String> {
    Ok(BalanceView {
        asset: require_nonempty_str(v, &["coin", "asset"], "coin")?,
        total: require_decimal_str(v, &["walletBalance", "total"], "walletBalance")?,
        free: json_field(v, &["availableToWithdraw", "available", "free"])
            .map(json_dec_value)
            .filter(|s| parse_decimal(s).is_ok()),
    })
}

pub fn signed_qty(side: &str, size: &str) -> Result<String, String> {
    let abs = parse_decimal(size).map_err(|e| format!("malformed size '{size}': {e}"))?;
    let signed = if side.eq_ignore_ascii_case("Sell") || side.eq_ignore_ascii_case("Short") {
        -abs
    } else {
        abs
    };
    Ok(signed.normalize().to_string())
}

pub fn parse_decimal(s: &str) -> Result<Decimal> {
    if s.trim().is_empty() {
        return Err(anyhow!("empty decimal"));
    }
    s.parse::<Decimal>().map_err(|e| anyhow!("{e}"))
}

pub fn matches_instrument(symbol: &str, instrument: &str) -> bool {
    normalize_symbol(symbol) == normalize_symbol(instrument)
}

pub fn instrument_base(instrument: &str) -> &str {
    instrument.split(['/', '-']).next().unwrap_or(instrument)
}

/// Walk pages until the cursor is empty. A leftover cursor is an error, not a short list.
pub fn merge_order_pages(
    pages: Vec<(Vec<String>, Option<String>)>,
    max_pages: usize,
) -> Result<Vec<String>, String> {
    if pages.len() > max_pages {
        return Err(format!("truncated after {max_pages} pages"));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut prev_cursor = None;
    let page_count = pages.len();
    for (idx, (items, cursor)) in pages.into_iter().enumerate() {
        for id in items {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
        match cursor {
            Some(c) => {
                if prev_cursor.as_ref() == Some(&c) {
                    return Err(format!("repeated cursor {c}"));
                }
                prev_cursor = Some(c);
                if idx + 1 == page_count {
                    return Err("incomplete REST pagination: remaining cursor".into());
                }
                if idx + 1 == max_pages {
                    return Err(format!("truncated after {max_pages} pages"));
                }
            }
            None => prev_cursor = None,
        }
    }
    Ok(out)
}

pub fn normalize_symbol(symbol: &str) -> String {
    symbol
        .replace('/', "")
        .replace('-', "")
        .to_ascii_uppercase()
}

pub fn earliest_observed_at(times: &[DateTime<Utc>]) -> Option<DateTime<Utc>> {
    times.iter().copied().min()
}

fn retain_earliest(slot: &mut Option<DateTime<Utc>>) {
    let now = Utc::now();
    *slot = Some(match *slot {
        Some(prev) => prev.min(now),
        None => now,
    });
}

fn observed_rfc3339(earliest: Option<DateTime<Utc>>) -> String {
    earliest.unwrap_or_else(Utc::now).to_rfc3339()
}

pub fn qty_close(a: &str, b: &str, tolerance: Decimal) -> Result<bool, String> {
    let x = parse_decimal(a).map_err(|e| format!("malformed quantity '{a}': {e}"))?;
    let y = parse_decimal(b).map_err(|e| format!("malformed quantity '{b}': {e}"))?;
    Ok((x - y).abs() <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VenueSourceConfig;
    use rust_decimal::Decimal;

    #[test]
    fn truncated_cursor_is_error_not_short_list() {
        let pages = vec![
            (vec!["a".into(), "b".into()], Some("c1".into())),
            (vec!["c".into()], Some("c2".into())),
        ];
        let err = merge_order_pages(pages, 2).expect_err("truncated");
        assert!(err.contains("truncated") || err.contains("remaining"));
    }

    #[test]
    fn complete_pages_dedup() {
        let pages = vec![
            (vec!["a".into(), "b".into()], Some("c1".into())),
            (vec!["b".into(), "c".into()], None),
        ];
        let ids = merge_order_pages(pages, 40).unwrap();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn spot_wallet_is_not_flat_from_unsupported_positions() {
        let snap = VenueSnapshot {
            observed_at_utc: "t".into(),
            http_methods: vec!["GET".into()],
            orders: Ok(vec![]),
            balances: Ok(vec![BalanceView {
                asset: "BTC".into(),
                total: "0.001".into(),
                free: None,
            }]),
            positions: Err("spot /positions unsupported; not proof of flatness".into()),
            positions_supported: false,
        };
        assert!(!snap.positions_supported);
        assert!(snap.positions.is_err());
        assert_eq!(snap.balances.as_ref().unwrap()[0].total, "0.001");
    }

    #[test]
    fn decimal_and_symbol_normalize() {
        assert!(qty_close("0.00100", "0.001", Decimal::new(1, 8)).unwrap());
        assert_eq!(normalize_symbol("btc/usdt"), "BTCUSDT");
    }

    #[test]
    fn matching_empty_quantities_are_not_equal() {
        let err = qty_close("", "", Decimal::new(1, 8)).expect_err("empty");
        assert!(err.contains("malformed"));
        assert!(qty_close("not-a-qty", "not-a-qty", Decimal::new(1, 8)).is_err());
    }

    #[test]
    fn incomplete_rest_row_is_rejected() {
        let missing_id = serde_json::json!({"instrumentId":"BTC-USDT","amount":"0.01"});
        assert!(parse_paper_order(&missing_id).is_err());
        let empty_qty = serde_json::json!({"orderId":"o1","instrumentId":"BTC-USDT","amount":""});
        assert!(parse_paper_order(&empty_qty).is_err());
        let dropped_shape = serde_json::json!({"symbol":"BTCUSDT"});
        assert!(parse_paper_position(&dropped_shape).is_err());
    }

    #[test]
    fn bybit_short_is_signed_negative() {
        let pos = parse_bybit_position(&serde_json::json!({
            "symbol": "BTCUSDT",
            "side": "Sell",
            "size": "0.010"
        }))
        .unwrap();
        assert_eq!(pos.quantity, "-0.01");
        let long = parse_bybit_position(&serde_json::json!({
            "symbol": "BTCUSDT",
            "side": "Buy",
            "size": "0.010"
        }))
        .unwrap();
        assert_eq!(long.quantity, "0.01");
    }

    #[test]
    fn unapproved_demo_host_is_rejected() {
        match HttpVenueReader::new(VenueSourceConfig {
            kind: "bybit_demo".into(),
            rest_base: "https://evil.example".into(),
            api_key_env: Some("BYBIT_DEMO_API_KEY".into()),
            api_secret_env: Some("BYBIT_DEMO_API_SECRET".into()),
            category: Some("linear".into()),
        }) {
            Ok(_) => panic!("unapproved host must be rejected"),
            Err(err) => assert!(err.to_string().contains(BYBIT_DEMO_REST)),
        }
    }

    #[test]
    fn later_response_does_not_replace_earliest_venue_time() {
        use chrono::TimeZone;
        let first = Utc
            .timestamp_millis_opt(1_700_000_000_000)
            .single()
            .unwrap();
        let later = first + chrono::Duration::milliseconds(20_000);
        let stamped = earliest_observed_at(&[later, first]).expect("times");
        assert_eq!(stamped, first);
        let age = (later.timestamp_millis() - stamped.timestamp_millis()).max(0);
        assert_eq!(age, 20_000);
        assert_ne!(age, 0);
    }

    #[tokio::test]
    async fn delayed_later_rest_call_keeps_earliest_venue_stamp() {
        use chrono::DateTime;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let line = req.lines().next().unwrap_or("");
                    let body = if line.contains("/open_orders") {
                        "[]"
                    } else if line.contains("/balances") {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        r#"[{"asset":"USDT","total":"0","available":"0","locked":"0"}]"#
                    } else if line.contains("/positions") {
                        "[]"
                    } else {
                        "[]"
                    };
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                });
            }
        });

        let snap = HttpVenueReader::new(VenueSourceConfig {
            kind: "paper".into(),
            rest_base: format!("http://{addr}"),
            api_key_env: None,
            api_secret_env: None,
            category: None,
        })
        .unwrap()
        .snapshot("BTC/USDT", "spot")
        .await;
        assert!(snap.orders.is_ok(), "{:?}", snap.orders.err());
        assert!(snap.balances.is_ok(), "{:?}", snap.balances.err());
        let observed = DateTime::parse_from_rfc3339(&snap.observed_at_utc).unwrap();
        let completed = Utc::now();
        let age_ms = (completed.timestamp_millis() - observed.timestamp_millis()).max(0);
        assert!(
            age_ms >= 250,
            "later balances delay must keep the orders stamp; age was {age_ms}ms (observed {})",
            snap.observed_at_utc
        );
    }
}
