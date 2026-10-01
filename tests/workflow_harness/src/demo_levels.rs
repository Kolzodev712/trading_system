//! Bounded Bybit Demo Levels mutation. Never runs without explicit opt-in env.

use anyhow::{bail, Context, Result};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::demo::{write_demo_truth, DemoAdapter, SecretBotFile};
use crate::grpc::GrpcCtl;
use crate::oracle::{
    journal_has_state, journaled_client_id_after, latest_level_client_order_id, qty_canon,
    qty_close, rest_btc_position, runner_btc_qty, unique_ledger_qty, working_ids,
};
use crate::process::{
    allocate_port, cargo_build, debug_bin, require_mongo, wait_health_cex_ready, workspace_root,
    ChildProc,
};
use crate::types::{ScenarioResult, Verdict};
use crate::venue::{RestOrder, RestPosition};

const MAX_QTY: &str = "0.001";
const MAX_NOTIONAL: &str = "150";
const DEADLINE_SECS: u64 = 300;
const ENTRY_SECS: u64 = 180;
const LEVEL_ID: &str = "wf_demo_buy";
const INSTRUMENT: &str = "BTCUSDT";
const SIDE: &str = "Buy";

/// Runner expands the template to `{template}_{BASE}{QUOTE}`.
pub fn expanded_instance_id(template_id: &str, instrument: &str) -> String {
    format!("{template_id}_{instrument}")
}

#[derive(Debug)]
pub struct MutateAuth {
    pub bot_path: PathBuf,
    pub operator_id: String,
    pub operator_token: String,
}

pub fn parse_mutate_auth() -> Result<MutateAuth, String> {
    require_exact("WORKFLOW_DEMO_MUTATE", "1")?;
    require_exact("WORKFLOW_DEMO_VENUE", "BYBIT")?;
    require_exact("WORKFLOW_DEMO_ENVIRONMENT", "DEMO")?;
    require_exact("WORKFLOW_DEMO_CATEGORY", "linear")?;
    require_exact("WORKFLOW_DEMO_SYMBOL", "BTCUSDT")?;
    require_exact("WORKFLOW_DEMO_DEADLINE_SECS", "300")?;
    require_exact("WORKFLOW_DEMO_MAX_QTY", MAX_QTY)?;
    require_exact("WORKFLOW_DEMO_MAX_NOTIONAL", MAX_NOTIONAL)?;
    let bot = std::env::var("WORKFLOW_DEMO_BOT")
        .map_err(|_| "WORKFLOW_DEMO_BOT is required (isolated Demo bot.json)".to_string())?;
    let bot_path = PathBuf::from(bot);
    if !bot_path.exists() {
        return Err(format!(
            "WORKFLOW_DEMO_BOT does not exist: {}",
            bot_path.display()
        ));
    }
    let operator_id = nonempty("TRADING_OPERATOR_ID")?;
    let operator_token = nonempty("TRADING_OPERATOR_TOKEN")?;
    Ok(MutateAuth {
        bot_path,
        operator_id,
        operator_token,
    })
}

fn require_exact(key: &str, expect: &str) -> Result<(), String> {
    match std::env::var(key) {
        Ok(v) if v.trim() == expect => Ok(()),
        Ok(v) => Err(format!("{key} must be {expect}, got {v}")),
        Err(_) => Err(format!("{key}={expect} is required")),
    }
}

fn nonempty(key: &str) -> Result<String, String> {
    std::env::var(key)
        .map(|s| s.trim().to_string())
        .map_err(|_| format!("{key} is required"))
        .and_then(|s| {
            if s.is_empty() {
                Err(format!("{key} is empty"))
            } else {
                Ok(s)
            }
        })
}

#[derive(Debug, Clone)]
pub struct InstrumentRules {
    pub qty_step: Decimal,
    pub min_qty: Decimal,
    pub min_notional: Decimal,
    pub tick: Decimal,
}

pub fn plan_qty(
    rules: &InstrumentRules,
    last: Decimal,
    max_qty: Decimal,
    max_notional: Decimal,
) -> Result<Decimal, String> {
    if last <= Decimal::ZERO {
        return Err("last price must be positive".into());
    }
    if rules.min_qty > max_qty {
        return Err(format!(
            "minOrderQty {} exceeds max qty {}",
            rules.min_qty, max_qty
        ));
    }
    if rules.min_notional > max_notional {
        return Err(format!(
            "minNotional {} exceeds max notional {}",
            rules.min_notional, max_notional
        ));
    }
    let by_notional = max_notional / last;
    let raw = if by_notional < max_qty {
        by_notional
    } else {
        max_qty
    };
    let qty = floor_to_step(raw, rules.qty_step);
    if qty < rules.min_qty {
        return Err(format!(
            "bounded qty {qty} is below minOrderQty {} (last={last})",
            rules.min_qty
        ));
    }
    if qty * last < rules.min_notional {
        return Err(format!(
            "bounded notional {} is below minNotional {} (qty={qty} last={last})",
            qty * last,
            rules.min_notional
        ));
    }
    if qty * last > max_notional {
        return Err(format!(
            "bounded notional {} exceeds max {}",
            qty * last,
            max_notional
        ));
    }
    Ok(qty)
}

