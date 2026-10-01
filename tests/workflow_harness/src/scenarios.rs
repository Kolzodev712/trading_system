use rust_decimal::Decimal;
use std::path::Path;
use std::time::Duration;

use crate::demo::{mutating_refused, DemoAdapter};
use crate::oracle::{
    assert_spot_agreement, btc_total, catalog_filled_qty, qty_canon, qty_close, rest_btc_position,
    runner_btc_qty, unique_ledger_qty, working_ids,
};
use crate::paper::{CatalogKind, PaperInstrument, PaperStack};
use crate::process::ui_dir;
use crate::types::{Layer, ScenarioResult, Verdict};
use crate::venue::RunnerSnapshot;

fn d(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap_or(Decimal::ZERO)
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
        let balances = stack.rest_balances().await.unwrap_or_default();
        let snap = stack.snapshot().await.ok();
        let paper = btc_total(&balances);
        let runner = snap.as_ref().map(runner_btc_qty).unwrap_or_default();
        let halt = snap
            .as_ref()
            .map(|s| s.exposure_halt_reason.clone())
            .unwrap_or_default();
        let cat = if check_catalog {
            stack.catalog().ok().map(|c| catalog_filled_qty(&c))
        } else {
            None
        };
        let led = Some(unique_ledger_qty(&stack.ledger_text()));
        last = format!("paper={paper} runner={runner}");
        if qty_close(paper, expect) && qty_close(runner, expect) {
            assert_spot_agreement(result, tag, expect, paper, runner, cat, led, &halt);
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    result.fail(tag, &expect.to_string(), &last);
}

pub async fn run_layer(layer: Layer, only: Option<&str>, artifacts: &Path) -> Vec<ScenarioResult> {
    let mut out = Vec::new();
    match layer {
        Layer::Paper => {
            let all = [
                "paper_spot_levels",
                "paper_futures_levels",
                "paper_shadow_zero_mutations",
                "paper_lost_ack_restart",
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
            Ok(d) => out.push(d.readonly_checks().await),
            Err(e) => {
                let mut r = ScenarioResult::new("demo_readonly_paused", "BYBIT", "Spot", "paused");
                r.fail("adapter", "demo adapter", &e.to_string());
                out.push(r);
            }
        },
        Layer::DemoMutating => out.push(mutating_refused()),
    }
    out
}

async fn run_named(name: &str, artifacts: &Path) -> ScenarioResult {
    match name {
        "paper_spot_levels" => paper_spot_levels(artifacts).await,
        "paper_futures_levels" => paper_futures_levels(artifacts).await,
        "paper_shadow_zero_mutations" => paper_shadow(artifacts).await,
        "paper_lost_ack_restart" => paper_lost_ack(artifacts).await,
        "paper_control_plane" => paper_control(artifacts).await,
        "paper_stale_and_rest_failure" => paper_stale_rest(artifacts).await,
        "negative_control_mismatch" => negative_control(artifacts).await,
        "frozen_parity_component" => {
            let mut r = ScenarioResult::new("frozen_parity_component", "PAPER", "n/a", "component");
            r.note("parity fixtures run in just test-fast; not a process launch");
            r.verdict = Verdict::NotRun;
            r
        }
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "connected", &e.to_string());
            return r;
        }
    };
    if let Err(e) = grpc.enable().await {
        r.fail("enable", "ok", &e.to_string());
        return r;
    }

    let buy = stack.instance_id("wf_buy");
    if grpc.toggle_level(&buy, true).await.is_err() {
        r.fail("toggle_buy", "ok", "toggle failed");
        return r;
    }
    let order = match stack.wait_open_at("10000").await {
        Ok(o) => o,
        Err(e) => {
            r.fail("rest_buy", "working order @10000", &e.to_string());
            return r;
        }
    };
    r.note(format!("buy order {}", order.order_id));
    let _ = stack
        .lab_post(
            "test/lab_fill_open",
            serde_json::json!({"qty":"0.001","order_id": order.order_id}),
        )
        .await;
    wait_spot_qty(&stack, &mut r, "after_buy", d("0.001"), true).await;

    if let Err(e) = stack.restart_runner().await {
        r.fail("restart", "runner restarted, paper alive", &e.to_string());
        return r;
    }
    grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc_after_restart", "connected", &e.to_string());
            return r;
        }
    };
    wait_spot_qty(&stack, &mut r, "after_restart", d("0.001"), false).await;
    let snap = stack.snapshot().await.unwrap_or_else(|_| empty_snap());
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

    let _ = grpc.enable().await;
    let sell = stack.instance_id("wf_sell");
    let _ = grpc.toggle_level(&sell, true).await;
    match stack.wait_open_at("100000").await {
        Ok(o) => {
            let _ = stack
                .lab_post(
                    "test/lab_fill_open",
                    serde_json::json!({"qty":"0.001","order_id": o.order_id}),
                )
                .await;
        }
        Err(e) => r.fail("rest_sell", "working sell", &e.to_string()),
    }
    wait_spot_qty(&stack, &mut r, "after_sell", Decimal::ZERO, false).await;

    let rest = stack.instance_id("wf_rest");
    let _ = grpc.toggle_level(&rest, true).await;
    if let Ok(o) = stack.wait_open_at("1010").await {
        let id = o.order_id.clone();
        let _ = grpc.toggle_level(&rest, false).await;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut gone = false;
        while std::time::Instant::now() < deadline {
            let orders = stack.rest_orders().await.unwrap_or_default();
            if !working_ids(&orders).contains(&id) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        r.assert_eq(
            "disable_rest_clears_owned",
            "gone",
            if gone { "gone" } else { "still_working" },
        );
    } else {
        r.fail("rest_disable", "order @1010", "missing");
    }

    let partial = stack.instance_id("wf_partial");
    let _ = grpc.toggle_level(&partial, true).await;
    if let Ok(o) = stack.wait_open_at("1020").await {
        let id = o.order_id.clone();
        let _ = stack
            .lab_post(
                "test/lab_fill_open",
                serde_json::json!({"qty":"0.001","order_id": id}),
            )
            .await;
        wait_spot_qty(&stack, &mut r, "after_partial", d("0.001"), false).await;
        let _ = grpc.delete_level(&partial).await;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut gone = false;
        while std::time::Instant::now() < deadline {
            let orders = stack.rest_orders().await.unwrap_or_default();
            if working_ids(&orders).iter().all(|x| x != &id) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        r.assert_eq(
            "delete_clears_owned",
            "gone",
            if gone { "gone" } else { "still_working" },
        );
        if let Ok(cat) = stack.catalog() {
            let tombs = cat
                .get("tombstones")
                .and_then(|t| t.as_array())
                .cloned()
                .unwrap_or_default();
            let has = tombs
                .iter()
                .any(|t| t.as_str().unwrap_or("").contains("wf_partial"));
            r.assert_eq("catalog_tombstone", "true", has.to_string());
        }
    }

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
    let mut grpc = match stack2.grpc().await {
        Ok(mut g) => {
            let _ = g.enable().await;
            g
        }
        Err(e) => {
            r.fail("grpc2", "ok", &e.to_string());
            return r;
        }
    };
    let a = stack2.instance_id("wf_rest_a");
    let b = stack2.instance_id("wf_rest_b");
    let _ = grpc.toggle_level(&a, true).await;
    let oa = match stack2.wait_open_at("1000").await {
        Ok(o) => o,
        Err(e) => {
            r.fail("rest_a", "order", &e.to_string());
            return r;
        }
    };
    let _ = stack2
        .lab_post("test/suppress_place_ack", serde_json::json!({}))
        .await;
    let _ = grpc.toggle_level(&b, true).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    if let Ok(ob) = stack2.wait_open_at("1010").await {
        r.assert_eq(
            "two_level_distinct_ids",
            "true",
            (oa.order_id != ob.order_id).to_string(),
        );
        let _ = stack2
            .lab_post(
                "test/emit_place_ack",
                serde_json::json!({
                    "order_id": ob.order_id,
                    "omit_client_order_id": true
                }),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        if let Ok(s) = stack2.snapshot().await {
            r.note(format!(
                "after wrong ACK halt={} code={}",
                s.exposure_halt_reason, s.last_block_reason_code
            ));
        }
        if let Ok(cat) = stack2.catalog() {
            let inst = cat
                .get("instances")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let a_ids: Vec<String> = inst
                .iter()
                .find(|i| i.get("id").and_then(|x| x.as_str()) == Some(a.as_str()))
                .and_then(|i| i.get("owned_exchange_ids"))
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            r.assert_eq(
                "a_did_not_claim_b",
                "true",
                (!a_ids.contains(&ob.order_id)).to_string(),
            );
        }
    }

    drop(stack2);
    if let Ok(stack3) = PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::AmendCap,
        "exchange",
    )
    .await
    {
        if let Ok(mut g) = stack3.grpc().await {
            let _ = g.enable().await;
            let id = stack3.instance_id("wf_amend");
            let _ = g.toggle_level(&id, true).await;
            if let Ok(o) = stack3.wait_open_at("1010").await {
                let before = o.order_id.clone();
                if g.update_level_price(&id, "1000").await.is_ok() {
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    if let Ok(o2) = stack3.wait_open_at("1000").await {
                        r.assert_eq("amend_keeps_id", before, o2.order_id);
                    } else {
                        r.fail("amend_visible", "order @1000", "missing");
                    }
                }
            }
        }
    }
    r
}

fn empty_snap() -> RunnerSnapshot {
    RunnerSnapshot {
        venue: String::new(),
        environment: String::new(),
        execution_mode: String::new(),
        trading_enabled: false,
        positions: vec![],
        orders: vec![],
        levels: vec![],
        exposure_halt_reason: String::new(),
        last_block_reason_code: String::new(),
        avg_entry: String::new(),
    }
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "ok", &e.to_string());
            return r;
        }
    };
    let _ = grpc.enable().await;
    let id = stack.instance_id("wf_fut_buy");
    let _ = grpc.toggle_level(&id, true).await;
    let order = match stack.wait_open_at("10000").await {
        Ok(o) => o,
        Err(e) => {
            r.fail("fut_rest", "order", &e.to_string());
            return r;
        }
    };
    let _ = stack
        .lab_post(
            "test/lab_fill_open",
            serde_json::json!({"qty":"0.001","order_id": order.order_id}),
        )
        .await;
    let expect = d("0.001");
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut ok = false;
    while std::time::Instant::now() < deadline {
        let rest = rest_btc_position(&stack.rest_positions().await.unwrap_or_default());
        let runner = stack
            .snapshot()
            .await
            .ok()
            .map(|s| runner_btc_qty(&s))
            .unwrap_or_default();
        if qty_close(rest, expect) && qty_close(runner, expect) {
            r.assert_eq("fut_rest_qty", qty_canon(expect), qty_canon(rest));
            r.assert_eq("fut_runner_qty", qty_canon(expect), qty_canon(runner));
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !ok {
        r.fail("fut_agree", "0.001", "timeout");
    }
    let _ = grpc.toggle_level(&id, false).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let working = working_ids(&stack.rest_orders().await.unwrap_or_default());
    r.assert_eq("fut_disable_no_working", "0", working.len().to_string());
    let rest = rest_btc_position(&stack.rest_positions().await.unwrap_or_default());
    r.assert_eq(
        "fut_disable_does_not_flatten",
        "true",
        qty_close(rest, expect).to_string(),
    );
    r
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "ok", &e.to_string());
            return r;
        }
    };
    let _ = grpc.enable().await;
    let id = stack.instance_id("wf_sh");
    let _ = grpc.toggle_level(&id, true).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let before = stack.rest_orders().await.unwrap_or_default();
    r.assert_eq(
        "shadow_no_orders_before_restart",
        "0",
        working_ids(&before).len().to_string(),
    );
    if stack.restart_runner().await.is_err() {
        r.fail("restart", "ok", "failed");
        return r;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = stack.rest_orders().await.unwrap_or_default();
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "ok", &e.to_string());
            return r;
        }
    };
    let _ = grpc.enable().await;
    let _ = stack
        .lab_post("test/suppress_place_ack", serde_json::json!({}))
        .await;
    let a = stack.instance_id("wf_rest_a");
    let _ = grpc.toggle_level(&a, true).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let journal = stack.journal_text();
    r.assert_true(
        "journal_has_submission",
        !journal.is_empty(),
        "journal record",
        if journal.is_empty() {
            "empty"
        } else {
            "present"
        },
    );
    if stack.restart_runner().await.is_err() {
        r.fail("restart", "ok", "failed");
        return r;
    }
    if let Ok(s) = stack.snapshot().await {
        r.note(format!(
            "after lost-ack restart halt={} code={}",
            s.exposure_halt_reason, s.last_block_reason_code
        ));
    }
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "ok", &e.to_string());
            return r;
        }
    };
    let snap = match grpc.snapshot().await {
        Ok(s) => s,
        Err(e) => {
            r.fail("status", "ok", &e.to_string());
            return r;
        }
    };
    r.assert_eq("venue", "PAPER", snap.venue.to_uppercase());
    r.assert_eq("environment", "LOCAL", snap.environment.to_uppercase());
    r.assert_eq(
        "execution_mode",
        "exchange",
        snap.execution_mode.to_lowercase(),
    );
    if grpc.enable().await.is_ok() {
        r.assert_eq("paper_enable_without_token", "ok", "ok");
    }
    let _ = grpc.pause_entries().await;
    let orders = stack.rest_orders().await.unwrap_or_default();
    r.note(format!(
        "REST working after pause {} (RPC success is not flat proof)",
        working_ids(&orders).len()
    ));
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "ok", &e.to_string());
            return r;
        }
    };
    let _ = grpc.enable().await;
    let _ = stack
        .lab_post("test/stale_orderbook", serde_json::json!({"age_secs": 600}))
        .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let a = stack.instance_id("wf_rest_a");
    let _ = grpc.toggle_level(&a, true).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let orders = stack.rest_orders().await.unwrap_or_default();
    r.assert_eq(
        "stale_book_blocks_or_hold",
        "true",
        working_ids(&orders).is_empty().to_string(),
    );
    let _ = grpc.toggle_level(&a, false).await;
    let _ = stack
        .lab_post("test/reset_lab_faults", serde_json::json!({}))
        .await;
    let _ = grpc.toggle_level(&a, true).await;
    if let Err(e) = stack.wait_open_at("1000").await {
        r.fail(
            "rest_after_stale_reset",
            "working order @1000",
            &e.to_string(),
        );
        return r;
    }
    let _ = stack
        .lab_post("test/fail_open_orders", serde_json::json!({}))
        .await;
    if stack.restart_runner().await.is_err() {
        r.fail("restart_after_rest_fail", "ok", "restart failed");
        return r;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    if let Ok(s) = stack.snapshot().await {
        r.note(format!(
            "after REST fail halt={} code={}",
            s.exposure_halt_reason, s.last_block_reason_code
        ));
        r.assert_true(
            "rest_failure_reason",
            s.last_block_reason_code == "REST_LOOKUP_FAILED"
                || s.exposure_halt_reason.to_lowercase().contains("rest"),
            "REST_LOOKUP_FAILED or halt",
            &format!("{} / {}", s.last_block_reason_code, s.exposure_halt_reason),
        );
    } else {
        r.fail("snapshot_after_rest_fail", "ok", "grpc snapshot failed");
    }
    r
}

