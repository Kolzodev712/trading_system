use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::Sha256;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::grpc::GrpcCtl;
use crate::oracle::{qty_canon, qty_close, rest_btc_position, runner_btc_qty, working_ids};
use crate::process::{
    allocate_port, cargo_build, debug_bin, require_mongo, wait_health_cex_ready, workspace_root,
    ChildProc,
};
use crate::types::{Capability, ScenarioResult};
use crate::venue::{RestBalance, RestOrder, RestPosition, RunnerSnapshot, VenueHarness};

const DEMO_REST: &str = "https://api-demo.bybit.com";

/// Read-only Bybit Demo adapter. Never places orders, never calls Resume.
pub struct DemoAdapter {
    pub rest_base: String,
    pub capabilities: Capability,
    api_key: String,
    api_secret: String,
    bot_path: PathBuf,
}

impl DemoAdapter {
    pub fn new() -> Result<Self> {
        if std::env::var("WORKFLOW_ALLOW_BYBIT_MAINNET")
            .ok()
            .as_deref()
            == Some("1")
        {
            bail!("refusing WORKFLOW_ALLOW_BYBIT_MAINNET: this harness must not target mainnet");
        }
        if std::env::var("WORKFLOW_ALLOW_BYBIT_TESTNET")
            .ok()
            .as_deref()
            == Some("1")
        {
            bail!("refusing WORKFLOW_ALLOW_BYBIT_TESTNET: Demo only, no silent testnet fallback");
        }
        let bot_path = demo_bot_path()?;
        let bot: Value = serde_json::from_str(
            &std::fs::read_to_string(&bot_path)
                .with_context(|| format!("read {}", bot_path.display()))?,
        )?;
        let eng = bot.get("exchange_engine").cloned().unwrap_or(Value::Null);
        if eng.get("exchange").and_then(|x| x.as_str()).unwrap_or("") != "Bybit" {
            bail!("demo bot must use exchange=Bybit");
        }
        let cfg = eng.get("config").cloned().unwrap_or(Value::Null);
        if cfg.get("demo").and_then(|x| x.as_bool()) != Some(true) {
            bail!("demo bot must set demo=true");
        }
        if cfg.get("testnet").and_then(|x| x.as_bool()) == Some(true) {
            bail!("demo bot must not set testnet");
        }
        let cred = cfg.get("credentials").cloned().unwrap_or(Value::Null);
        let api_key = cred
            .get("apiKey")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let api_secret = cred
            .get("apiSecret")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if api_key.is_empty() || api_key.contains("REPLACE") {
            bail!(
                "authenticated Demo snapshot requires real credentials in {} (placeholders refused)",
                bot_path.display()
            );
        }
        Ok(Self {
            rest_base: DEMO_REST.into(),
            capabilities: Capability::demo_readonly(),
            api_key,
            api_secret,
            bot_path,
        })
    }

    pub async fn readonly_checks(&self, artifacts: &Path) -> ScenarioResult {
        let mut r = ScenarioResult::new("demo_readonly_paused", "BYBIT", "Futures", "paused");
        r.note("read-only Demo adapter; no mutating RPCs, no Resume");
        r.note(format!("credentials from {}", self.bot_path.display()));
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("client");
        let time_url = format!("{}/v5/market/time", self.rest_base);
        match client.get(&time_url).send().await {
            Ok(resp) => {
                r.assert_eq("demo_host", "api-demo.bybit.com", {
                    if time_url.contains("api-demo.bybit.com")
                        && !time_url.contains("testnet")
                        && !time_url.contains("api.bybit.com/v5")
                    {
                        "api-demo.bybit.com"
                    } else {
                        "wrong-host"
                    }
                });
                r.assert_true(
                    "market_time_http",
                    resp.status().is_success(),
                    "2xx",
                    &resp.status().to_string(),
                );
            }
            Err(e) => r.fail("market_time", "reachable Demo public API", &e.to_string()),
        }

        let wallet = client
            .get(format!("{}/v5/account/wallet-balance", self.rest_base))
            .query(&[("accountType", "UNIFIED")])
            .send()
            .await;
        match wallet {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let denied =
                    !status.is_success() || body.contains("retCode") || body.contains("API key");
                r.assert_eq(
                    "unauth_wallet_denial",
                    "denied",
                    if denied {
                        "denied"
                    } else {
                        "unexpected_success"
                    },
                );
            }
            Err(e) => r.fail("unauth_wallet", "denied without keys", &e.to_string()),
        }