fn floor_to_step(qty: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return qty;
    }
    (qty / step).trunc() * step
}

fn align_tick(price: Decimal, tick: Decimal) -> Decimal {
    if tick <= Decimal::ZERO {
        return price;
    }
    (price / tick).trunc() * tick
}

fn d(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap_or(Decimal::ZERO)
}

pub async fn run(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("demo_levels_bounded", "BYBIT", "Futures", "exchange");
    r.note("bounded Demo Levels; isolated runner; no Candidate #2; no synthetic candles");
    let auth = match parse_mutate_auth() {
        Ok(a) => a,
        Err(e) => {
            r.fail(
                "explicit_mutate_opt_in",
                "all required Demo mutate env set",
                &e,
            );
            return r;
        }
    };
    r.note(format!(
        "opt-in BYBIT/DEMO linear/BTCUSDT 300s bot={} operator_id set token set",
        auth.bot_path.display()
    ));
    let adapter = match DemoAdapter::new() {
        Ok(a) => a,
        Err(e) => {
            r.fail("adapter", "demo adapter", &e.to_string());
            return r;
        }
    };
    if let Err(e) = run_inner(&mut r, artifacts, &auth, &adapter).await {
        if r.verdict != Verdict::Fail {
            r.fail("workflow", "ok", &e.to_string());
        }
        operator_alert(&r, artifacts, &e.to_string());
    }
    r
}

#[derive(Debug, Clone)]
pub struct OwnedOrderSpec {
    pub client_order_id: String,
    pub instrument: String,
    pub side: String,
    pub qty: Decimal,
    pub price: Decimal,
}

/// Exact journaled client id + instrument/side/qty/price. A nonempty id alone is not enough.
pub fn rest_order_matches_owned(order: &RestOrder, spec: &OwnedOrderSpec) -> bool {
    if spec.client_order_id.is_empty() || order.client_order_id != spec.client_order_id {
        return false;
    }
    order.instrument.eq_ignore_ascii_case(&spec.instrument)
        && order.side.eq_ignore_ascii_case(&spec.side)
        && qty_close(order.amount, spec.qty)
        && qty_close(order.price, spec.price)
        && (order.status == "New" || order.status == "PartiallyFilled" || order.status == "Filled")
}

/// Open-order list first, then exact-ID history. Immediate fills may skip realtime.
pub fn owned_order_from_open_or_history(
    open: &[RestOrder],
    history: Option<&RestOrder>,
    spec: &OwnedOrderSpec,
) -> Option<RestOrder> {
    if let Some(order) = open
        .iter()
        .find(|order| rest_order_matches_owned(order, spec))
    {
        return Some(order.clone());
    }
    history
        .filter(|order| rest_order_matches_owned(order, spec))
        .cloned()
}

/// REST errors are unverified, never an empty account.
pub fn cleanup_verification(
    orders: Result<Vec<RestOrder>, String>,
    positions: Result<Vec<RestPosition>, String>,
    entries_paused: Option<bool>,
    levels_disabled: Option<bool>,
) -> Result<(), String> {
    let orders = orders.map_err(|e| format!("REST orders unverified (not empty): {e}"))?;
    let positions = positions.map_err(|e| format!("REST positions unverified (not empty): {e}"))?;
    let working = working_ids(&orders);
    let qty = rest_btc_position(&positions);
    if !working.is_empty() || !qty_close(qty, Decimal::ZERO) {
        return Err(format!(
            "REST not flat: working={} pos={}",
            working.len(),
            qty
        ));
    }
    match entries_paused {
        Some(true) => {}
        Some(false) => return Err("runner entries not paused".into()),
        None => return Err("runner snapshot unverified".into()),
    }
    match levels_disabled {
        Some(true) => {}
        Some(false) => return Err("level still enabled".into()),
        None => return Err("runner snapshot unverified".into()),
    }
    Ok(())
}

/// Failed GetLevels or a missing instance is unverified. Template-only ids do not count.
pub fn levels_disabled_proof(
    levels_verified: bool,
    levels: &[(String, bool, String)],
    required_instance_id: &str,
) -> Option<bool> {
    if !levels_verified || required_instance_id.is_empty() {
        return None;
    }
    let Some((_, enabled, _)) = levels.iter().find(|(id, _, _)| id == required_instance_id) else {
        return None;
    };
    Some(!*enabled && levels.iter().all(|(_, other, _)| !other))
}

