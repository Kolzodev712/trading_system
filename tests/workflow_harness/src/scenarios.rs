use rust_decimal::Decimal;
use std::path::Path;
use std::time::Duration;

use crate::demo::DemoAdapter;
use crate::demo_levels;
use crate::oracle::{
    assert_spot_agreement, btc_total, catalog_filled_qty, catalog_owns,
    distinct_level_client_order_ids, halt_is_disagreement, journal_has_state,
    journaled_client_id_after, latest_level_client_order_id, qty_canon, qty_close,
    rest_btc_position, runner_btc_qty, unique_ledger_qty, working_ids,
};
use crate::paper::{CatalogKind, PaperInstrument, PaperStack};
use crate::process::ui_dir;
use crate::types::{Layer, ScenarioResult, Verdict};
use crate::venue::RunnerSnapshot;

fn d(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap_or(Decimal::ZERO)
}

fn require<T, E: std::fmt::Display>(
    r: &mut ScenarioResult,
    tag: &str,
    res: Result<T, E>,
) -> Option<T> {
    match res {
        Ok(v) => Some(v),
        Err(e) => {
            r.fail(tag, "ok", &e.to_string());
            None
        }
    }
}

fn empty_snap() -> RunnerSnapshot {
    RunnerSnapshot {
        venue: String::new(),
        environment: String::new(),
        execution_mode: String::new(),
        trading_enabled: false,
        entries_paused: false,
        flatten_latched: false,
        positions: vec![],
        orders: vec![],
        levels: vec![],
        levels_verified: false,
        exposure_halt_reason: String::new(),
        last_block_reason_code: String::new(),
        avg_entry: String::new(),
    }
}