async fn negative_control(artifacts: &Path) -> ScenarioResult {
    let mut r = ScenarioResult::new("negative_control_mismatch", "PAPER", "Spot", "exchange");
    r.expected_failure = true;
    let stack = match PaperStack::launch(
        artifacts,
        PaperInstrument::Spot,
        CatalogKind::SpotSignoff,
        "exchange",
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
    let mut grpc = match stack.grpc().await {
        Ok(g) => g,
        Err(e) => {
            r.fail("grpc", "ok", &e.to_string());
            r.expected_failure = false;
            return r;
        }
    };
    let _ = grpc.enable().await;
    let buy = stack.instance_id("wf_buy");
    let _ = grpc.toggle_level(&buy, true).await;
    let order = match stack.wait_open_at("10000").await {
        Ok(o) => o,
        Err(e) => {
            r.fail("rest", "order", &e.to_string());
            r.expected_failure = false;
            return r;
        }
    };
    let _ = stack
        .lab_post(
            "test/lab_fill_open",
            serde_json::json!({"qty":"0.001","order_id": order.order_id}),
        )
        .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let runner = stack
        .snapshot()
        .await
        .ok()
        .map(|s| runner_btc_qty(&s))
        .unwrap_or_default();
    r.note("deliberately comparing runner qty to a poisoned REST quantity of 999");
    assert_spot_agreement(
        &mut r,
        "poisoned_rest",
        d("0.001"),
        d("999"),
        runner,
        None,
        None,
        "",
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
    if let Ok(s) = stack.snapshot().await {
        r.assert_eq("grpc_venue", "PAPER", s.venue.to_uppercase());
        r.assert_eq("grpc_environment", "LOCAL", s.environment.to_uppercase());
    }
    let envoy_port = crate::process::allocate_port().unwrap_or(18081);
    let admin = crate::process::allocate_port().unwrap_or(19901);
    let grpc_port = stack.grpc_url.rsplit(':').next().unwrap_or("50051");
    let yaml = format!(
        include_str!("envoy_stub.yaml"),
        admin = admin,
        envoy = envoy_port,
        grpc = grpc_port
    );
    let yaml_path = stack.run_dir.join("envoy.yaml");
    let _ = std::fs::write(&yaml_path, yaml);
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
            let ui_port = crate::process::allocate_port().unwrap_or(15173);
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
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", &name])
                .status();
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