/// The paused runner must already expose the expanded instance, not the template id.
pub fn resolve_paused_instance_id(
    levels_verified: bool,
    levels: &[(String, bool, String)],
    template_id: &str,
    instrument: &str,
) -> Result<String, String> {
    if !levels_verified {
        return Err("GetLevels unverified while paused".into());
    }
    let expected = expanded_instance_id(template_id, instrument);
    match levels.iter().find(|(id, _, _)| id == &expected) {
        Some((_, false, _)) => Ok(expected),
        Some((_, true, _)) => Err(format!("{expected} enabled before Resume")),
        None => Err(format!(
            "paused runner missing instance {expected} (template {template_id} is not the gRPC id)"
        )),
    }
}

/// Cleanup retries must be new operations. Reusing a cached request id can
/// replay a failed CancelWorking / FlattenAndPause after REST recovers.
pub fn next_cleanup_request_id(kind: &str, attempt: u32) -> String {
    format!("wf-demo-cleanup-{kind}-{attempt}")
}

async fn run_inner(
    r: &mut ScenarioResult,
    artifacts: &Path,
    auth: &MutateAuth,
    adapter: &DemoAdapter,
) -> Result<()> {
    let rules = fetch_rules(adapter).await?;
    r.note(format!(
        "instruments-info step={} minQty={} minNotional={} tick={}",
        rules.qty_step, rules.min_qty, rules.min_notional, rules.tick
    ));
    let last = fetch_last(adapter).await?;
    r.note(format!("ticker last={last}"));
    let qty = match plan_qty(&rules, last, d(MAX_QTY), d(MAX_NOTIONAL)) {
        Ok(q) => q,
        Err(e) => {
            r.fail("preflight_instrument_bounds", "qty fits 0.001/150", &e);
            return Ok(());
        }
    };
    r.assert_eq(
        "planned_qty_le_max",
        "true",
        (qty <= d(MAX_QTY)).to_string(),
    );

    let rest_pos = adapter.signed_positions().await?;
    let rest_orders = adapter.signed_orders().await?;
    r.assert_eq(
        "preflight_rest_flat",
        "0",
        working_ids(&rest_orders).len().to_string(),
    );
    r.assert_eq(
        "preflight_rest_position",
        "0",
        qty_canon(rest_btc_position(&rest_pos)),
    );
    if !working_ids(&rest_orders).is_empty()
        || !qty_close(rest_btc_position(&rest_pos), Decimal::ZERO)
    {
        r.fail(
            "isolated_account",
            "zero working orders and zero BTC position",
            "account not empty",
        );
        return Ok(());
    }
    let (mode, leverage) = fetch_mode_leverage(adapter).await?;
    r.assert_eq("position_mode", "one_way", mode);
    r.assert_eq("leverage", "1", leverage.normalize().to_string());
    if r.verdict == Verdict::Fail {
        return Ok(());
    }

    let rest_price = align_tick(last * d("0.5"), rules.tick);
    let fill_price = align_tick(last + rules.tick * Decimal::from(5), rules.tick);
    if rest_price * qty < rules.min_notional {
        r.fail(
            "rest_price_min_notional",
            "far-from-market rest still meets min notional",
            &format!("qty={qty} rest={rest_price}"),
        );
        return Ok(());
    }

    write_demo_truth(artifacts, "preflight", &rest_orders, &rest_pos, &[], None)?;

    let mut stack = DemoLevelsRunner::launch(artifacts, auth, qty, rest_price).await?;
    let mut grpc = GrpcCtl::connect_with_token(&stack.grpc_url, Some(&auth.operator_token)).await?;
    let clock = Instant::now();
    let entry_deadline = clock + Duration::from_secs(ENTRY_SECS);
    let hard_deadline = clock + Duration::from_secs(DEADLINE_SECS);
    let instance_id =
        match wait_paused_instance(&mut grpc, Instant::now() + Duration::from_secs(30)).await {
            Ok(id) => {
                r.assert_eq(
                    "paused_instance_id",
                    &expanded_instance_id(LEVEL_ID, INSTRUMENT),
                    &id,
                );
                id
            }
            Err(e) => {
                r.fail(
                    "paused_instance_id",
                    &expanded_instance_id(LEVEL_ID, INSTRUMENT),
                    &e.to_string(),
                );
                expanded_instance_id(LEVEL_ID, INSTRUMENT)
            }
        };

    let started = if r.verdict == Verdict::Fail {
        Err(anyhow::anyhow!(
            "paused instance was not verified; Resume skipped"
        ))
    } else {
        run_after_start(
            r,
            adapter,
            &mut grpc,
            &stack,
            &instance_id,
            qty,
            rest_price,
            fill_price,
            entry_deadline,
        )
        .await
    };
    let filled = started.as_ref().ok().copied().unwrap_or(Decimal::ZERO);
    let cleanup = protective_cleanup(
        r,
        adapter,
        &mut grpc,
        &stack,
        &instance_id,
        Some(filled),
        hard_deadline,
    )
    .await;
    if let Err(e) = cleanup {
        r.fail(
            "cleanup",
            "verified flat REST + paused + level disabled",
            &e.to_string(),
        );
        operator_alert(r, artifacts, &e.to_string());
        write_recovery_handoff(artifacts, &stack, &e.to_string());
        stack.disarm();
        return Ok(());
    }
    if let Err(e) = started {
        r.fail("workflow", "ok", &e.to_string());
        return Ok(());
    }
    if Instant::now() > hard_deadline && r.verdict != Verdict::Fail {
        r.fail(
            "deadline_without_verified_flat_check",
            "flat before 300s",
            "deadline elapsed; cleanup continued",
        );
    }

    let rest_end = adapter.signed_positions().await?;
    let orders_end = adapter.signed_orders().await?;
    let snap_end = grpc.snapshot().await?;
    write_demo_truth(
        artifacts,
        "final",
        &orders_end,
        &rest_end,
        &[],
        Some(&snap_end),
    )?;
    r.assert_eq(
        "end_entries_paused",
        "true",
        snap_end.entries_paused.to_string(),
    );
    r.assert_eq(
        "end_level_disabled",
        "true",
        levels_disabled_proof(snap_end.levels_verified, &snap_end.levels, &instance_id)
            .unwrap_or(false)
            .to_string(),
    );
    r.assert_eq(
        "end_rest_orders",
        "0",
        working_ids(&orders_end).len().to_string(),
    );
    r.assert_eq(
        "end_rest_position",
        "0",
        qty_canon(rest_btc_position(&rest_end)),
    );
    r.assert_eq("end_runner_qty", "0", qty_canon(runner_btc_qty(&snap_end)));
    r.assert_eq("end_runner_orders", "0", snap_end.orders.len().to_string());

    let journal = stack.journal_text();
    r.assert_true(
        "journal_has_submission",
        journal_has_state(&journal, "prepared")
            || journal_has_state(&journal, "submitted")
            || journal.contains("order"),
        "journaled",
        if journal.is_empty() {
            "empty"
        } else {
            "present"
        },
    );
    let ledger_qty = unique_ledger_qty(&stack.ledger_text());
    r.note(format!("ledger unique signed qty={ledger_qty}"));

    if qty_close(filled, Decimal::ZERO) && r.verdict == Verdict::Pass {
        r.mark_not_run(
            "fill_flatten",
            "no fill within entry window; place/cancel proven; not full Demo Levels qualification",
        );
        r.note("placement/cancel PASS; fill/flatten NOT_RUN");
    }
    let _ = stack;
    Ok(())
}