        let rest_pos = match self.signed_positions().await {
            Ok(p) => p,
            Err(e) => {
                r.fail("auth_rest_positions", "signed Demo REST", &e.to_string());
                return r;
            }
        };
        let rest_orders = match self.signed_orders().await {
            Ok(o) => o,
            Err(e) => {
                r.fail("auth_rest_orders", "signed Demo REST", &e.to_string());
                return r;
            }
        };
        let rest_bal = match self.signed_wallet().await {
            Ok(b) => b,
            Err(e) => {
                r.fail("auth_rest_wallet", "signed Demo REST", &e.to_string());
                return r;
            }
        };
        r.note(format!(
            "independent Demo REST orders={} pos={} wallet_rows={}",
            rest_orders.len(),
            rest_pos.len(),
            rest_bal.len()
        ));
        if let Err(e) =
            write_demo_truth(artifacts, "rest", &rest_orders, &rest_pos, &rest_bal, None)
        {
            r.fail("truth_rest", "ok", &e.to_string());
            return r;
        }

        let paused = match DemoPausedRunner::launch(artifacts, &self.bot_path).await {
            Ok(p) => p,
            Err(e) => {
                r.fail(
                    "paused_runner",
                    "launch paused Demo composition",
                    &e.to_string(),
                );
                return r;
            }
        };
        let snap = match paused.snapshot().await {
            Ok(s) => s,
            Err(e) => {
                r.fail("paused_grpc", "status while paused", &e.to_string());
                return r;
            }
        };
        r.assert_eq("venue", "BYBIT", snap.venue.to_uppercase());
        r.assert_true(
            "environment_demo",
            snap.environment.to_uppercase().contains("DEMO"),
            "DEMO",
            &snap.environment,
        );
        r.assert_eq("entries_paused", "true", snap.entries_paused.to_string());
        r.assert_eq(
            "execution_mode",
            "exchange",
            snap.execution_mode.to_lowercase(),
        );

