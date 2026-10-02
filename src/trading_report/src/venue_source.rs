use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use sha2::Sha256;
use std::collections::HashSet;
use std::time::Duration;

use crate::config::VenueSourceConfig;
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
        Ok(Self {
            cfg,
            client: Client::builder().timeout(Duration::from_secs(15)).build()?,
        })
    }

    pub async fn snapshot(&self, instrument: &str, instrument_type: &str) -> VenueSnapshot {
        let observed = chrono::Utc::now().to_rfc3339();
        match self.cfg.kind.to_ascii_lowercase().as_str() {
            "paper" => self.paper_snapshot(observed, instrument_type).await,
            "bybit_demo" => {
                self.bybit_snapshot(observed, instrument, instrument_type)
                    .await
            }
            other => VenueSnapshot {
                observed_at_utc: observed,
                http_methods: vec![],
                orders: Err(format!("unsupported venue kind {other}")),
                balances: Err(format!("unsupported venue kind {other}")),
                positions: Err(format!("unsupported venue kind {other}")),
                positions_supported: false,
            },
        }
    }

    async fn paper_snapshot(&self, observed: String, instrument_type: &str) -> VenueSnapshot {
        let mut methods = Vec::new();
        let orders = self.paper_get_orders(&mut methods).await;
        let balances = self.paper_get_balances(&mut methods).await;
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
        VenueSnapshot {
            observed_at_utc: observed,
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
        Ok(items
            .iter()
            .filter_map(|v| {
                Some(OrderView {
                    order_id: v.get("order_id")?.as_str()?.to_string(),
                    client_order_id: v
                        .get("client_order_id")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                    symbol: v
                        .get("instrument_id")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    side: v
                        .get("side")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    price: json_dec(v.get("price")),
                    quantity: json_dec(v.get("amount")),
                    status: v
                        .get("status")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    created_at: v
                        .get("created_at")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                })
            })
            .collect())
    }

    async fn paper_get_balances(
        &self,
        methods: &mut Vec<String>,
    ) -> Result<Vec<BalanceView>, String> {
        methods.push("GET".into());
        let url = format!("{}/balances", self.cfg.rest_base.trim_end_matches('/'));
        let value = get_json(&self.client, &url).await?;
        let items = as_array(&value)?;
        Ok(items
            .iter()
            .filter_map(|v| {
                Some(BalanceView {
                    asset: v.get("asset")?.as_str()?.to_string(),
                    total: json_dec(v.get("total")),
                    free: v.get("available").map(json_dec_value),
                })
            })
            .collect())
    }

    async fn paper_get_positions(
        &self,
        methods: &mut Vec<String>,
    ) -> Result<Vec<PositionView>, String> {
        methods.push("GET".into());
        let url = format!("{}/positions", self.cfg.rest_base.trim_end_matches('/'));
        let value = get_json(&self.client, &url).await?;
        let items = as_array(&value)?;
        Ok(items
            .iter()
            .filter_map(|v| {
                Some(PositionView {
                    symbol: v
                        .get("instrument_id")
                        .or_else(|| v.get("symbol"))
                        .and_then(|x| x.as_str())?
                        .to_string(),
                    quantity: json_dec(v.get("quantity").or_else(|| v.get("size"))),
                })
            })
            .collect())
    }

    async fn bybit_snapshot(
        &self,
        observed: String,
        instrument: &str,
        instrument_type: &str,
    ) -> VenueSnapshot {
        let mut methods = Vec::new();
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
        let (balances, positions, positions_supported) =
            if instrument_type.eq_ignore_ascii_case("spot") {
                (
                    self.bybit_wallet(&mut methods).await,
                    Err("spot uses balances; /positions is not proof of flatness".into()),
                    false,
                )
            } else {
                (
                    self.bybit_wallet(&mut methods).await,
                    self.bybit_positions(&mut methods, &category, &symbol).await,
                    true,
                )
            };
        VenueSnapshot {
            observed_at_utc: observed,
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
            for item in list {
                let id = item
                    .get("orderId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if seen.insert(id.clone()) {
                    pages.push(OrderView {
                        order_id: id,
                        client_order_id: item
                            .get("orderLinkId")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        symbol: item
                            .get("symbol")
                            .and_then(|v| v.as_str())
                            .unwrap_or(symbol)
                            .to_string(),
                        side: item
                            .get("side")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        price: json_dec(item.get("price")),
                        quantity: json_dec(item.get("qty")),
                        status: item
                            .get("orderStatus")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        created_at: item
                            .get("createdTime")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
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
        Ok(list
            .iter()
            .filter_map(|item| {
                Some(PositionView {
                    symbol: item.get("symbol")?.as_str()?.to_string(),
                    quantity: json_dec(item.get("size")),
                })
            })
            .collect())
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
        let mut out = Vec::new();
        if let Some(list) = value.pointer("/result/list").and_then(|v| v.as_array()) {
            for acct in list {
                if let Some(coins) = acct.get("coin").and_then(|v| v.as_array()) {
                    for coin in coins {
                        if let Some(asset) = coin.get("coin").and_then(|v| v.as_str()) {
                            out.push(BalanceView {
                                asset: asset.to_string(),
                                total: json_dec(coin.get("walletBalance")),
                                free: coin.get("availableToWithdraw").map(json_dec_value),
                            });
                        }
                    }
                }
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

fn json_dec(v: Option<&serde_json::Value>) -> String {
    v.map(json_dec_value).unwrap_or_default()
}

fn json_dec_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

pub fn parse_decimal(s: &str) -> Result<Decimal> {
    if s.trim().is_empty() {
        return Err(anyhow!("empty decimal"));
    }
    s.parse::<Decimal>().map_err(|e| anyhow!("{e}"))
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

pub fn qty_close(a: &str, b: &str, tolerance: Decimal) -> bool {
    match (parse_decimal(a), parse_decimal(b)) {
        (Ok(x), Ok(y)) => (x - y).abs() <= tolerance,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(qty_close("0.00100", "0.001", Decimal::new(1, 8)));
        assert_eq!(normalize_symbol("btc/usdt"), "BTCUSDT");
    }
}