async fn run_after_start(
    r: &mut ScenarioResult,
    adapter: &DemoAdapter,
    grpc: &mut GrpcCtl,
    stack: &DemoLevelsRunner,
    instance_id: &str,
    qty: Decimal,
    rest_price: Decimal,
    fill_price: Decimal,
    entry_deadline: Instant,
) -> Result<Decimal> {
    let snap = grpc.snapshot().await?;
    r.assert_eq("venue", "BYBIT", snap.venue.to_uppercase());
    r.assert_true(
        "environment_demo",
        snap.environment.to_uppercase().contains("DEMO"),
        "DEMO",
        &snap.environment,
    );
    r.assert_eq(
        "entries_paused_start",
        "true",
        snap.entries_paused.to_string(),
    );
    r.assert_eq(
        "execution_mode",
        "exchange",
        snap.execution_mode.to_lowercase(),
    );
    if r.verdict == Verdict::Fail {
        return Err(anyhow::anyhow!("pre-resume snapshot assertions failed"));
    }

    grpc.enable().await.context("SetTradingEnabled")?;
    grpc.resume_entries().await.context("ResumeEntries")?;
    r.note("one authenticated entry-enable (ResumeEntries) after independent preflight");

    place_and_cancel(
        r,
        adapter,
        grpc,
        qty,
        rest_price,
        entry_deadline,
        stack,
        instance_id,
    )
    .await?;
    if r.verdict == Verdict::Fail {
        return Err(anyhow::anyhow!("place/cancel assertions failed"));
    }

    let mut filled = rest_btc_position(&adapter.signed_positions().await?);
    if qty_close(filled, Decimal::ZERO) && Instant::now() < entry_deadline {
        attempt_fill(
            r,
            adapter,
            grpc,
            qty,
            fill_price,
            entry_deadline,
            stack,
            instance_id,
        )
        .await?;
        filled = rest_btc_position(&adapter.signed_positions().await?);
    }
    Ok(filled)
}