        let rest_qty = rest_btc_position(&rest_pos);
        let runner_qty = runner_btc_qty(&snap);
        r.assert_eq(
            "paused_runner_vs_rest_qty",
            qty_canon(rest_qty),
            qty_canon(runner_qty),
        );
        r.assert_eq(
            "paused_runner_vs_rest_orders",
            working_ids(&rest_orders).len().to_string(),
            snap.orders.len().to_string(),
        );
        if !qty_close(rest_qty, runner_qty) {
            r.fail(
                "paused_snapshot_agreement",
                &qty_canon(rest_qty),
                &qty_canon(runner_qty),
            );
        }
        if let Err(e) = write_demo_truth(
            artifacts,
            "paused",
            &rest_orders,
            &rest_pos,
            &rest_bal,
            Some(&snap),
        ) {
            r.fail("truth_paused", "ok", &e.to_string());
            return r;
        }
        r.assert_eq(
            "no_mutating_flag",
            "false",
            self.capabilities.mutating.to_string(),
        );
        r
    }

    pub(crate) async fn signed_get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let qs = serde_urlencoded(query);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();
        let recv = "5000";
        let payload = format!("{}{}{}{}", ts, self.api_key, recv, qs);
        let sign = hmac_hex(&self.api_secret, &payload);
        let url = format!("{}{}?{}", self.rest_base, path, qs);
        let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
        let resp = client
            .get(&url)
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-TIMESTAMP", ts)
            .header("X-BAPI-RECV-WINDOW", recv)
            .header("X-BAPI-SIGN", sign)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.context("demo REST json")?;
        if !status.is_success() || body.get("retCode").and_then(|x| x.as_i64()) != Some(0) {
            bail!(
                "Demo REST {path} status={status} ret={:?}",
                body.get("retMsg")
            );
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    pub(crate) async fn signed_positions(&self) -> Result<Vec<RestPosition>> {
        let v = self
            .signed_get(
                "/v5/position/list",
                &[("category", "linear"), ("symbol", "BTCUSDT")],
            )
            .await?;
        Ok(v.get("list")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let qty = dec_field(p, "size");
                if qty.is_zero() {
                    return None;
                }
                let side = p.get("side").and_then(|x| x.as_str()).unwrap_or("");
                let signed = if side.eq_ignore_ascii_case("Sell") {
                    -qty
                } else {
                    qty
                };
                Some(RestPosition {
                    instrument: p
                        .get("symbol")
                        .and_then(|x| x.as_str())
                        .unwrap_or("BTCUSDT")
                        .to_string(),
                    quantity: signed,
                })
            })
            .collect())
    }

    pub(crate) async fn signed_orders(&self) -> Result<Vec<RestOrder>> {
        let v = self
            .signed_get(
                "/v5/order/realtime",
                &[("category", "linear"), ("symbol", "BTCUSDT")],
            )
            .await?;
        Ok(parse_rest_orders(&v))
    }

    /// Exact `orderLinkId` via open orders, then order history. An immediate
    /// fill may never appear on the default realtime list.
    pub(crate) async fn signed_order_by_link_id(
        &self,
        client_order_id: &str,
    ) -> Result<Option<RestOrder>> {
        if client_order_id.is_empty() {
            return Ok(None);
        }
        let open = self
            .signed_get(
                "/v5/order/realtime",
                &[
                    ("category", "linear"),
                    ("symbol", "BTCUSDT"),
                    ("orderLinkId", client_order_id),
                ],
            )
            .await?;
        if let Some(order) = parse_rest_orders(&open)
            .into_iter()
            .find(|o| o.client_order_id == client_order_id)
        {
            return Ok(Some(order));
        }
        let history = self
            .signed_get(
                "/v5/order/history",
                &[
                    ("category", "linear"),
                    ("symbol", "BTCUSDT"),
                    ("orderLinkId", client_order_id),
                ],
            )
            .await?;
        Ok(parse_rest_orders(&history)
            .into_iter()
            .find(|o| o.client_order_id == client_order_id))
    }

    pub(crate) async fn signed_wallet(&self) -> Result<Vec<RestBalance>> {
        let v = self
            .signed_get("/v5/account/wallet-balance", &[("accountType", "UNIFIED")])
            .await?;
        let mut out = Vec::new();
        for acct in v
            .get("list")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            for c in acct
                .get("coin")
                .and_then(|x| x.as_array())
                .into_iter()
                .flatten()
            {
                out.push(RestBalance {
                    asset: c
                        .get("coin")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    total: dec_field(c, "walletBalance"),
                });
            }
        }
        Ok(out)
    }

    pub(crate) async fn public_get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let qs = serde_urlencoded(query);
        let url = format!("{}{}?{}", self.rest_base, path, qs);
        let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
        let resp = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.context("demo public REST json")?;
        if !status.is_success() || body.get("retCode").and_then(|x| x.as_i64()) != Some(0) {
            bail!(
                "Demo public REST {path} status={status} ret={:?}",
                body.get("retMsg")
            );
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }
}

