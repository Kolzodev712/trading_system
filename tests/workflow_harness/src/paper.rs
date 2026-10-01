use anyhow::{bail, Context, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use uuid::Uuid;

use crate::artifacts::file_sha256;
use crate::grpc::{parse_dec, GrpcCtl};
use crate::process::{
    allocate_port, cargo_build, debug_bin, k_paper_dir, require_mongo, wait_health_cex_ready,
    wait_http, workspace_root, ChildProc,
};
use crate::types::Capability;
use crate::venue::{RestBalance, RestOrder, RestPosition, RunnerSnapshot};

pub struct PaperStack {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub http: String,
    pub grpc_url: String,
    pub health: String,
    pub capabilities: Capability,
    kpaper: Option<ChildProc>,
    runner: Option<ChildProc>,
    runner_bin: PathBuf,
    pub catalog_path: PathBuf,
    pub ledger_path: PathBuf,
    pub journal_path: PathBuf,
    bot_path: PathBuf,
    levels_path: PathBuf,
    instruments_path: PathBuf,
    workspace: PathBuf,
    grpc_bind: String,
    health_port: u16,
    paper_port: u16,
    _instrument: PaperInstrument,
    bin: PaperBin,
}

#[derive(Clone, Copy)]
pub enum PaperInstrument {
    Spot,
    Futures,
}

#[derive(Clone, Copy)]
pub enum CatalogKind {
    TwoRest,
    SpotSignoff,
    FuturesSignoff,
    Shadow,
    AmendCap,
}

#[derive(Clone, Copy)]
enum PaperBin {
    Runner,
    C2,
}

impl PaperStack {
    pub async fn launch(
        artifacts: &Path,
        instrument: PaperInstrument,
        catalog: CatalogKind,
        execution_mode: &str,
    ) -> Result<Self> {
        Self::launch_inner(
            artifacts,
            instrument,
            Some(catalog),
            execution_mode,
            180_000,
            PaperBin::Runner,
        )
        .await
    }

    pub async fn launch_fast_recon(
        artifacts: &Path,
        instrument: PaperInstrument,
        catalog: CatalogKind,
        execution_mode: &str,
        recon_ms: u64,
    ) -> Result<Self> {
        Self::launch_inner(
            artifacts,
            instrument,
            Some(catalog),
            execution_mode,
            recon_ms,
            PaperBin::Runner,
        )
        .await
    }

    pub async fn launch_c2(artifacts: &Path) -> Result<Self> {
        Self::launch_inner(
            artifacts,
            PaperInstrument::Futures,
            None,
            "exchange",
            1_000,
            PaperBin::C2,
        )
        .await
    }

    async fn launch_inner(
        artifacts: &Path,
        instrument: PaperInstrument,
        catalog: Option<CatalogKind>,
        execution_mode: &str,
        recon_ms: u64,
        bin: PaperBin,
    ) -> Result<Self> {
        require_mongo()?;
        let workspace = workspace_root();
        let kpaper_dir = k_paper_dir()?;
        let pkg = match bin {
            PaperBin::Runner => ("runner", "runner"),
            PaperBin::C2 => ("btc_price_ma_app", "btc_price_ma_app"),
        };
        cargo_build(&workspace, &["build", "-q", "-p", pkg.0, "--bin", pkg.1])?;
        cargo_build(&kpaper_dir, &["build", "-q", "--bin", "k-paper"])?;
        let runner_bin = debug_bin(&workspace, pkg.1);
        let kpaper_bin = debug_bin(&kpaper_dir, "k-paper");
        if !runner_bin.exists() {
            bail!("missing binary {}", runner_bin.display());
        }
        if !kpaper_bin.exists() {
            bail!("missing k-paper binary {}", kpaper_bin.display());
        }
        let _ = (file_sha256(&runner_bin), file_sha256(&kpaper_bin));

        let run_id = format!("wf-{}", Uuid::new_v4().simple());
        let run_dir = artifacts.join(&run_id).join("run");
        fs::create_dir_all(&run_dir)?;

        let paper_port = allocate_port()?;
        let health_port = allocate_port()?;
        let grpc_port = allocate_port()?;
        let grpc_bind = format!("127.0.0.1:{grpc_port}");
        let http = format!("http://127.0.0.1:{paper_port}");
        let health = format!("http://127.0.0.1:{health_port}/health");
        let grpc_url = format!("http://127.0.0.1:{grpc_port}");

        let instruments = run_dir.join("instruments.yaml");
        fs::write(&instruments, instruments_yaml(instrument))?;
        let bot_path = run_dir.join("bot.json");
        let levels_path = run_dir.join("levels.json");
        let ledger_path = run_dir.join("execution-ledger.jsonl");
        let journal_path = run_dir.join("submission-journal.jsonl");
        fs::write(
            &bot_path,
            bot_json(
                paper_port,
                health_port,
                &grpc_bind,
                &run_dir,
                &run_id,
                instrument,
                execution_mode,
                recon_ms,
            ),
        )?;
        if let Some(catalog) = catalog {
            fs::write(&levels_path, catalog_json(catalog, &run_id))?;
        }
        if matches!(bin, PaperBin::C2) {
            fs::write(run_dir.join("strategies.json"), c2_strategies_json())?;
        }

        let mut k_cmd = Command::new(&kpaper_bin);
        k_cmd
            .env("PAPER_EXCHANGE_PORT", paper_port.to_string())
            .env("PAPER_EXCHANGE_CONFIG", &instruments)
            .env("PAPER_EXCHANGE_PROFILE", "lab")
            .env("PAPER_EXCHANGE_HOST", "127.0.0.1")
            .current_dir(&kpaper_dir);
        let kpaper = ChildProc::spawn("k-paper", k_cmd, &run_dir.join("k-paper.log"))?;
        wait_http(&format!("{http}/open_orders"), Duration::from_secs(20)).await?;

        let mut stack = Self {
            run_id,
            run_dir,
            http,
            grpc_url,
            health,
            capabilities: Capability::paper(),
            kpaper: Some(kpaper),
            runner: None,
            runner_bin,
            catalog_path: levels_path.clone(),
            ledger_path,
            journal_path,
            bot_path,
            levels_path,
            instruments_path: instruments,
            workspace,
            grpc_bind,
            health_port,
            paper_port,
            _instrument: instrument,
            bin,
        };
        match bin {
            PaperBin::Runner => stack.start_runner("runner.log").await?,
            PaperBin::C2 => stack.start_c2("runner.log").await?,
        }
        let _ = (stack.health_port, stack.paper_port, stack.grpc_bind.clone());
        Ok(stack)
    }

    pub async fn start_runner(&mut self, log_name: &str) -> Result<()> {
        if let Some(mut prev) = self.runner.take() {
            prev.terminate();
        }
        let mut cmd = Command::new(&self.runner_bin);
        cmd.arg("--mode")
            .arg("levels")
            .arg("--config")
            .arg(&self.bot_path)
            .arg("--levels-config")
            .arg(&self.levels_path)
            .env("TRACE_JAEGER_DISABLED", "1")
            .env("TRADING_LOG_JSON", "1")
            .env("TRADING_RUN_ID", &self.run_id)
            .env("TRADING_GRPC", &self.grpc_url)
            .current_dir(&self.workspace);
        let runner = ChildProc::spawn("runner", cmd, &self.run_dir.join(log_name))?;
        wait_health_cex_ready(&self.health, Duration::from_secs(45)).await?;
        self.runner = Some(runner);
        Ok(())
    }

    pub async fn start_c2(&mut self, log_name: &str) -> Result<()> {
        if let Some(mut prev) = self.runner.take() {
            prev.terminate();
        }
        let candles = self.workspace.join("scripts/inc9a_long_candles.json");
        if !candles.exists() {
            bail!("missing {}", candles.display());
        }
        let mut cmd = Command::new(&self.runner_bin);
        cmd.arg("--mode")
            .arg("strategies")
            .arg("--config")
            .arg(&self.bot_path)
            .arg("--strategies-config")
            .arg(self.run_dir.join("strategies.json"))
            .env("TRACE_JAEGER_DISABLED", "1")
            .env("TRADING_LOG_JSON", "1")
            .env("TRADING_RUN_ID", &self.run_id)
            .env("TRADING_GRPC", &self.grpc_url)
            .env("TRADING_TEST_CANDLES_PATH", &candles)
            .env("TRADING_TEST_NOW_MS", "259200000")
            .current_dir(&self.workspace);
        if log_name == "runner.log" {
            cmd.env("TRADING_TEST_FORCE_PLACE", "1")
                .env("TRADING_TEST_LIMIT_PRICE", "10000")
                .env("TRADING_TEST_ORDER_QTY", "0.001");
        }
        let runner = ChildProc::spawn("btc_price_ma_app", cmd, &self.run_dir.join(log_name))?;
        wait_health_cex_ready(&self.health, Duration::from_secs(60)).await?;
        self.runner = Some(runner);
        Ok(())
    }

    pub async fn restart_runner(&mut self) -> Result<()> {
        if let Some(mut r) = self.runner.take() {
            r.terminate();
            tokio::time::sleep(Duration::from_millis(300)).await;
            if !r.try_wait()? {
                r.terminate();
            }
        }
        match self.bin {
            PaperBin::Runner => self.start_runner("runner-restart.log").await,
            PaperBin::C2 => self.start_c2("runner-restart.log").await,
        }
    }

    pub async fn grpc(&self) -> Result<GrpcCtl> {
        GrpcCtl::connect(&self.grpc_url).await
    }

    pub async fn rest_orders(&self) -> Result<Vec<RestOrder>> {
        let v: serde_json::Value = self.http_get("open_orders").await?;
        let arr = v.as_array().cloned().unwrap_or_default();
        Ok(arr
            .iter()
            .map(|o| RestOrder {
                order_id: o
                    .get("orderId")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                client_order_id: o
                    .get("clientOrderId")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                instrument: o
                    .get("symbol")
                    .and_then(|x| x.as_str())
                    .or_else(|| o.get("instrument").and_then(|x| x.as_str()))
                    .unwrap_or("BTCUSDT")
                    .to_string(),
                price: parse_dec(o.get("price").unwrap_or(&serde_json::Value::Null)),
                amount: parse_dec(o.get("amount").unwrap_or(&serde_json::Value::Null)),
                filled: parse_dec(o.get("filledAmount").unwrap_or(&serde_json::Value::Null)),
                status: o
                    .get("status")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                side: o
                    .get("side")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect())
    }

    pub async fn rest_balances(&self) -> Result<Vec<RestBalance>> {
        let v: serde_json::Value = self.http_get("balances").await?;
        let arr = if let Some(a) = v.as_array() {
            a.clone()
        } else {
            v.get("balances")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default()
        };
        Ok(arr
            .iter()
            .map(|b| RestBalance {
                asset: b
                    .get("asset")
                    .or_else(|| b.get("symbol"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                total: parse_dec(
                    b.get("total")
                        .or_else(|| b.get("available"))
                        .unwrap_or(&serde_json::Value::Null),
                ),
            })
            .collect())
    }

    pub async fn rest_positions(&self) -> Result<Vec<RestPosition>> {
        let v: serde_json::Value = self.http_get("positions").await?;
        let arr = if let Some(a) = v.as_array() {
            a.clone()
        } else {
            v.get("positions")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default()
        };
        Ok(arr
            .iter()
            .map(|p| RestPosition {
                instrument: p
                    .get("instrument_id")
                    .or_else(|| p.get("symbol"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                quantity: parse_dec(
                    p.get("quantity")
                        .or_else(|| p.get("qty"))
                        .unwrap_or(&serde_json::Value::Null),
                ),
            })
            .collect())
    }

    pub async fn lab_post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/{}", self.http, path))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;
        if !resp.status().is_success() {
            bail!("POST {path} {}", resp.status());
        }
        let text = resp.text().await.unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(json!({}));
        }
        Ok(serde_json::from_str(&text).unwrap_or(json!({"raw": text})))
    }

    pub async fn wait_open_at(&self, price: &str) -> Result<RestOrder> {
        let want = rust_decimal::Decimal::from_str_exact(price).unwrap_or_default();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            let orders = self.rest_orders().await?;
            if let Some(o) = orders.iter().find(|o| {
                o.price.normalize() == want.normalize()
                    && (o.status == "New" || o.status == "PartiallyFilled")
            }) {
                return Ok(o.clone());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        bail!("no working order at price {price}")
    }

    pub async fn snapshot(&self) -> Result<RunnerSnapshot> {
        self.grpc().await?.snapshot().await
    }

    pub fn catalog(&self) -> Result<serde_json::Value> {
        let raw = fs::read_to_string(&self.catalog_path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn ledger_text(&self) -> String {
        fs::read_to_string(&self.ledger_path).unwrap_or_default()
    }

    pub fn journal_text(&self) -> String {
        fs::read_to_string(&self.journal_path).unwrap_or_default()
    }

    pub fn instance_id(&self, template: &str) -> String {
        format!("{template}_BTCUSDT")
    }

    pub fn persist_artifacts(&self) {
        let Some(dest) = self.run_dir.parent().map(|p| p.join("snapshots")) else {
            return;
        };
        let _ = fs::create_dir_all(&dest);
        for (src, name) in [
            (&self.catalog_path, "catalog.json"),
            (&self.ledger_path, "execution-ledger.jsonl"),
            (&self.journal_path, "submission-journal.jsonl"),
            (&self.bot_path, "bot.json"),
            (&self.instruments_path, "instruments.yaml"),
        ] {
            if src.exists() {
                let _ = fs::copy(src, dest.join(name));
            }
        }
        let strategies = self.run_dir.join("strategies.json");
        if strategies.exists() {
            let _ = fs::copy(&strategies, dest.join("strategies.json"));
        }
        for log in ["runner.log", "runner-restart.log", "k-paper.log"] {
            let src = self.run_dir.join(log);
            if src.exists() {
                let _ = fs::copy(&src, dest.join(log));
            }
        }
    }

    pub async fn write_truth(&self, tag: &str) -> Result<()> {
        let dest = self
            .run_dir
            .parent()
            .map(|p| p.join("snapshots").join(tag))
            .ok_or_else(|| anyhow::anyhow!("no snapshot dest"))?;
        fs::create_dir_all(&dest)?;
        for path in ["open_orders", "balances", "positions"] {
            match self.http_get(path).await {
                Ok(v) => {
                    fs::write(
                        dest.join(format!("rest-{}.json", path.replace('_', "-"))),
                        serde_json::to_vec_pretty(&v)?,
                    )?;
                }
                Err(e) => {
                    fs::write(
                        dest.join(format!("rest-{}-error.txt", path.replace('_', "-"))),
                        e.to_string(),
                    )?;
                }
            }
        }
        match self.snapshot().await {
            Ok(s) => {
                fs::write(
                    dest.join("grpc-status.json"),
                    serde_json::to_vec_pretty(&s)?,
                )?;
            }
            Err(e) => {
                fs::write(dest.join("grpc-status-error.txt"), e.to_string())?;
            }
        }
        Ok(())
    }

    async fn http_get(&self, path: &str) -> Result<serde_json::Value> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let resp = client
            .get(format!("{}/{}", self.http, path))
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        if !resp.status().is_success() {
            bail!("GET {path} {}", resp.status());
        }
        Ok(resp.json().await?)
    }
}

fn instruments_yaml(kind: PaperInstrument) -> String {
    let fut = match kind {
        PaperInstrument::Futures => "\n    instrument_type: futures",
        PaperInstrument::Spot => "",
    };
    let linear = match kind {
        PaperInstrument::Futures => {
            "\nlinear:\n  leverage: 10\n  taker_fee_bps: 6\n  funding_rate: \"0\"\n"
        }
        PaperInstrument::Spot => "",
    };
    format!(
        "instruments:\n  - id: BTC-USDT\n    symbol: BTC-USDT\n    base_asset: BTC\n    quote_asset: USDT\n    tick_size: 0.01\n    lot_size: 0.001\n    min_notional: 10{fut}\ninitial_balances:\n  - asset: USDT\n    amount: 10000000\n{linear}fill_model:\n  kind: immediate_cross\n  slippage_bps: 0\n  fee_bps: 0\n  partial_fills: false\nmax_trade_history: 10000\nmarket:\n  interval_ms: 500\n  seed: 7\n  step: 100\n  spread_half: 50\n  levels: 5\n  trade_probability: 0.0\n  instruments:\n    - id: BTC-USDT\n      initial_mid: 50000\n"
    )
}

fn bot_json(
    paper_port: u16,
    health_port: u16,
    grpc_bind: &str,
    run_dir: &Path,
    run_id: &str,
    instrument: PaperInstrument,
    execution_mode: &str,
    recon_ms: u64,
) -> String {
    let inst = match instrument {
        PaperInstrument::Spot => "Spot",
        PaperInstrument::Futures => "Futures",
    };
    let lev = match instrument {
        PaperInstrument::Futures => ",\n        \"leverage\": 10",
        PaperInstrument::Spot => "",
    };
    format!(
        r#"{{
  "system": {{ "log_level": "info", "shutdown_timeout_secs": 10 }},
  "venue": {{ "mode": "cex" }},
  "execution_mode": "{execution_mode}",
  "exchange_engine": {{
    "exchange": "Paper",
    "config": {{
      "paper_exchange_url": "ws://127.0.0.1:{paper_port}/ws/trade",
      "pairs": [{{ "base": "BTC", "quote": "USDT" }}],
      "instrument_type": "{inst}"{lev}
    }}
  }},
  "server": {{ "bind_address": "127.0.0.1", "port": {health_port}, "grpc_bind": "{grpc_bind}" }},
  "redis": {{ "url": "", "enabled": false }},
  "features": {{ "enable_reconciliation": true }},
  "reconciliation": {{
    "exchange_level_interval_ms": {recon_ms},
    "trading_level_interval_ms": {recon_ms},
    "enabled": true
  }},
  "mongo": {{
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system_{run_id}",
    "bot_id": "{run_id}",
    "enabled": true
  }},
  "submission_journal_path": "{journal}",
  "execution_ledger_path": "{ledger}",
  "shadow_journal_path": "{shadow}",
  "runtime_snapshot_path": "{snap}",
  "restart_flag_path": "{flag}"
}}"#,
        journal = run_dir.join("submission-journal.jsonl").display(),
        ledger = run_dir.join("execution-ledger.jsonl").display(),
        shadow = run_dir.join("shadow-journal.jsonl").display(),
        snap = run_dir.join("runtime-snapshot.json").display(),
        flag = run_dir.join("restart.requested").display(),
    )
}

fn level(
    id: &str,
    name: &str,
    side: &str,
    qty: &str,
    price: &str,
    max_pos: &str,
) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "side": side,
        "order_type": "Limit",
        "quantity": { "type": "Fixed", "value": qty },
        "price": { "Fixed": price },
        "risk": { "max_position": max_pos, "max_orders": 1 },
        "lifecycle": {
            "auto_repost": false,
            "auto_repost_on_cancel": false,
            "max_reposts": 0,
            "repost_delay_secs": 1,
            "repost_conditions": []
        },
        "enabled": false,
        "group": "workflow"
    })
}

fn catalog_json(kind: CatalogKind, run_id: &str) -> String {
    let (levels, account, instrument, risk) = match kind {
        CatalogKind::TwoRest => (
            vec![
                level("wf_rest_a", "rest A", "Buy", "0.01", "1000", "0.02"),
                level("wf_rest_b", "rest B", "Buy", "0.01", "1010", "0.02"),
            ],
            run_id,
            "Spot",
            json!({"total_max_position":"0.05","max_total_orders":8,"max_notional":"200","max_order_qty":"0.01"}),
        ),
        CatalogKind::SpotSignoff => (
            vec![
                level("wf_buy", "buy", "Buy", "0.001", "10000", "0.02"),
                level("wf_sell", "sell", "Sell", "0.001", "100000", "0.02"),
                level("wf_rest", "rest", "Buy", "0.01", "1010", "0.02"),
                level("wf_partial", "partial", "Buy", "0.01", "1020", "0.02"),
            ],
            run_id,
            "Spot",
            json!({"total_max_position":"0.05","max_total_orders":8,"max_notional":"200","max_order_qty":"0.01"}),
        ),
        CatalogKind::FuturesSignoff => (
            vec![
                level("wf_fut_buy", "fut buy", "Buy", "0.001", "10000", "0.002"),
                level(
                    "wf_fut_adverse",
                    "fut adverse",
                    "Buy",
                    "0.001",
                    "11000",
                    "0.003",
                ),
                level(
                    "wf_fut_sell",
                    "fut sell",
                    "Sell",
                    "0.001",
                    "100000",
                    "0.002",
                ),
                level(
                    "wf_fut_flip",
                    "fut flip",
                    "Sell",
                    "0.002",
                    "100000",
                    "0.003",
                ),
            ],
            run_id,
            "Futures",
            json!({"total_max_position":"0.01","max_total_orders":4,"max_notional":"400","max_order_qty":"0.002"}),
        ),
        CatalogKind::Shadow => (
            vec![level("wf_sh", "shadow", "Buy", "0.01", "1000", "0.02")],
            run_id,
            "Spot",
            json!({"total_max_position":"0.05","max_total_orders":4,"max_notional":"200","max_order_qty":"0.01"}),
        ),
        CatalogKind::AmendCap => (
            vec![level("wf_amend", "amend", "Buy", "0.01", "1010", "0.02")],
            run_id,
            "Spot",
            json!({"total_max_position":"0.02","max_total_orders":1,"max_notional":"200","max_order_qty":"0.01"}),
        ),
    };
    json!({
        "levels": levels,
        "instances": [],
        "tombstones": [],
        "risk_limits": risk,
        "venue": "Paper",
        "instrument": instrument,
        "account": account
    })
    .to_string()
}

fn c2_strategies_json() -> String {
    json!({
        "strategies": [{
            "strategy_type": "BTCPriceMALongFlatV1",
            "symbol": { "base": "BTC", "quote": "USDT" },
            "position_size": "0.001",
            "max_position": "0.001",
            "buy_threshold_percent": "0.5",
            "sell_threshold_percent": "0.5",
            "max_active_orders": 1,
            "price_history_size": 10,
            "min_price_history": 2,
            "min_order_size": "0.001",
            "step_size": "0.001",
            "tick_size": "0.01",
            "min_notional": "10.0",
            "lookback": 2,
            "max_entry_age_secs": 86400000,
            "max_market_data_age_ms": 5000,
            "initially_enabled": true
        }],
        "risk_limits": {
            "total_max_position": "0.01",
            "max_total_orders": 4,
            "max_notional": "1000",
            "max_order_qty": "0.01"
        }
    })
    .to_string()
}

impl Drop for PaperStack {
    fn drop(&mut self) {
        self.persist_artifacts();
        if let Some(mut r) = self.runner.take() {
            r.terminate();
        }
        if let Some(mut k) = self.kpaper.take() {
            k.terminate();
        }
    }
}

#[async_trait::async_trait]
impl crate::venue::VenueHarness for PaperStack {
    fn venue_name(&self) -> &'static str {
        "PAPER"
    }

    fn capabilities(&self) -> crate::types::Capability {
        self.capabilities
    }

    async fn snapshot(&self) -> Result<crate::venue::RunnerSnapshot> {
        PaperStack::snapshot(self).await
    }

    async fn rest_orders(&self) -> Result<Vec<crate::venue::RestOrder>> {
        PaperStack::rest_orders(self).await
    }

    async fn rest_balances(&self) -> Result<Vec<crate::venue::RestBalance>> {
        PaperStack::rest_balances(self).await
    }

    async fn rest_positions(&self) -> Result<Vec<crate::venue::RestPosition>> {
        PaperStack::rest_positions(self).await
    }
}