async fn wait_spot_qty(
    stack: &PaperStack,
    result: &mut ScenarioResult,
    tag: &str,
    expect: Decimal,
    check_catalog: bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        let balances = match stack.rest_balances().await {
            Ok(b) => b,
            Err(e) => {
                result.fail(&format!("{tag}.rest_balances"), "ok", &e.to_string());
                return;
            }
        };
        let snap = match stack.snapshot().await {
            Ok(s) => s,
            Err(e) => {
                last = format!("grpc {e}");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        let paper = btc_total(&balances);
        let runner = runner_btc_qty(&snap);
        let cat = if check_catalog {
            match stack.catalog() {
                Ok(c) => Some(catalog_filled_qty(&c)),
                Err(e) => {
                    result.fail(&format!("{tag}.catalog"), "ok", &e.to_string());
                    return;
                }
            }
        } else {
            None
        };
        let led = Some(unique_ledger_qty(&stack.ledger_text()));
        last = format!("paper={paper} runner={runner}");
        if qty_close(paper, expect) && qty_close(runner, expect) {
            assert_spot_agreement(
                result,
                tag,
                expect,
                paper,
                runner,
                cat,
                led,
                &snap.exposure_halt_reason,
            );
            require_truth(result, stack, tag).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    result.fail(tag, &expect.to_string(), &last);
}

async fn require_truth(r: &mut ScenarioResult, stack: &PaperStack, tag: &str) {
    let _ = require(r, &format!("truth_{tag}"), stack.write_truth(tag).await);
}

async fn wait_fut_qty(stack: &PaperStack, result: &mut ScenarioResult, tag: &str, expect: Decimal) {
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        let rest = match stack.rest_positions().await {
            Ok(p) => rest_btc_position(&p),
            Err(e) => {
                result.fail(&format!("{tag}.rest_positions"), "ok", &e.to_string());
                return;
            }
        };
        let runner = match stack.snapshot().await {
            Ok(s) => runner_btc_qty(&s),
            Err(e) => {
                last = format!("grpc {e}");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        last = format!("rest={rest} runner={runner}");
        if qty_close(rest, expect) && qty_close(runner, expect) {
            result.assert_eq(&format!("{tag}.rest"), qty_canon(expect), qty_canon(rest));
            result.assert_eq(
                &format!("{tag}.runner"),
                qty_canon(expect),
                qty_canon(runner),
            );
            require_truth(result, stack, tag).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    result.fail(tag, &expect.to_string(), &last);
}

async fn wait_gone(stack: &PaperStack, order_id: &str) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        if let Ok(orders) = stack.rest_orders().await {
            if !working_ids(&orders).contains(&order_id.to_string()) {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

pub async fn run_layer(layer: Layer, only: Option<&str>, artifacts: &Path) -> Vec<ScenarioResult> {
    let mut out = Vec::new();
    match layer {
        Layer::Paper => {
            let all = [
                "paper_spot_levels",
                "paper_futures_levels",
                "paper_two_creates_one_resume",
                "paper_flatten_after_stale_entry",
                "paper_fill_ask_above_last",
                "paper_shadow_zero_mutations",
                "paper_lost_ack_restart",
                "paper_c2_exchange",
                "paper_control_plane",
                "paper_stale_and_rest_failure",
                "negative_control_mismatch",
            ];
            for name in all {
                if let Some(only) = only {
                    if name != only {
                        continue;
                    }
                }
                out.push(run_named(name, artifacts).await);
            }
        }
        Layer::Ui => {
            if only.map(|s| s != "paper_ui_envoy").unwrap_or(false) {
                return out;
            }
            out.push(paper_ui_envoy(artifacts).await);
        }
        Layer::DemoReadonly => match DemoAdapter::new() {
            Ok(d) => out.push(d.readonly_checks(artifacts).await),
            Err(e) => {
                let mut r = ScenarioResult::new("demo_readonly_paused", "BYBIT", "Spot", "paused");
                r.fail("adapter", "demo adapter", &e.to_string());
                out.push(r);
            }
        },
        Layer::DemoMutating => out.push(demo_levels::run(artifacts).await),
    }
    out
}

async fn run_named(name: &str, artifacts: &Path) -> ScenarioResult {
    match name {
        "paper_spot_levels" => paper_spot_levels(artifacts).await,
        "paper_futures_levels" => paper_futures_levels(artifacts).await,
        "paper_two_creates_one_resume" => paper_two_creates_one_resume(artifacts).await,
        "paper_flatten_after_stale_entry" => paper_flatten_after_stale_entry(artifacts).await,
        "paper_fill_ask_above_last" => paper_fill_ask_above_last(artifacts).await,
        "paper_shadow_zero_mutations" => paper_shadow(artifacts).await,
        "paper_lost_ack_restart" => paper_lost_ack(artifacts).await,
        "paper_c2_exchange" => paper_c2_exchange(artifacts).await,
        "paper_control_plane" => paper_control(artifacts).await,
        "paper_stale_and_rest_failure" => paper_stale_rest(artifacts).await,
        "negative_control_mismatch" => negative_control(artifacts).await,
        other => {
            let mut r = ScenarioResult::new(other, "PAPER", "Spot", "exchange");
            r.fail("unknown", other, "no such scenario");
            r
        }
    }
}

async fn paper_spot_levels(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_spot_levels", "PAPER", "Spot", "exchange");
    let mut stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::SpotSignoff,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "paper stack", &e.to_string());
            return r;
        }
    };
    r.note(format!("run_id={} grpc={}", stack.run_id, stack.grpc_url));
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }

    let buy = stack.instance_id("wf_buy");
    if require(&mut r, "toggle_buy", grpc.toggle_level(&buy, true).await).is_none() {
        return r;
    }
    let order = match require(&mut r, "rest_buy", stack.wait_open_at("10000").await) {
        Some(o) => o,
        None => return r,
    };
    r.note(format!("buy order {}", order.order_id));
    if require(
        &mut r,
        "lab_fill_buy",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": order.order_id}),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    wait_spot_qty(&stack, &mut r, "after_buy", d("0.001"), true).await;
    if r.verdict == Verdict::Fail {
        return r;
    }

    if require(&mut r, "restart", stack.restart_runner().await).is_none() {
        return r;
    }
    grpc = match require(&mut r, "grpc_after_restart", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    wait_spot_qty(&stack, &mut r, "after_restart", d("0.001"), false).await;
    let snap = match require(&mut r, "snapshot_after_restart", stack.snapshot().await) {
        Some(s) => s,
        None => empty_snap(),
    };
    r.assert_eq(
        "restart_entry_unknown",
        "?",
        if snap.avg_entry == "?" || snap.avg_entry.is_empty() {
            "?"
        } else {
            snap.avg_entry.as_str()
        },
    );
    r.assert_eq("venue", "PAPER", snap.venue.to_uppercase());
    r.assert_eq("environment", "LOCAL", snap.environment.to_uppercase());

    if require(&mut r, "enable_after_restart", grpc.enable().await).is_none() {
        return r;
    }
    let sell = stack.instance_id("wf_sell");
    if require(&mut r, "toggle_sell", grpc.toggle_level(&sell, true).await).is_none() {
        return r;
    }
    let sell_order = match require(&mut r, "rest_sell", stack.wait_open_at("100000").await) {
        Some(o) => o,
        None => return r,
    };
    if require(
        &mut r,
        "lab_fill_sell",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": sell_order.order_id}),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    wait_spot_qty(&stack, &mut r, "after_sell", Decimal::ZERO, false).await;

    let rest = stack.instance_id("wf_rest");
    if require(&mut r, "toggle_rest", grpc.toggle_level(&rest, true).await).is_none() {
        return r;
    }
    let rest_order = match require(&mut r, "rest_disable", stack.wait_open_at("1010").await) {
        Some(o) => o,
        None => return r,
    };
    let rest_id = rest_order.order_id.clone();
    if require(
        &mut r,
        "disable_rest",
        grpc.toggle_level(&rest, false).await,
    )
    .is_none()
    {
        return r;
    }
    r.assert_eq(
        "disable_rest_clears_owned",
        "gone",
        if wait_gone(&stack, &rest_id).await {
            "gone"
        } else {
            "still_working"
        },
    );

    let partial = stack.instance_id("wf_partial");
    if require(
        &mut r,
        "toggle_partial",
        grpc.toggle_level(&partial, true).await,
    )
    .is_none()
    {
        return r;
    }
    let part_order = match require(&mut r, "rest_partial", stack.wait_open_at("1020").await) {
        Some(o) => o,
        None => return r,
    };
    let part_id = part_order.order_id.clone();
    if require(
        &mut r,
        "lab_fill_partial",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": part_id}),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    wait_spot_qty(&stack, &mut r, "after_partial", d("0.001"), false).await;
    if require(&mut r, "delete_partial", grpc.delete_level(&partial).await).is_none() {
        return r;
    }
    r.assert_eq(
        "delete_clears_owned",
        "gone",
        if wait_gone(&stack, &part_id).await {
            "gone"
        } else {
            "still_working"
        },
    );
    let cat = match require(&mut r, "catalog_after_delete", stack.catalog()) {
        Some(c) => c,
        None => return r,
    };
    let tombs = cat
        .get("tombstones")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    let has = tombs
        .iter()
        .any(|t| t.as_str().unwrap_or("").contains("wf_partial"));
    r.assert_eq("catalog_tombstone", "true", has.to_string());

    drop(stack);
    let stack2 = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch_two_rest", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc2", stack2.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable2", grpc.enable().await).is_none() {
        return r;
    }
    let a = stack2.instance_id("wf_rest_a");
    let b = stack2.instance_id("wf_rest_b");
    if require(&mut r, "toggle_a", grpc.toggle_level(&a, true).await).is_none() {
        return r;
    }
    let oa = match require(&mut r, "rest_a", stack2.wait_open_at("1000").await) {
        Some(o) => o,
        None => return r,
    };
    if require(
        &mut r,
        "suppress_ack",
        stack2
            .lab_post("test/suppress_place_ack", serde_json::json!({}))
            .await,
    )
    .is_none()
    {
        return r;
    }
    if require(&mut r, "toggle_b", grpc.toggle_level(&b, true).await).is_none() {
        return r;
    }
    let ob = match require(&mut r, "rest_b", stack2.wait_open_at("1010").await) {
        Some(o) => o,
        None => return r,
    };
    r.assert_eq(
        "two_level_distinct_ids",
        "true",
        (oa.order_id != ob.order_id).to_string(),
    );
    if require(
        &mut r,
        "emit_wrong_ack",
        stack2
            .lab_post(
                "test/emit_place_ack",
                serde_json::json!({
                    "order_id": ob.order_id,
                    "omit_client_order_id": true
                }),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let cat = match require(&mut r, "catalog_two_rest", stack2.catalog()) {
        Some(c) => c,
        None => return r,
    };
    r.assert_eq(
        "a_did_not_claim_b",
        "true",
        (!catalog_owns(&cat, &a, &ob.order_id)).to_string(),
    );
    require_truth(&mut r, &stack2, "two_rest").await;

    drop(stack2);
    let stack3 = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::AmendCap,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch_amend", "ok", &e.to_string());
            return r;
        }
    };
    let mut g = match require(&mut r, "grpc_amend", stack3.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable_amend", g.enable().await).is_none() {
        return r;
    }
    let id = stack3.instance_id("wf_amend");
    if require(&mut r, "toggle_amend", g.toggle_level(&id, true).await).is_none() {
        return r;
    }
    let o = match require(&mut r, "amend_open", stack3.wait_open_at("1010").await) {
        Some(o) => o,
        None => return r,
    };
    let before = o.order_id.clone();
    if require(
        &mut r,
        "update_price",
        g.update_level_price(&id, "1000").await,
    )
    .is_none()
    {
        return r;
    }
    let o2 = match require(&mut r, "amend_visible", stack3.wait_open_at("1000").await) {
        Some(o) => o,
        None => return r,
    };
    r.assert_eq("amend_keeps_id", before, o2.order_id);
    r
}

async fn paper_futures_levels(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_futures_levels", "PAPER", "Futures", "exchange");
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Futures,
        CatalogKind::FuturesSignoff,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }

    let buy = stack.instance_id("wf_fut_buy");
    if require(
        &mut r,
        "toggle_fut_buy",
        grpc.toggle_level(&buy, true).await,
    )
    .is_none()
    {
        return r;
    }
    let order = match require(&mut r, "fut_rest", stack.wait_open_at("10000").await) {
        Some(o) => o,
        None => return r,
    };
    if require(
        &mut r,
        "lab_fill_long",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": order.order_id}),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    wait_fut_qty(&stack, &mut r, "fut_long", d("0.001")).await;
    if r.verdict == Verdict::Fail {
        return r;
    }

    if require(
        &mut r,
        "disable_buy_before_adverse",
        grpc.toggle_level(&buy, false).await,
    )
    .is_none()
    {
        return r;
    }
    let adverse = stack.instance_id("wf_fut_adverse");
    if require(
        &mut r,
        "toggle_adverse",
        grpc.toggle_level(&adverse, true).await,
    )
    .is_none()
    {
        return r;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let adv_orders = match require(&mut r, "adverse_orders", stack.rest_orders().await) {
        Some(o) => o,
        None => return r,
    };
    let adverse_working = adv_orders.iter().any(|o| qty_close(o.price, d("11000")));
    r.assert_eq(
        "adverse_same_side_blocked",
        "false",
        adverse_working.to_string(),
    );
    r.note("venue/risk rule: same-side increase blocked while long (new exposure blocked)");
    let rest = match require(&mut r, "adverse_pos", stack.rest_positions().await) {
        Some(p) => rest_btc_position(&p),
        None => return r,
    };
    r.assert_eq(
        "adverse_does_not_change_filled_pos",
        qty_canon(d("0.001")),
        qty_canon(rest),
    );
    if require(
        &mut r,
        "disable_adverse",
        grpc.toggle_level(&adverse, false).await,
    )
    .is_none()
    {
        return r;
    }

    let sell = stack.instance_id("wf_fut_sell");
    let sell_res = grpc.toggle_level(&sell, true).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let after_sell = match require(
        &mut r,
        "orders_after_short_attempt",
        stack.rest_orders().await,
    ) {
        Some(o) => o,
        None => return r,
    };
    let sell_working = after_sell
        .iter()
        .any(|o| o.side.eq_ignore_ascii_case("sell"));
    r.assert_eq(
        "paper_one_way_no_naked_short_working",
        "false",
        sell_working.to_string(),
    );
    let pos = match require(
        &mut r,
        "pos_after_short_attempt",
        stack.rest_positions().await,
    ) {
        Some(p) => rest_btc_position(&p),
        None => return r,
    };
    r.assert_eq(
        "short_attempt_leaves_long",
        qty_canon(d("0.001")),
        qty_canon(pos),
    );
    r.note(format!(
        "venue rule: one-way linear rejects opening a short; toggle_sell={sell_res:?}"
    ));
    let _ = grpc.toggle_level(&sell, false).await;

    let flip = stack.instance_id("wf_fut_flip");
    let flip_res = grpc.toggle_level(&flip, true).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let pos = match require(
        &mut r,
        "pos_after_flip_attempt",
        stack.rest_positions().await,
    ) {
        Some(p) => rest_btc_position(&p),
        None => return r,
    };
    r.assert_true(
        "flip_does_not_open_short",
        pos >= Decimal::ZERO,
        "qty>=0",
        &qty_canon(pos),
    );
    r.note(format!(
        "venue rule: oversized sell would enlarge a short; toggle_flip={flip_res:?} qty={pos}"
    ));
    let _ = grpc.toggle_level(&flip, false).await;

    let working = match require(&mut r, "working_after_disable", stack.rest_orders().await) {
        Some(o) => working_ids(&o),
        None => return r,
    };
    r.assert_eq("fut_disable_no_working", "0", working.len().to_string());
    r.assert_eq(
        "fut_disable_does_not_flatten",
        "true",
        qty_close(pos, d("0.001")).to_string(),
    );

    let flatten_msg = match require(
        &mut r,
        "flatten_reduce_only",
        grpc.flatten_and_pause("wf-fut-flatten").await,
    ) {
        Some(m) => m,
        None => return r,
    };
    r.note(format!("flatten {flatten_msg}"));
    wait_fut_qty(&stack, &mut r, "fut_reduce_only_flat", Decimal::ZERO).await;
    let snap = match require(&mut r, "snap_after_flatten", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq("flatten_latched", "true", snap.flatten_latched.to_string());
    r.assert_true(
        "journal_operator_flatten",
        stack.journal_text().contains("operator_flatten"),
        "operator_flatten journaled",
        "missing",
    );
    r
}

async fn paper_two_creates_one_resume(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_two_creates_one_resume", "PAPER", "Spot", "exchange");
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }
    if require(&mut r, "one_resume", grpc.resume_entries().await).is_none() {
        return r;
    }
    let snap = match require(&mut r, "snap_after_resume", grpc.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq(
        "entries_paused_after_resume",
        "false",
        snap.entries_paused.to_string(),
    );

    let id = stack.instance_id("wf_rest_a");
    let first = match require(
        &mut r,
        "first_journal",
        enable_and_wait_journal(&stack, &mut grpc, &id, "1000").await,
    ) {
        Some(v) => v,
        None => return r,
    };
    if require(&mut r, "disable_first", grpc.toggle_level(&id, false).await).is_none() {
        return r;
    }
    r.assert_eq(
        "first_rest_gone",
        "gone",
        if wait_gone(&stack, &first.1).await {
            "gone"
        } else {
            "still_working"
        },
    );
    let snap = match require(&mut r, "snap_before_second", grpc.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq(
        "entries_paused_before_second_create",
        "false",
        snap.entries_paused.to_string(),
    );
    if snap.entries_paused {
        r.fail(
            "no_cancel_working_between_creates",
            "entries_paused=false",
            "true",
        );
        return r;
    }

    let second = match require(
        &mut r,
        "second_journal",
        enable_and_wait_journal(&stack, &mut grpc, &id, "1030").await,
    ) {
        Some(v) => v,
        None => return r,
    };
    r.assert_true(
        "two_distinct_journal_ids",
        first.0 != second.0 && !first.0.is_empty() && !second.0.is_empty(),
        "two client ids",
        &format!("{} / {}", first.0, second.0),
    );
    let _ = grpc.toggle_level(&id, false).await;
    require_truth(&mut r, &stack, "two_creates").await;
    r
}

/// Demo cleanup failed because FlattenAndPause skipped the journaled reduce-only
/// close when TSM still held a cancelled entry as working. Reproduce the sequence
/// on Paper: disable+REST-gone without CancelWorking, fill, then FlattenAndPause
/// must journal `operator_flatten` and rest-flat.
async fn paper_flatten_after_stale_entry(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new(
        "paper_flatten_after_stale_entry",
        "PAPER",
        "Futures",
        "exchange",
    );
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Futures,
        CatalogKind::FuturesSignoff,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }
    if require(&mut r, "one_resume", grpc.resume_entries().await).is_none() {
        return r;
    }

    let id = stack.instance_id("wf_fut_buy");
    let first = match require(
        &mut r,
        "first_journal",
        enable_and_wait_journal(&stack, &mut grpc, &id, "10000").await,
    ) {
        Some(v) => v,
        None => return r,
    };
    if require(&mut r, "disable_first", grpc.toggle_level(&id, false).await).is_none() {
        return r;
    }
    r.assert_eq(
        "first_rest_gone",
        "gone",
        if wait_gone(&stack, &first.1).await {
            "gone"
        } else {
            "still_working"
        },
    );
    let snap = match require(&mut r, "snap_before_second", grpc.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq(
        "entries_paused_before_second_create",
        "false",
        snap.entries_paused.to_string(),
    );
    if snap.entries_paused {
        r.fail(
            "no_cancel_working_between_creates",
            "entries_paused=false",
            "true",
        );
        return r;
    }

    let second = match require(
        &mut r,
        "second_journal",
        enable_and_wait_journal(&stack, &mut grpc, &id, "10000").await,
    ) {
        Some(v) => v,
        None => return r,
    };
    r.assert_true(
        "two_distinct_journal_ids",
        first.0 != second.0 && !first.0.is_empty() && !second.0.is_empty(),
        "two client ids",
        &format!("{} / {}", first.0, second.0),
    );
    if require(
        &mut r,
        "lab_fill_second",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": second.1}),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    wait_fut_qty(&stack, &mut r, "filled_long", d("0.001")).await;
    if r.verdict == Verdict::Fail {
        return r;
    }

    let journal_before = stack.journal_text();
    r.assert_true(
        "no_operator_flatten_before_cleanup",
        !journal_before.contains("operator_flatten"),
        "no flatten yet",
        "operator_flatten already present",
    );

    let flatten_msg = match require(
        &mut r,
        "flatten_reduce_only",
        grpc.flatten_and_pause("wf-stale-entry-flatten").await,
    ) {
        Some(m) => m,
        None => return r,
    };
    r.note(format!("flatten {flatten_msg}"));
    r.assert_true(
        "flatten_sent_reduce_only",
        !flatten_msg.contains("0 reduce-only")
            && (flatten_msg.contains("reduce-only") || flatten_msg.to_lowercase().contains("flat")),
        "journaled reduce-only exit sent",
        &flatten_msg,
    );
    wait_fut_qty(&stack, &mut r, "flatten_rest_flat", Decimal::ZERO).await;
    let journal = stack.journal_text();
    r.assert_true(
        "journal_operator_flatten",
        journal.contains("operator_flatten"),
        "operator_flatten journaled",
        "missing",
    );
    r.assert_true(
        "journal_flatten_sell",
        journal.contains("operator_flatten") && journal.contains("Sell"),
        "flatten Sell journaled",
        "missing Sell",
    );
    let snap = match require(&mut r, "snap_after_flatten", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq("flatten_latched", "true", snap.flatten_latched.to_string());
    r
}

/// Last-based create 2 stays New when the live ask sits above that last.
/// One enabled price amend of the same owned order must keep client/exchange ids.
async fn paper_fill_ask_above_last(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_fill_ask_above_last", "PAPER", "Futures", "exchange");
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Futures,
        CatalogKind::FuturesSignoff,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }
    if require(&mut r, "one_resume", grpc.resume_entries().await).is_none() {
        return r;
    }

    let id = stack.instance_id("wf_fut_buy");
    let earlier_last = d("10000");
    let qty = d("0.001");
    let first = match require(
        &mut r,
        "first_journal",
        enable_and_wait_journal(&stack, &mut grpc, &id, "10000").await,
    ) {
        Some(v) => v,
        None => return r,
    };
    if require(&mut r, "disable_first", grpc.toggle_level(&id, false).await).is_none() {
        return r;
    }
    r.assert_eq(
        "first_rest_gone",
        "gone",
        if wait_gone(&stack, &first.1).await {
            "gone"
        } else {
            "still_working"
        },
    );

    let second = match require(
        &mut r,
        "second_journal",
        enable_and_wait_journal(&stack, &mut grpc, &id, "10000").await,
    ) {
        Some(v) => v,
        None => return r,
    };
    r.assert_true(
        "two_distinct_journal_ids",
        first.0 != second.0 && !first.0.is_empty() && !second.0.is_empty(),
        "two client ids",
        &format!("{} / {}", first.0, second.0),
    );

    let asks = [(d("10100"), d("0.002"))];
    let walk = match demo_levels::limit_buy_from_asks(&asks, qty, d("0.1"), d("150")) {
        Ok(px) => px,
        Err(e) => {
            r.fail("walk_asks", "ask above last within $150", &e);
            return r;
        }
    };
    r.assert_true(
        "walk_ask_above_last",
        walk > earlier_last,
        "ask > earlier last",
        &format!("last={earlier_last} walk={walk}"),
    );
    r.assert_true(
        "walk_notional_le_max",
        qty * walk <= d("150"),
        "qty×limit ≤ 150",
        &(qty * walk).to_string(),
    );

    let ids_before = distinct_level_client_order_ids(&stack.journal_text(), &id);
    if require(
        &mut r,
        "amend_to_ask",
        grpc.update_level_price(&id, &walk.normalize().to_string())
            .await,
    )
    .is_none()
    {
        return r;
    }

    let amended = match require(
        &mut r,
        "amend_visible",
        wait_same_order_amended(&stack, &second.1, walk).await,
    ) {
        Some(o) => o,
        None => return r,
    };
    r.assert_eq("amend_keeps_exchange_id", &second.1, &amended.order_id);
    if !amended.client_order_id.is_empty() {
        r.assert_eq(
            "amend_keeps_rest_client_id",
            &second.0,
            &amended.client_order_id,
        );
    }
    r.assert_eq(
        "amend_keeps_latest_journal_id",
        &second.0,
        &latest_level_client_order_id(&stack.journal_text(), &id).unwrap_or_default(),
    );
    let ids_after = distinct_level_client_order_ids(&stack.journal_text(), &id);
    r.assert_eq(
        "no_third_journal_id",
        &ids_before.len().to_string(),
        &ids_after.len().to_string(),
    );
    r.assert_eq("two_creates_only", "2", &ids_after.len().to_string());
    let _ = grpc.toggle_level(&id, false).await;
    require_truth(&mut r, &stack, "ask_above_last").await;
    r
}

async fn wait_same_order_amended(
    stack: &PaperStack,
    order_id: &str,
    price: Decimal,
) -> Result<crate::venue::RestOrder, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        let orders = stack.rest_orders().await.map_err(|e| e.to_string())?;
        if let Some(o) = orders
            .iter()
            .find(|o| o.order_id == order_id && qty_close(o.price, price))
        {
            return Ok(o.clone());
        }
        last = format!(
            "open={} want order={} price={}",
            orders.len(),
            order_id,
            price
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(format!("owned order not visible at amended price: {last}"))
}

async fn enable_and_wait_journal(
    stack: &PaperStack,
    grpc: &mut crate::grpc::GrpcCtl,
    instance_id: &str,
    price: &str,
) -> Result<(String, String), String> {
    grpc.update_level_price(instance_id, price)
        .await
        .map_err(|e| e.to_string())?;
    let watermark = latest_level_client_order_id(&stack.journal_text(), instance_id);
    grpc.toggle_level(instance_id, true)
        .await
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut cid = String::new();
    while std::time::Instant::now() < deadline {
        if let Some(found) =
            journaled_client_id_after(&stack.journal_text(), instance_id, watermark.as_deref())
        {
            cid = found;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if cid.is_empty() {
        return Err(format!(
            "no new journaled client_order_id for {instance_id}"
        ));
    }
    let order = stack.wait_open_at(price).await.map_err(|e| e.to_string())?;
    Ok((cid, order.order_id))
}

async fn paper_shadow(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_shadow_zero_mutations", "PAPER", "Spot", "shadow");
    let mut stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::Shadow,
        "shadow",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }
    let id = stack.instance_id("wf_sh");
    if require(&mut r, "toggle_shadow", grpc.toggle_level(&id, true).await).is_none() {
        return r;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let before = match require(&mut r, "rest_before", stack.rest_orders().await) {
        Some(o) => o,
        None => return r,
    };
    r.assert_eq(
        "shadow_no_orders_before_restart",
        "0",
        working_ids(&before).len().to_string(),
    );
    if require(&mut r, "restart", stack.restart_runner().await).is_none() {
        return r;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = match require(&mut r, "rest_after", stack.rest_orders().await) {
        Some(o) => o,
        None => return r,
    };
    r.assert_eq(
        "shadow_no_orders_after_restart",
        "0",
        working_ids(&after).len().to_string(),
    );
    r
}

async fn paper_lost_ack(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_lost_ack_restart", "PAPER", "Spot", "exchange");
    let mut stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }
    if require(
        &mut r,
        "suppress_ack",
        stack
            .lab_post("test/suppress_place_ack", serde_json::json!({}))
            .await,
    )
    .is_none()
    {
        return r;
    }
    let a = stack.instance_id("wf_rest_a");
    if require(&mut r, "toggle_a", grpc.toggle_level(&a, true).await).is_none() {
        return r;
    }
    let placed = match require(
        &mut r,
        "lost_ack_rest_order",
        stack.wait_open_at("1000").await,
    ) {
        Some(o) => o,
        None => return r,
    };
    let placed_id = placed.order_id.clone();
    let journal = stack.journal_text();
    r.assert_true(
        "journal_has_submission",
        journal_has_state(&journal, "prepared") || journal_has_state(&journal, "submitted"),
        "prepared|submitted",
        if journal.is_empty() { "empty" } else { "other" },
    );
    if require(&mut r, "restart", stack.restart_runner().await).is_none() {
        return r;
    }
    let after = match require(&mut r, "rest_after_restart", stack.rest_orders().await) {
        Some(o) => o,
        None => return r,
    };
    let ids = working_ids(&after);
    r.assert_eq(
        "recovered_same_order_id",
        &placed_id,
        ids.first().cloned().unwrap_or_default(),
    );
    r.assert_eq("no_replacement_order", "1", ids.len().to_string());
    let cat = match require(&mut r, "catalog_after_restart", stack.catalog()) {
        Some(c) => c,
        None => return r,
    };
    r.assert_true(
        "catalog_owns_recovered",
        catalog_owns(&cat, &a, &placed_id),
        "owned",
        "not owned",
    );
    let snap = match require(&mut r, "grpc_after_restart", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    let runner_has = snap.orders.iter().any(|(id, _, _)| id == &placed_id);
    r.assert_eq("runner_owns_recovered", "true", runner_has.to_string());
    require_truth(&mut r, &stack, "lost_ack").await;
    r
}

async fn paper_c2_exchange(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_c2_exchange", "PAPER", "Futures", "exchange");
    let mut stack = match PaperStack::launch_c2(artifacts).await {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch_c2", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable_global", grpc.enable().await).is_none() {
        return r;
    }
    if require(
        &mut r,
        "enable_strategy",
        grpc.toggle_strategy("BTCPriceMALongFlatV1", "BTC/USDT", true)
            .await,
    )
    .is_none()
    {
        return r;
    }
    if require(&mut r, "resume", grpc.resume_entries().await).is_none() {
        return r;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut entry: Option<crate::venue::RestOrder> = None;
    while std::time::Instant::now() < deadline {
        match stack.rest_orders().await {
            Ok(orders) => {
                if let Some(o) = working_ids(&orders)
                    .first()
                    .and_then(|id| orders.iter().find(|o| &o.order_id == id))
                    .cloned()
                {
                    entry = Some(o);
                    break;
                }
            }
            Err(e) => {
                r.fail("c2_rest_orders", "ok", &e.to_string());
                return r;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let entry = match entry {
        Some(o) => o,
        None => {
            r.fail("c2_entry", "working Paper order", "timeout");
            return r;
        }
    };
    let journal_before_fill = stack.journal_text();
    r.assert_true(
        "journal_before_send_or_ack",
        journal_has_state(&journal_before_fill, "prepared")
            || journal_has_state(&journal_before_fill, "submitted"),
        "prepared|submitted before fill",
        if journal_before_fill.is_empty() {
            "empty"
        } else {
            "other"
        },
    );
    if require(
        &mut r,
        "lab_fill_c2",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": entry.order_id}),
            )
            .await,
    )
    .is_none()
    {
        return r;
    }
    wait_fut_qty(&stack, &mut r, "c2_fill", d("0.001")).await;
    let ledger_qty = unique_ledger_qty(&stack.ledger_text());
    r.assert_eq(
        "c2_unique_ledger",
        qty_canon(d("0.001")),
        qty_canon(ledger_qty),
    );

    if require(&mut r, "restart", stack.restart_runner().await).is_none() {
        return r;
    }
    wait_fut_qty(&stack, &mut r, "c2_after_restart", d("0.001")).await;
    let after = match require(&mut r, "rest_after_c2_restart", stack.rest_orders().await) {
        Some(o) => working_ids(&o),
        None => return r,
    };
    r.assert_true(
        "c2_no_replacement_after_restart",
        after.len() <= 1,
        "<=1 working",
        &after.len().to_string(),
    );

    grpc = match require(&mut r, "grpc_after_c2_restart", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "c2_pause", grpc.pause_entries().await).is_none() {
        return r;
    }
    if require(
        &mut r,
        "c2_flatten",
        grpc.flatten_and_pause("wf-c2-flatten").await,
    )
    .is_none()
    {
        return r;
    }
    wait_fut_qty(&stack, &mut r, "c2_flat", Decimal::ZERO).await;
    let snap = match require(&mut r, "c2_snap_flat", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq(
        "c2_flatten_latched",
        "true",
        snap.flatten_latched.to_string(),
    );
    r.assert_eq("c2_entries_paused", "true", snap.entries_paused.to_string());
    require_truth(&mut r, &stack, "c2").await;
    r
}

async fn paper_control(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_control_plane", "PAPER", "Spot", "exchange");
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    let snap = match require(&mut r, "status", grpc.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq("venue", "PAPER", snap.venue.to_uppercase());
    r.assert_eq("environment", "LOCAL", snap.environment.to_uppercase());
    r.assert_eq(
        "execution_mode",
        "exchange",
        snap.execution_mode.to_lowercase(),
    );
    if require(&mut r, "paper_enable_without_token", grpc.enable().await).is_none() {
        return r;
    }
    if require(&mut r, "pause", grpc.pause_entries().await).is_none() {
        return r;
    }
    let paused = match require(&mut r, "status_after_pause", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq("entries_paused", "true", paused.entries_paused.to_string());
    let orders = match require(&mut r, "rest_after_pause", stack.rest_orders().await) {
        Some(o) => o,
        None => return r,
    };
    r.note(format!(
        "REST working after pause {} (RPC success is not flat proof)",
        working_ids(&orders).len()
    ));
    require_truth(&mut r, &stack, "control").await;
    r
}

async fn paper_stale_rest(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_stale_and_rest_failure", "PAPER", "Spot", "exchange");
    let mut stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => return r,
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        return r;
    }
    if require(
        &mut r,
        "stale_book",
        stack
            .lab_post("test/stale_orderbook", serde_json::json!({"age_secs": 600}))
            .await,
    )
    .is_none()
    {
        return r;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let a = stack.instance_id("wf_rest_a");
    if require(&mut r, "toggle_stale", grpc.toggle_level(&a, true).await).is_none() {
        return r;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let orders = match require(&mut r, "rest_during_stale", stack.rest_orders().await) {
        Some(o) => o,
        None => return r,
    };
    r.assert_eq(
        "stale_book_blocks_or_hold",
        "true",
        working_ids(&orders).is_empty().to_string(),
    );
    if require(&mut r, "disable_stale", grpc.toggle_level(&a, false).await).is_none() {
        return r;
    }
    if require(
        &mut r,
        "reset_lab",
        stack
            .lab_post("test/reset_lab_faults", serde_json::json!({}))
            .await,
    )
    .is_none()
    {
        return r;
    }
    if require(
        &mut r,
        "toggle_after_reset",
        grpc.toggle_level(&a, true).await,
    )
    .is_none()
    {
        return r;
    }
    if require(
        &mut r,
        "rest_after_stale_reset",
        stack.wait_open_at("1000").await,
    )
    .is_none()
    {
        return r;
    }
    if require(
        &mut r,
        "fail_open_orders",
        stack
            .lab_post("test/fail_open_orders", serde_json::json!({}))
            .await,
    )
    .is_none()
    {
        return r;
    }
    if require(
        &mut r,
        "restart_after_rest_fail",
        stack.restart_runner().await,
    )
    .is_none()
    {
        return r;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let s = match require(&mut r, "snapshot_after_rest_fail", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_true(
        "rest_failure_reason",
        s.last_block_reason_code == "REST_LOOKUP_FAILED"
            || s.exposure_halt_reason.to_lowercase().contains("rest"),
        "REST_LOOKUP_FAILED or halt",
        &format!("{} / {}", s.last_block_reason_code, s.exposure_halt_reason),
    );
    require_truth(&mut r, &stack, "stale_rest").await;
    r
}

async fn negative_control(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("negative_control_mismatch", "PAPER", "Futures", "exchange");
    r.expected_failure = true;
    let stack = match PaperStack::launch_fast_recon(
        artifacts,
        PaperInstrument::Futures,
        CatalogKind::FuturesSignoff,
        "exchange",
        1_000,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            r.expected_failure = false;
            return r;
        }
    };
    let mut grpc = match require(&mut r, "grpc", stack.grpc().await) {
        Some(g) => g,
        None => {
            r.expected_failure = false;
            return r;
        }
    };
    if require(&mut r, "enable", grpc.enable().await).is_none() {
        r.expected_failure = false;
        return r;
    }
    let buy = stack.instance_id("wf_fut_buy");
    if require(&mut r, "toggle_buy", grpc.toggle_level(&buy, true).await).is_none() {
        r.expected_failure = false;
        return r;
    }
    let order = match require(&mut r, "rest", stack.wait_open_at("10000").await) {
        Some(o) => o,
        None => {
            r.expected_failure = false;
            return r;
        }
    };
    if require(
        &mut r,
        "lab_fill",
        stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": order.order_id}),
            )
            .await,
    )
    .is_none()
    {
        r.expected_failure = false;
        return r;
    }
    wait_fut_qty(&stack, &mut r, "before_poison", d("0.001")).await;
    if r.verdict == Verdict::Fail {
        r.expected_failure = false;
        return r;
    }
    let rest_before = match require(&mut r, "rest_before_poison", stack.rest_positions().await) {
        Some(p) => rest_btc_position(&p),
        None => {
            r.expected_failure = false;
            return r;
        }
    };
    if require(
        &mut r,
        "emit_ws_position",
        stack
            .lab_post(
                "test/emit_ws_position",
                serde_json::json!({"quantity":"1","entry_price":"50000"}),
            )
            .await,
    )
    .is_none()
    {
        r.expected_failure = false;
        return r;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut runner = Decimal::ZERO;
    let mut rest_qty = rest_before;
    let mut snap = empty_snap();
    let mut last_grpc: Option<String> = None;
    while std::time::Instant::now() < deadline {
        let positions = match stack.rest_positions().await {
            Ok(p) => p,
            Err(e) => {
                r.fail("poisoned_rest_positions", "ok", &e.to_string());
                r.expected_failure = false;
                return r;
            }
        };
        rest_qty = rest_btc_position(&positions);
        snap = match stack.snapshot().await {
            Ok(s) => s,
            Err(e) => {
                last_grpc = Some(e.to_string());
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        runner = runner_btc_qty(&snap);
        if halt_is_disagreement(&snap) || !qty_close(rest_qty, runner) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    require_truth(&mut r, &stack, "after_poison").await;
    r.assert_eq(
        "independent_rest_unpoisoned",
        qty_canon(d("0.001")),
        qty_canon(rest_qty),
    );
    if r.verdict == Verdict::Fail {
        r.expected_failure = false;
        return r;
    }
    if !halt_is_disagreement(&snap) {
        r.expected_failure = false;
        r.fail(
            "runner_halt_disagreement",
            "RECONCILE_DISAGREEMENT",
            &format!(
                "halt={} code={} rest={} runner={} grpc={:?}",
                snap.exposure_halt_reason, snap.last_block_reason_code, rest_qty, runner, last_grpc
            ),
        );
        return r;
    }
    r.assert_eq(
        "runner_halt_disagreement",
        "RECONCILE_DISAGREEMENT",
        &snap.last_block_reason_code,
    );

    r.note("independent REST stayed 0.001; runner ingested poisoned WS qty and recorded RECONCILE_DISAGREEMENT");
    r.assert_eq(
        "poisoned_rest.paper",
        qty_canon(d("0.001")),
        qty_canon(rest_qty),
    );
    r.assert_eq(
        "poisoned_rest.runner",
        qty_canon(d("0.001")),
        qty_canon(runner),
    );
    if r.verdict != Verdict::Fail {
        r.fail(
            "expected_mismatch_detection",
            "FAIL",
            &r.verdict.to_string(),
        );
        r.expected_failure = false;
    }
    r
}

async fn paper_ui_envoy(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("paper_ui_envoy", "PAPER", "Spot", "exchange");
    let ui = ui_dir();
    if !ui.join("package.json").exists() {
        r.fail("ui_checkout", "trading-system-ui present", "missing");
        return r;
    }
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::TwoRest,
        "exchange",
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            r.fail("launch", "ok", &e.to_string());
            return r;
        }
    };
    let s = match require(&mut r, "grpc_status", stack.snapshot().await) {
        Some(s) => s,
        None => return r,
    };
    r.assert_eq("grpc_venue", "PAPER", s.venue.to_uppercase());
    r.assert_eq("grpc_environment", "LOCAL", s.environment.to_uppercase());
    let envoy_port = match require(&mut r, "envoy_port", crate::process::allocate_port()) {
        Some(p) => p,
        None => return r,
    };
    let admin = match require(&mut r, "admin_port", crate::process::allocate_port()) {
        Some(p) => p,
        None => return r,
    };
    let grpc_port = stack.grpc_url.rsplit(':').next().unwrap_or("50051");
    let yaml = format!(
        include_str!("envoy_stub.yaml"),
        admin = admin,
        envoy = envoy_port,
        grpc = grpc_port
    );
    let yaml_path = stack.run_dir.join("envoy.yaml");
    if require(&mut r, "write_envoy_yaml", std::fs::write(&yaml_path, yaml)).is_none() {
        return r;
    }
    let name = format!("wf-envoy-{}", &stack.run_id.replace("wf-", ""));
    let docker = std::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "-d",
            "--network",
            "host",
            "-v",
            &format!("{}:/etc/envoy/envoy.yaml:ro", yaml_path.display()),
            "--name",
            &name,
            "envoyproxy/envoy:v1.31-latest",
        ])
        .output();
    match docker {
        Ok(o) if o.status.success() => {
            let cid = String::from_utf8_lossy(&o.stdout).trim().to_string();
            r.note(format!("envoy {cid}"));
            let ui_port = match require(&mut r, "ui_port", crate::process::allocate_port()) {
                Some(p) => p,
                None => {
                    let _ = std::process::Command::new("docker")
                        .args(["rm", "-f", &name])
                        .status();
                    return r;
                }
            };
            let vite_bin = ui.join("node_modules/.bin/vite");
            let mut vite_cmd = std::process::Command::new(&vite_bin);
            vite_cmd
                .args([
                    "--host",
                    "127.0.0.1",
                    "--port",
                    &ui_port.to_string(),
                    "--strictPort",
                ])
                .env("VITE_WORKFLOW_SKIP_AUTH", "1")
                .env("GRPC_WEB_PROXY", format!("http://127.0.0.1:{envoy_port}"))
                .current_dir(&ui);
            match crate::process::ChildProc::spawn(
                "vite",
                vite_cmd,
                &stack.run_dir.join("vite.log"),
            ) {
                Ok(mut vite) => {
                    let ui_url = format!("http://127.0.0.1:{ui_port}");
                    if crate::process::wait_http(&ui_url, Duration::from_secs(45))
                        .await
                        .is_err()
                    {
                        r.fail("vite", "listening", "timeout");
                    } else {
                        let pw_cli = ui.join("node_modules/@playwright/test/cli.js");
                        let mut pw_cmd = std::process::Command::new("node");
                        pw_cmd
                            .arg(&pw_cli)
                            .args(["test", "e2e/paper-control.spec.ts"])
                            .env("WORKFLOW_UI_BASE", &ui_url)
                            .env("WORKFLOW_ENVOY_NAME", &name)
                            .env("VITE_WORKFLOW_SKIP_AUTH", "1")
                            .current_dir(&ui);
                        if let Ok(browsers) = std::env::var("PLAYWRIGHT_BROWSERS_PATH") {
                            pw_cmd.env("PLAYWRIGHT_BROWSERS_PATH", browsers);
                        }
                        let pw = pw_cmd.output();
                        match pw {
                            Ok(out) if out.status.success() => {
                                r.assert_eq("playwright_paper_control", "PASS", "PASS");
                            }
                            Ok(out) => r.fail(
                                "playwright",
                                "PASS",
                                &format!(
                                    "{}\n{}",
                                    String::from_utf8_lossy(&out.stdout),
                                    String::from_utf8_lossy(&out.stderr)
                                ),
                            ),
                            Err(e) => r.fail("playwright_spawn", "ok", &e.to_string()),
                        }
                    }
                    vite.terminate();
                }
                Err(e) => r.fail("vite_spawn", "ok", &e.to_string()),
            }
            if require(
                &mut r,
                "docker_rm_envoy",
                std::process::Command::new("docker")
                    .args(["rm", "-f", &name])
                    .status(),
            )
            .is_none()
            {
                return r;
            }
        }
        Ok(o) => r.fail(
            "docker_envoy",
            "envoy container",
            &String::from_utf8_lossy(&o.stderr),
        ),
        Err(e) => r.fail("docker", "available for Envoy", &e.to_string()),
    }
    r.note(format!("UI dir {}", ui.display()));
    r
}