fn parse_rest_orders(result: &Value) -> Vec<RestOrder> {
    result
        .get("list")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
        .map(|o| RestOrder {
            order_id: o
                .get("orderId")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            client_order_id: o
                .get("orderLinkId")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            instrument: o
                .get("symbol")
                .and_then(|x| x.as_str())
                .unwrap_or("BTCUSDT")
                .to_string(),
            price: dec_field(o, "price"),
            amount: dec_field(o, "qty"),
            filled: dec_field(o, "cumExecQty"),
            status: o
                .get("orderStatus")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            side: o
                .get("side")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect()
}

/// Live credentialed bot.json lives outside preserved artifacts (0600). Drop shreds it.
pub(crate) struct SecretBotFile {
    live_dir: PathBuf,
    live_bot: PathBuf,
    archive_bot: PathBuf,
    archived: bool,
}

impl SecretBotFile {
    pub(crate) fn create(archive_bot: PathBuf, bot: &Value) -> Result<Self> {
        let live_dir =
            std::env::temp_dir().join(format!("ts-demo-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&live_dir).with_context(|| format!("create {}", live_dir.display()))?;
        restrict_dir(&live_dir, 0o700);
        let live_bot = live_dir.join("bot.json");
        fs::write(&live_bot, serde_json::to_vec_pretty(bot)?)
            .with_context(|| format!("write {}", live_bot.display()))?;
        restrict_dir(&live_bot, 0o600);
        Ok(Self {
            live_dir,
            live_bot,
            archive_bot,
            archived: false,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.live_bot
    }

    pub(crate) fn archive_redacted(&mut self) -> Result<()> {
        let raw = fs::read_to_string(&self.live_bot)
            .with_context(|| format!("read {}", self.live_bot.display()))?;
        let mut v: Value = serde_json::from_str(&raw)?;
        redact_credentials(&mut v);
        if let Some(parent) = self.archive_bot.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.archive_bot, serde_json::to_vec_pretty(&v)?)?;
        self.archived = true;
        Ok(())
    }
}

impl Drop for SecretBotFile {
    fn drop(&mut self) {
        if !self.archived {
            let _ = self.archive_redacted();
        }
        shred_and_remove(&self.live_bot);
        let _ = fs::remove_dir_all(&self.live_dir);
    }
}

fn restrict_dir(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    }
    let _ = mode;
}

fn shred_and_remove(path: &Path) {
    if path.exists() {
        let zeros = vec![0u8; 4096];
        let _ = fs::write(path, zeros);
        let _ = fs::remove_file(path);
    }
}

fn redact_credentials(v: &mut Value) {
    if let Some(cred) = v
        .pointer_mut("/exchange_engine/config/credentials")
        .and_then(|c| c.as_object_mut())
    {
        if cred.contains_key("apiKey") {
            cred.insert("apiKey".into(), "REDACTED".into());
        }
        if cred.contains_key("apiSecret") {
            cred.insert("apiSecret".into(), "REDACTED".into());
        }
    }
}

struct DemoPausedRunner {
    grpc_url: String,
    _secret: SecretBotFile,
    _child: ChildProc,
}

impl DemoPausedRunner {
    async fn launch(artifacts: &Path, src_bot: &Path) -> Result<Self> {
        require_mongo()?;
        let ws = workspace_root();
        cargo_build(
            &ws,
            &[
                "build",
                "-q",
                "-p",
                "btc_price_ma_app",
                "--bin",
                "btc_price_ma_app",
            ],
        )?;
        let bin = debug_bin(&ws, "btc_price_ma_app");
        if !bin.exists() {
            bail!("missing {}", bin.display());
        }
        let run_id = format!("demo-{}", uuid::Uuid::new_v4().simple());
        let run_dir = artifacts.join(&run_id).join("run");
        fs::create_dir_all(&run_dir)?;
        let health_port = allocate_port()?;
        let grpc_port = allocate_port()?;
        let grpc_bind = format!("127.0.0.1:{grpc_port}");
        let grpc_url = format!("http://127.0.0.1:{grpc_port}");
        let mut bot: Value = serde_json::from_str(&fs::read_to_string(src_bot)?)?;
        if let Some(obj) = bot.as_object_mut() {
            obj.insert(
                "submission_journal_path".into(),
                run_dir
                    .join("submission-journal.jsonl")
                    .display()
                    .to_string()
                    .into(),
            );
            obj.insert(
                "execution_ledger_path".into(),
                run_dir
                    .join("execution-ledger.jsonl")
                    .display()
                    .to_string()
                    .into(),
            );
            obj.insert(
                "runtime_snapshot_path".into(),
                run_dir
                    .join("runtime-snapshot.json")
                    .display()
                    .to_string()
                    .into(),
            );
            obj.insert(
                "restart_flag_path".into(),
                run_dir
                    .join("restart.requested")
                    .display()
                    .to_string()
                    .into(),
            );
            if let Some(server) = obj.get_mut("server").and_then(|s| s.as_object_mut()) {
                server.insert("bind_address".into(), "127.0.0.1".into());
                server.insert("port".into(), health_port.into());
                server.insert("grpc_bind".into(), grpc_bind.into());
            } else {
                obj.insert(
                    "server".into(),
                    serde_json::json!({
                        "bind_address": "127.0.0.1",
                        "port": health_port,
                        "grpc_bind": grpc_bind
                    }),
                );
            }
            if let Some(mongo) = obj.get_mut("mongo").and_then(|m| m.as_object_mut()) {
                mongo.insert("bot_id".into(), run_id.clone().into());
                mongo.insert("database".into(), format!("trading_system_{run_id}").into());
            }
        }
        let mut secret = SecretBotFile::create(run_dir.join("bot.json"), &bot)?;
        let strategies = ws.join("sample_config/bybit-demo/strategies.json");
        if !strategies.exists() {
            bail!("missing {}", strategies.display());
        }
        let mut cmd = Command::new(&bin);
        cmd.arg("--mode")
            .arg("strategies")
            .arg("--config")
            .arg(secret.path())
            .arg("--strategies-config")
            .arg(&strategies)
            .env("TRACE_JAEGER_DISABLED", "1")
            .env("TRADING_LOG_JSON", "1")
            .env("TRADING_RUN_ID", &run_id)
            .env("TRADING_GRPC", &grpc_url)
            .env_remove("TRADING_TEST_CANDLES_PATH")
            .env_remove("TRADING_TEST_NOW_MS")
            .env_remove("INC10_FRESH_ENTRY_FIXTURE")
            .current_dir(&ws);
        let child = ChildProc::spawn("btc_price_ma_app", cmd, &run_dir.join("runner.log"))?;
        wait_health_cex_ready(
            &format!("http://127.0.0.1:{health_port}/health"),
            Duration::from_secs(90),
        )
        .await?;
        secret.archive_redacted()?;
        Ok(Self {
            grpc_url,
            _secret: secret,
            _child: child,
        })
    }

    async fn snapshot(&self) -> Result<RunnerSnapshot> {
        GrpcCtl::connect(&self.grpc_url).await?.snapshot().await
    }
}

#[async_trait::async_trait]
impl VenueHarness for DemoAdapter {
    fn venue_name(&self) -> &'static str {
        "BYBIT"
    }

    fn capabilities(&self) -> Capability {
        self.capabilities
    }

    async fn snapshot(&self) -> Result<RunnerSnapshot> {
        bail!("DemoAdapter snapshot requires the paused runner launched by readonly_checks")
    }

    async fn rest_orders(&self) -> Result<Vec<RestOrder>> {
        self.signed_orders().await
    }

    async fn rest_balances(&self) -> Result<Vec<RestBalance>> {
        self.signed_wallet().await
    }

    async fn rest_positions(&self) -> Result<Vec<RestPosition>> {
        self.signed_positions().await
    }
}

impl Default for DemoAdapter {
    fn default() -> Self {
        Self::new().expect("demo adapter")
    }
}

fn demo_bot_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("WORKFLOW_DEMO_BOT") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
        bail!("WORKFLOW_DEMO_BOT does not exist: {}", path.display());
    }
    let ws = workspace_root();
    let runtime = ws.join("runtime-data/bybit-demo/bot.json");
    if runtime.exists() {
        return Ok(runtime);
    }
    let sample = ws.join("sample_config/bybit-demo/bot.json");
    if sample.exists() {
        return Ok(sample);
    }
    bail!("no Demo bot.json (set WORKFLOW_DEMO_BOT or copy sample_config/bybit-demo/bot.json)");
}

fn hmac_hex(secret: &str, payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn serde_urlencoded(query: &[(&str, &str)]) -> String {
    query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn dec_field(v: &Value, key: &str) -> Decimal {
    v.get(key)
        .and_then(|x| x.as_str())
        .and_then(|s| Decimal::from_str_exact(s).ok())
        .unwrap_or(Decimal::ZERO)
}

pub(crate) fn write_demo_truth(
    artifacts: &Path,
    tag: &str,
    orders: &[RestOrder],
    positions: &[RestPosition],
    balances: &[RestBalance],
    snap: Option<&RunnerSnapshot>,
) -> Result<()> {
    let dest = artifacts.join("demo-snapshots").join(tag);
    fs::create_dir_all(&dest)?;
    fs::write(
        dest.join("rest-open-orders.json"),
        serde_json::to_vec_pretty(
            &orders
                .iter()
                .map(|o| {
                    serde_json::json!({
                        "id": o.order_id,
                        "client_id": o.client_order_id,
                        "side": o.side,
                        "qty": o.amount.to_string(),
                        "status": o.status
                    })
                })
                .collect::<Vec<_>>(),
        )?,
    )?;
    fs::write(
        dest.join("rest-positions.json"),
        serde_json::to_vec_pretty(
            &positions
                .iter()
                .map(|p| serde_json::json!({"instrument": p.instrument, "qty": p.quantity.to_string()}))
                .collect::<Vec<_>>(),
        )?,
    )?;
    fs::write(
        dest.join("rest-balances.json"),
        serde_json::to_vec_pretty(
            &balances
                .iter()
                .map(|b| serde_json::json!({"asset": b.asset, "total": b.total.to_string()}))
                .collect::<Vec<_>>(),
        )?,
    )?;
    if let Some(s) = snap {
        fs::write(dest.join("grpc-status.json"), serde_json::to_vec_pretty(s)?)?;
    }
    Ok(())
}