async fn place_and_cancel(
    r: &mut ScenarioResult,
    adapter: &DemoAdapter,
    grpc: &mut GrpcCtl,
    qty: Decimal,
    rest_price: Decimal,
    entry_deadline: Instant,
    stack: &DemoLevelsRunner,
    instance_id: &str,
) -> Result<()> {
    grpc.update_level_price(instance_id, &rest_price.normalize().to_string())
        .await?;
    let watermark = latest_level_client_order_id(&stack.journal_text(), instance_id);
    grpc.toggle_level(instance_id, true).await?;
    r.note(format!("create 1/2 rest qty={qty} price={rest_price}"));
    let client_order_id =
        wait_journaled_client_id(stack, instance_id, entry_deadline, watermark.as_deref()).await?;
    let spec = OwnedOrderSpec {
        client_order_id,
        instrument: INSTRUMENT.into(),
        side: SIDE.into(),
        qty,
        price: rest_price,
    };
    let order = wait_owned_order(adapter, &spec, entry_deadline).await?;
    r.assert_eq(
        "rest_client_id",
        &spec.client_order_id,
        &order.client_order_id,
    );
    r.assert_eq("rest_side", SIDE, order.side.clone());
    r.assert_eq("rest_instrument", INSTRUMENT, order.instrument.clone());
    write_demo_truth(
        &stack.artifacts,
        "resting",
        std::slice::from_ref(&order),
        &[],
        &[],
        None,
    )?;
    grpc.toggle_level(instance_id, false).await?;
    r.note("disable first level; CancelWorking is reserved for protective cleanup");
    if !wait_order_gone(adapter, &order.order_id, Duration::from_secs(30)).await {
        bail!("REST still lists disabled order {}", order.order_id);
    }
    r.assert_eq("rest_cancel_removed", "true", "true");
    Ok(())
}

async fn attempt_fill(
    r: &mut ScenarioResult,
    adapter: &DemoAdapter,
    grpc: &mut GrpcCtl,
    qty: Decimal,
    fill_price: Decimal,
    entry_deadline: Instant,
    stack: &DemoLevelsRunner,
    instance_id: &str,
) -> Result<()> {
    grpc.update_level_price(instance_id, &fill_price.normalize().to_string())
        .await?;
    let snap = grpc.snapshot().await?;
    r.assert_eq(
        "entries_paused_before_second_create",
        "false",
        snap.entries_paused.to_string(),
    );
    if snap.entries_paused {
        return Err(anyhow::anyhow!(
            "entries still paused before second create; CancelWorking must not run between attempts"
        ));
    }
    let watermark = latest_level_client_order_id(&stack.journal_text(), instance_id);
    grpc.toggle_level(instance_id, true).await?;
    r.note(format!(
        "create 2/2 fill attempt qty={qty} price={fill_price}"
    ));
    let client_order_id =
        wait_journaled_client_id(stack, instance_id, entry_deadline, watermark.as_deref()).await?;
    let spec = OwnedOrderSpec {
        client_order_id,
        instrument: INSTRUMENT.into(),
        side: SIDE.into(),
        qty,
        price: fill_price,
    };
    let order = wait_owned_order(adapter, &spec, entry_deadline).await?;
    r.note(format!(
        "fill-leg owned client_id={} order={} status={}",
        order.client_order_id, order.order_id, order.status
    ));
    if order.status == "Filled" || order.status == "PartiallyFilled" {
        r.note(format!(
            "exact-id {} confirmed via open or history as {}",
            order.client_order_id, order.status
        ));
    }
    let deadline = entry_deadline.min(Instant::now() + Duration::from_secs(45));
    while Instant::now() < deadline {
        let pos = rest_btc_position(&adapter.signed_positions().await?);
        if pos > Decimal::ZERO {
            r.assert_true(
                "fill_qty_le_max",
                pos <= d(MAX_QTY),
                "<=0.001",
                &qty_canon(pos),
            );
            let ledger_qty = unique_ledger_qty(&stack.ledger_text());
            r.note(format!(
                "filled rest qty={pos} ledger_qty={ledger_qty} client_id={}",
                order.client_order_id
            ));
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    grpc.toggle_level(instance_id, false).await?;
    Ok(())
}

async fn protective_cleanup(
    r: &mut ScenarioResult,
    adapter: &DemoAdapter,
    grpc: &mut GrpcCtl,
    stack: &DemoLevelsRunner,
    instance_id: &str,
    known_pos: Option<Decimal>,
    hard_deadline: Instant,
) -> Result<()> {
    r.note("protective cleanup: pause, disable, cancel, flatten if needed");
    let give_up = Instant::now() + Duration::from_secs(90);
    let deadline = give_up.min(hard_deadline);
    let mut attempt = 0u32;
    loop {
        let orders = adapter.signed_orders().await.map_err(|e| e.to_string());
        let positions = adapter.signed_positions().await.map_err(|e| e.to_string());
        let snap = grpc.snapshot().await.ok();
        let paused = snap.as_ref().map(|s| s.entries_paused);
        let disabled = snap
            .as_ref()
            .and_then(|s| levels_disabled_proof(s.levels_verified, &s.levels, instance_id));
        match cleanup_verification(orders.clone(), positions.clone(), paused, disabled) {
            Ok(()) => {
                if let (Ok(orders), Ok(positions)) = (orders, positions) {
                    let _ = write_demo_truth(
                        &stack.artifacts,
                        "cleanup",
                        &orders,
                        &positions,
                        &[],
                        snap.as_ref(),
                    );
                }
                return Ok(());
            }
            Err(e) => {
                if Instant::now() > deadline {
                    eprintln!(
                        "OPERATOR ALERT: Demo Levels cleanup not verified; {e}; artifacts={} grpc={} pid={}",
                        stack.artifacts.display(),
                        stack.grpc_url,
                        stack.pid()
                    );
                    bail!("{e}");
                }
                r.note(format!("cleanup retry after venue/journal check: {e}"));
                attempt += 1;
                retry_note(
                    r,
                    grpc.pause_entries_named(&next_cleanup_request_id("pause", attempt))
                        .await,
                    "pause",
                );
                retry_note(r, grpc.toggle_level(instance_id, false).await, "disable");
                retry_note(
                    r,
                    grpc.cancel_working(&next_cleanup_request_id("cancel", attempt))
                        .await,
                    "cancel_working",
                );
                let pos = match known_pos {
                    Some(p) if p > Decimal::ZERO => Some(p),
                    _ => match adapter.signed_positions().await {
                        Ok(rows) => Some(rest_btc_position(&rows)),
                        Err(rest_err) => {
                            r.note(format!(
                                "REST positions unverified during flatten check: {rest_err}"
                            ));
                            None
                        }
                    },
                };
                if pos.is_some_and(|p| p > Decimal::ZERO) {
                    r.note(format!(
                        "reduce-only flatten of independently verified qty={}",
                        pos.unwrap_or_default()
                    ));
                    retry_note(
                        r,
                        grpc.flatten_and_pause(&next_cleanup_request_id("flatten", attempt))
                            .await,
                        "flatten",
                    );
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }
    }
}

fn retry_note<T, E: std::fmt::Display>(r: &mut ScenarioResult, result: Result<T, E>, what: &str) {
    if let Err(e) = result {
        r.note(format!("{what} failed (will retry): {e}"));
    }
}

fn operator_alert(r: &ScenarioResult, artifacts: &Path, why: &str) {
    eprintln!(
        "OPERATOR ALERT: Demo Levels gate failed ({why}); preserve artifacts {}; first_failure={:?}",
        artifacts.display(),
        r.first_failure.as_ref().map(|f| &f.name)
    );
}

async fn wait_paused_instance(grpc: &mut GrpcCtl, deadline: Instant) -> Result<String> {
    let mut last = String::new();
    while Instant::now() < deadline {
        match grpc.snapshot().await {
            Ok(snap) => match resolve_paused_instance_id(
                snap.levels_verified,
                &snap.levels,
                LEVEL_ID,
                INSTRUMENT,
            ) {
                Ok(id) => return Ok(id),
                Err(e) => last = e,
            },
            Err(e) => last = e.to_string(),
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("paused instance {LEVEL_ID}_{INSTRUMENT} not verified: {last}")
}

async fn wait_journaled_client_id(
    stack: &DemoLevelsRunner,
    instance_id: &str,
    deadline: Instant,
    after: Option<&str>,
) -> Result<String> {
    let mut last = String::new();
    while Instant::now() < deadline {
        let journal = stack.journal_text();
        if let Some(cid) = journaled_client_id_after(&journal, instance_id, after) {
            return Ok(cid);
        }
        last = match (after, latest_level_client_order_id(&journal, instance_id)) {
            (Some(prev), Some(existing)) if existing == prev => {
                format!("still seeing previous client_order_id {prev}")
            }
            _ if journal.is_empty() => "journal empty".into(),
            _ => "no new level client_order_id yet".into(),
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("no new journaled client_order_id for {instance_id}: {last}")
}

async fn wait_owned_order(
    adapter: &DemoAdapter,
    spec: &OwnedOrderSpec,
    deadline: Instant,
) -> Result<RestOrder> {
    let mut last = String::new();
    while Instant::now() < deadline {
        let open = match adapter.signed_orders().await {
            Ok(orders) => orders,
            Err(e) => {
                last = format!("REST orders unverified: {e}");
                tokio::time::sleep(Duration::from_millis(300)).await;
                continue;
            }
        };
        let (history, history_note) =
            match adapter.signed_order_by_link_id(&spec.client_order_id).await {
                Ok(order) => (order, String::new()),
                Err(e) => (None, format!(" history_err={e}")),
            };
        if let Some(order) = owned_order_from_open_or_history(&open, history.as_ref(), spec) {
            return Ok(order);
        }
        last = format!(
            "open={} history={}{history_note} want client_id={} qty={} price={}",
            open.len(),
            history
                .as_ref()
                .map(|o| o.status.as_str())
                .unwrap_or("none"),
            spec.client_order_id,
            spec.qty,
            spec.price
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    bail!(
        "no REST order matching journaled {}: {last}",
        spec.client_order_id
    )
}

fn write_recovery_handoff(artifacts: &Path, stack: &DemoLevelsRunner, why: &str) {
    let body = format!(
        "OPERATOR RECOVERY HANDOFF\n\
         reason={why}\n\
         artifacts={}\n\
         grpc={}\n\
         runner_pid={}\n\
         The runner was left running (not killed).\n\
         Do not Resume. Use authenticated grpc_ctl PauseEntries, CancelWorking, FlattenAndPause.\n\
         Confirm Bybit Demo REST working orders=0 and position=0 before stopping the process.\n",
        artifacts.display(),
        stack.grpc_url,
        stack.pid()
    );
    let path = artifacts.join("OPERATOR_RECOVERY.txt");
    let _ = fs::write(&path, &body);
    eprintln!("{body}");
}

async fn wait_order_gone(adapter: &DemoAdapter, order_id: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(orders) = adapter.signed_orders().await {
            if !working_ids(&orders).iter().any(|id| id == order_id)
                && !orders.iter().any(|o| {
                    o.order_id == order_id && (o.status == "New" || o.status == "PartiallyFilled")
                })
            {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

async fn fetch_rules(adapter: &DemoAdapter) -> Result<InstrumentRules> {
    let v = adapter
        .public_get(
            "/v5/market/instruments-info",
            &[("category", "linear"), ("symbol", "BTCUSDT")],
        )
        .await?;
    let inst = v
        .get("list")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("instruments-info missing BTCUSDT"))?;
    let lot = inst.get("lotSizeFilter").cloned().unwrap_or(Value::Null);
    let price = inst.get("priceFilter").cloned().unwrap_or(Value::Null);
    Ok(InstrumentRules {
        qty_step: dec_field(&lot, "qtyStep"),
        min_qty: dec_field(&lot, "minOrderQty"),
        min_notional: {
            let n = dec_field(&lot, "minNotionalValue");
            if n > Decimal::ZERO {
                n
            } else {
                dec_field(&lot, "minOrderAmt")
            }
        },
        tick: dec_field(&price, "tickSize"),
    })
}

async fn fetch_last(adapter: &DemoAdapter) -> Result<Decimal> {
    let v = adapter
        .public_get(
            "/v5/market/tickers",
            &[("category", "linear"), ("symbol", "BTCUSDT")],
        )
        .await?;
    let last = v
        .get("list")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .map(|t| dec_field(t, "lastPrice"))
        .unwrap_or(Decimal::ZERO);
    if last <= Decimal::ZERO {
        bail!("ticker lastPrice missing");
    }
    Ok(last)
}

async fn fetch_mode_leverage(adapter: &DemoAdapter) -> Result<(String, Decimal)> {
    let v = adapter
        .signed_get(
            "/v5/position/list",
            &[("category", "linear"), ("symbol", "BTCUSDT")],
        )
        .await?;
    let row = v
        .get("list")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or(Value::Null);
    let idx = row
        .get("positionIdx")
        .and_then(|x| x.as_i64())
        .or_else(|| {
            row.get("positionIdx")
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0);
    if idx != 0 {
        bail!("account is not one-way (positionIdx={idx})");
    }
    let lev = dec_field(&row, "leverage");
    Ok(("one_way".into(), lev))
}

fn dec_field(v: &Value, key: &str) -> Decimal {
    v.get(key)
        .and_then(|x| x.as_str())
        .and_then(|s| Decimal::from_str_exact(s).ok())
        .or_else(|| {
            v.get(key)
                .and_then(|x| x.as_f64())
                .and_then(|f| Decimal::try_from(f).ok())
        })
        .unwrap_or(Decimal::ZERO)
}

struct DemoLevelsRunner {
    artifacts: PathBuf,
    grpc_url: String,
    journal_path: PathBuf,
    ledger_path: PathBuf,
    _secret: SecretBotFile,
    child: ChildProc,
}

impl DemoLevelsRunner {
    async fn launch(
        artifacts: &Path,
        auth: &MutateAuth,
        qty: Decimal,
        rest_price: Decimal,
    ) -> Result<Self> {
        require_mongo()?;
        let ws = workspace_root();
        cargo_build(&ws, &["build", "-q", "-p", "runner", "--bin", "runner"])?;
        let bin = debug_bin(&ws, "runner");
        if !bin.exists() {
            bail!("missing {}", bin.display());
        }
        let run_id = format!("demo-lv-{}", uuid::Uuid::new_v4().simple());
        let run_dir = artifacts.join(&run_id).join("run");
        fs::create_dir_all(&run_dir)?;
        let health_port = allocate_port()?;
        let grpc_port = allocate_port()?;
        let grpc_bind = format!("127.0.0.1:{grpc_port}");
        let grpc_url = format!("http://127.0.0.1:{grpc_port}");
        let mut bot: Value = serde_json::from_str(&fs::read_to_string(&auth.bot_path)?)?;
        isolate_bot(&mut bot, &run_dir, &run_id, health_port, &grpc_bind)?;
        let levels_path = run_dir.join("levels.json");
        fs::write(&levels_path, demo_levels_json(&run_id, qty, rest_price))?;
        let mut secret = SecretBotFile::create(run_dir.join("bot.json"), &bot)?;
        let mut cmd = Command::new(&bin);
        cmd.arg("--mode")
            .arg("levels")
            .arg("--config")
            .arg(secret.path())
            .arg("--levels-config")
            .arg(&levels_path)
            .env("TRACE_JAEGER_DISABLED", "1")
            .env("TRADING_LOG_JSON", "1")
            .env("TRADING_RUN_ID", &run_id)
            .env("TRADING_GRPC", &grpc_url)
            .env("TRADING_OPERATOR_ID", &auth.operator_id)
            .env("TRADING_OPERATOR_TOKEN", &auth.operator_token)
            .env_remove("TRADING_TEST_CANDLES_PATH")
            .env_remove("TRADING_TEST_NOW_MS")
            .env_remove("INC10_FRESH_ENTRY_FIXTURE")
            .env_remove("TRADING_TEST_FORCE_PLACE")
            .current_dir(&ws);
        let child = ChildProc::spawn("runner", cmd, &run_dir.join("runner.log"))?;
        wait_health_cex_ready(
            &format!("http://127.0.0.1:{health_port}/health"),
            Duration::from_secs(90),
        )
        .await?;
        secret.archive_redacted()?;
        Ok(Self {
            artifacts: artifacts.to_path_buf(),
            grpc_url,
            journal_path: run_dir.join("submission-journal.jsonl"),
            ledger_path: run_dir.join("execution-ledger.jsonl"),
            _secret: secret,
            child,
        })
    }

    fn disarm(&mut self) {
        self.child.disarm();
    }

    fn pid(&self) -> u32 {
        self.child.pid()
    }

    fn journal_text(&self) -> String {
        fs::read_to_string(&self.journal_path).unwrap_or_default()
    }

    fn ledger_text(&self) -> String {
        fs::read_to_string(&self.ledger_path).unwrap_or_default()
    }
}

fn isolate_bot(
    bot: &mut Value,
    run_dir: &Path,
    run_id: &str,
    health_port: u16,
    grpc_bind: &str,
) -> Result<()> {
    let obj = bot
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("bot.json is not an object"))?;
    obj.insert("execution_mode".into(), "exchange".into());
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
    if let Some(cfg) = obj
        .get_mut("exchange_engine")
        .and_then(|e| e.get_mut("config"))
        .and_then(|c| c.as_object_mut())
    {
        cfg.insert("instrumentType".into(), "Futures".into());
        cfg.insert("leverage".into(), json!(1));
        cfg.insert("demo".into(), json!(true));
        cfg.insert("testnet".into(), json!(false));
        cfg.insert("pairs".into(), json!([{"base":"BTC","quote":"USDT"}]));
    }
    if let Some(server) = obj.get_mut("server").and_then(|s| s.as_object_mut()) {
        server.insert("bind_address".into(), "127.0.0.1".into());
        server.insert("port".into(), health_port.into());
        server.insert("grpc_bind".into(), grpc_bind.into());
    } else {
        obj.insert(
            "server".into(),
            json!({"bind_address":"127.0.0.1","port":health_port,"grpc_bind":grpc_bind}),
        );
    }
    if let Some(mongo) = obj.get_mut("mongo").and_then(|m| m.as_object_mut()) {
        mongo.insert("bot_id".into(), run_id.into());
        mongo.insert("database".into(), format!("trading_system_{run_id}").into());
    }
    Ok(())
}

fn demo_levels_json(account: &str, qty: Decimal, price: Decimal) -> String {
    json!({
        "levels": [{
            "id": LEVEL_ID,
            "name": "demo bounded buy",
            "side": "Buy",
            "order_type": "Limit",
            "quantity": { "type": "Fixed", "value": qty.normalize().to_string() },
            "price": { "Fixed": price.normalize().to_string() },
            "risk": { "max_position": MAX_QTY, "max_orders": 1 },
            "lifecycle": {
                "auto_repost": false,
                "auto_repost_on_cancel": false,
                "max_reposts": 0,
                "repost_delay_secs": 1,
                "repost_conditions": []
            },
            "enabled": false,
            "group": "workflow-demo"
        }],
        "instances": [],
        "tombstones": [],
        "risk_limits": {
            "total_max_position": MAX_QTY,
            "max_total_orders": 1,
            "max_notional": MAX_NOTIONAL,
            "max_order_qty": MAX_QTY
        },
        "venue": "Bybit",
        "instrument": "Futures",
        "account": account
    })
    .to_string()
}
