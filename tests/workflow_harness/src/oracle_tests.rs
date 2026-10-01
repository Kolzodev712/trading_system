//! Oracle unit tests (no processes).

use crate::demo_levels::{
    align_tick_up, cleanup_verification, expanded_instance_id, levels_disabled_proof,
    limit_buy_from_asks, next_cleanup_request_id, owned_order_from_open_or_history, parse_asks,
    resolve_paused_instance_id, rest_order_matches_owned, OwnedOrderSpec,
};
use crate::oracle::assert_spot_agreement;
use crate::types::{ScenarioResult, Verdict};
use crate::venue::{RestOrder, RestPosition};
use rust_decimal::Decimal;

#[test]
fn poisoned_rest_qty_fails_closed() {
    let mut r = ScenarioResult::new("neg", "PAPER", "Spot", "exchange");
    assert_spot_agreement(
        &mut r,
        "poison",
        Decimal::from_str_exact("0.001").unwrap(),
        Decimal::from_str_exact("999").unwrap(),
        Decimal::from_str_exact("0.001").unwrap(),
        None,
        None,
        "",
    );
    assert_eq!(r.verdict, Verdict::Fail);
    assert!(r.first_failure.is_some());
}

#[test]
fn ledger_sells_are_signed() {
    let jsonl = r#"{"kind":"fill","execution_id":"a","side":"Buy","quantity":"0.001"}
{"kind":"fill","execution_id":"b","side":"Sell","quantity":"0.001"}
"#;
    assert_eq!(crate::oracle::unique_ledger_qty(jsonl), Decimal::ZERO);
}

#[test]
fn not_supported_is_never_gate_ok() {
    let mut r = ScenarioResult::new("demo_mutating_preflight", "BYBIT", "Spot", "opt-in");
    r.verdict = Verdict::NotSupported;
    assert!(!r.gate_ok());
}

#[test]
fn expected_failure_is_gate_ok_only_when_scenario_fails() {
    let mut r = ScenarioResult::new("negative_control_mismatch", "PAPER", "Futures", "exchange");
    r.expected_failure = true;
    assert!(!r.gate_ok());
    r.fail("runner_vs_rest", "0.001", "1");
    assert!(r.expected_failure);
    assert_eq!(r.verdict, Verdict::Fail);
    assert!(r.gate_ok());
}

#[test]
fn mutate_auth_requires_explicit_box() {
    std::env::remove_var("WORKFLOW_DEMO_MUTATE");
    let err = crate::demo_levels::parse_mutate_auth().unwrap_err();
    assert!(err.contains("WORKFLOW_DEMO_MUTATE"));
}

#[test]
fn not_run_is_never_gate_ok() {
    let mut r = ScenarioResult::new("demo_levels_bounded", "BYBIT", "Futures", "exchange");
    r.mark_not_run("fill_flatten", "no fill");
    assert_eq!(r.verdict, Verdict::NotRun);
    assert!(!r.gate_ok());
}

#[test]
fn planned_qty_fails_when_min_qty_exceeds_bound() {
    let rules = crate::demo_levels::InstrumentRules {
        qty_step: Decimal::from_str_exact("0.001").unwrap(),
        min_qty: Decimal::from_str_exact("0.01").unwrap(),
        min_notional: Decimal::from_str_exact("5").unwrap(),
        tick: Decimal::from_str_exact("0.1").unwrap(),
    };
    let err = crate::demo_levels::plan_qty(
        &rules,
        Decimal::from_str_exact("100000").unwrap(),
        Decimal::from_str_exact("0.001").unwrap(),
        Decimal::from_str_exact("150").unwrap(),
    )
    .unwrap_err();
    assert!(err.contains("minOrderQty"));
}

#[test]
fn planned_qty_fits_current_btc() {
    let rules = crate::demo_levels::InstrumentRules {
        qty_step: Decimal::from_str_exact("0.001").unwrap(),
        min_qty: Decimal::from_str_exact("0.001").unwrap(),
        min_notional: Decimal::from_str_exact("5").unwrap(),
        tick: Decimal::from_str_exact("0.1").unwrap(),
    };
    let qty = crate::demo_levels::plan_qty(
        &rules,
        Decimal::from_str_exact("100000").unwrap(),
        Decimal::from_str_exact("0.001").unwrap(),
        Decimal::from_str_exact("150").unwrap(),
    )
    .unwrap();
    assert_eq!(qty, Decimal::from_str_exact("0.001").unwrap());
}

#[test]
fn secret_bot_archives_redacted_and_shreds_live() {
    let dir = std::env::temp_dir().join(format!("ts-secret-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let archive = dir.join("bot.json");
    let bot = serde_json::json!({
        "exchange_engine": {"config": {"credentials": {"apiKey": "SECRETKEY", "apiSecret": "SECRETSEC"}}}
    });
    let live = {
        let mut secret = crate::demo::SecretBotFile::create(archive.clone(), &bot).unwrap();
        let live_path = secret.path().to_path_buf();
        assert!(live_path.exists());
        let raw = std::fs::read_to_string(&live_path).unwrap();
        assert!(raw.contains("SECRETKEY"));
        secret.archive_redacted().unwrap();
        let archived = std::fs::read_to_string(&archive).unwrap();
        assert!(archived.contains("REDACTED"));
        assert!(!archived.contains("SECRETKEY"));
        live_path
    };
    assert!(!live.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rest_error_is_not_empty_account() {
    let err = cleanup_verification(Err("timeout".into()), Ok(vec![]), Some(true), Some(true))
        .unwrap_err();
    assert!(err.contains("unverified"));
    assert!(!err.contains("working=0"));
}

#[test]
fn cleanup_requires_rest_and_runner() {
    let leftover = vec![RestOrder {
        order_id: "1".into(),
        client_order_id: "cid-a".into(),
        instrument: "BTCUSDT".into(),
        price: Decimal::ONE,
        amount: Decimal::from_str_exact("0.001").unwrap(),
        filled: Decimal::ZERO,
        status: "New".into(),
        side: "Buy".into(),
    }];
    assert!(cleanup_verification(Ok(leftover), Ok(vec![]), Some(true), Some(true)).is_err());
    assert!(cleanup_verification(
        Ok(vec![]),
        Ok(vec![RestPosition {
            instrument: "BTCUSDT".into(),
            quantity: Decimal::from_str_exact("0.001").unwrap(),
        }]),
        Some(true),
        Some(true),
    )
    .is_err());
    assert!(cleanup_verification(Ok(vec![]), Ok(vec![]), Some(false), Some(true)).is_err());
    assert!(cleanup_verification(Ok(vec![]), Ok(vec![]), Some(true), Some(false)).is_err());
    assert!(cleanup_verification(Ok(vec![]), Ok(vec![]), None, Some(true)).is_err());
    cleanup_verification(Ok(vec![]), Ok(vec![]), Some(true), Some(true)).unwrap();
}

#[test]
fn failed_or_empty_get_levels_is_not_all_disabled() {
    let instance = expanded_instance_id("wf_demo_buy", "BTCUSDT");
    assert_eq!(levels_disabled_proof(false, &[], &instance), None);
    assert_eq!(levels_disabled_proof(true, &[], &instance), None);
    assert_eq!(
        levels_disabled_proof(
            false,
            &[("wf_demo_buy_BTCUSDT".into(), false, "ok".into())],
            &instance
        ),
        None
    );
    assert_eq!(
        levels_disabled_proof(
            true,
            &[("wf_demo_buy".into(), false, "ok".into())],
            &instance
        ),
        None
    );
    assert_eq!(
        levels_disabled_proof(
            true,
            &[("wf_demo_buy_BTCUSDT".into(), false, "ok".into())],
            &instance
        ),
        Some(true)
    );
    assert_eq!(
        levels_disabled_proof(
            true,
            &[("wf_demo_buy_BTCUSDT".into(), true, "ok".into())],
            &instance
        ),
        Some(false)
    );
    assert!(cleanup_verification(Ok(vec![]), Ok(vec![]), Some(true), None).is_err());
}

#[test]
fn paused_instance_id_is_expanded_not_template() {
    let levels = vec![("wf_demo_buy_BTCUSDT".into(), false, "Ready".into())];
    assert_eq!(
        resolve_paused_instance_id(true, &levels, "wf_demo_buy", "BTCUSDT").as_deref(),
        Ok("wf_demo_buy_BTCUSDT")
    );
    assert!(resolve_paused_instance_id(
        true,
        &[("wf_demo_buy".into(), false, "Ready".into())],
        "wf_demo_buy",
        "BTCUSDT"
    )
    .is_err());
    assert!(resolve_paused_instance_id(true, &[], "wf_demo_buy", "BTCUSDT").is_err());
    assert!(resolve_paused_instance_id(
        true,
        &[("wf_demo_buy_BTCUSDT".into(), true, "Ready".into())],
        "wf_demo_buy",
        "BTCUSDT"
    )
    .is_err());
}

#[test]
fn journal_watermark_before_enable_keeps_id_prepared_during_toggle() {
    let instance = "wf_demo_buy_BTCUSDT";
    let before_enable = "";
    let after_toggle = r#"{"client_order_id":"cid-fast","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
"#;
    let watermark = crate::oracle::latest_level_client_order_id(before_enable, instance);
    let mistaken = crate::oracle::latest_level_client_order_id(after_toggle, instance);
    assert_eq!(mistaken.as_deref(), Some("cid-fast"));
    assert_eq!(
        crate::oracle::journaled_client_id_after(after_toggle, instance, mistaken.as_deref()),
        None
    );
    assert_eq!(
        crate::oracle::journaled_client_id_after(after_toggle, instance, watermark.as_deref())
            .as_deref(),
        Some("cid-fast")
    );
}

#[test]
fn immediate_fill_is_accepted_from_history() {
    let spec = OwnedOrderSpec {
        client_order_id: "cid-fill".into(),
        instrument: "BTCUSDT".into(),
        side: "Buy".into(),
        qty: Decimal::from_str_exact("0.001").unwrap(),
        price: Decimal::from_str_exact("120000").unwrap(),
    };
    let filled = RestOrder {
        order_id: "ex-fill".into(),
        client_order_id: "cid-fill".into(),
        instrument: "BTCUSDT".into(),
        price: spec.price,
        amount: spec.qty,
        filled: spec.qty,
        status: "Filled".into(),
        side: "Buy".into(),
    };
    assert!(owned_order_from_open_or_history(&[], None, &spec).is_none());
    assert_eq!(
        owned_order_from_open_or_history(&[], Some(&filled), &spec)
            .unwrap()
            .order_id,
        "ex-fill"
    );
    assert!(owned_order_from_open_or_history(
        &[],
        Some(&RestOrder {
            client_order_id: "other".into(),
            ..filled.clone()
        }),
        &spec
    )
    .is_none());
}

#[test]
fn owned_order_requires_exact_journaled_id() {
    let spec = OwnedOrderSpec {
        client_order_id: "cid-wanted".into(),
        instrument: "BTCUSDT".into(),
        side: "Buy".into(),
        qty: Decimal::from_str_exact("0.001").unwrap(),
        price: Decimal::from_str_exact("50000").unwrap(),
    };
    let twin = RestOrder {
        order_id: "other".into(),
        client_order_id: "cid-other".into(),
        instrument: "BTCUSDT".into(),
        price: spec.price,
        amount: spec.qty,
        filled: Decimal::ZERO,
        status: "New".into(),
        side: "Buy".into(),
    };
    let owned = RestOrder {
        order_id: "mine".into(),
        client_order_id: "cid-wanted".into(),
        instrument: "BTCUSDT".into(),
        price: spec.price,
        amount: spec.qty,
        filled: Decimal::ZERO,
        status: "New".into(),
        side: "Buy".into(),
    };
    assert!(!rest_order_matches_owned(&twin, &spec));
    assert!(rest_order_matches_owned(&owned, &spec));
    let nonempty_only = OwnedOrderSpec {
        client_order_id: String::new(),
        ..spec.clone()
    };
    assert!(!rest_order_matches_owned(&owned, &nonempty_only));
}

#[test]
fn second_create_does_not_reuse_first_journal_id() {
    let first = r#"{"client_order_id":"cid-1","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
"#;
    assert_eq!(
        crate::oracle::journaled_client_id_after(first, "wf_demo_buy_BTCUSDT", None).as_deref(),
        Some("cid-1")
    );
    assert_eq!(
        crate::oracle::journaled_client_id_after(first, "wf_demo_buy_BTCUSDT", Some("cid-1")),
        None
    );
    let second = r#"{"client_order_id":"cid-1","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
{"client_order_id":"cid-2","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
"#;
    assert_eq!(
        crate::oracle::journaled_client_id_after(second, "wf_demo_buy_BTCUSDT", Some("cid-1"))
            .as_deref(),
        Some("cid-2")
    );
}

#[test]
fn cleanup_retry_ids_are_new_operations() {
    let first = next_cleanup_request_id("cancel", 1);
    let second = next_cleanup_request_id("cancel", 2);
    let flatten = next_cleanup_request_id("flatten", 2);
    assert_ne!(first, second);
    assert_ne!(second, flatten);
    assert!(first.contains("cancel-1"));
    assert!(second.contains("cancel-2"));
}

#[test]
fn latest_level_client_order_id_uses_last_row() {
    let jsonl = r#"{"client_order_id":"old","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
{"client_order_id":"new","level_id":"wf_demo_buy_BTCUSDT","state":"acknowledged"}
{"client_order_id":"other","level_id":"other_level","state":"submitted"}
"#;
    assert_eq!(
        crate::oracle::latest_level_client_order_id(jsonl, "wf_demo_buy_BTCUSDT").as_deref(),
        Some("new")
    );
}

#[test]
fn matching_qty_passes() {
    let mut r = ScenarioResult::new("ok", "PAPER", "Spot", "exchange");
    let q = Decimal::from_str_exact("0.001").unwrap();
    assert_spot_agreement(&mut r, "ok", q, q, q, Some(q), Some(q), "");
    assert_eq!(r.verdict, Verdict::Pass);
}

#[test]
fn walk_asks_uses_level_that_covers_qty() {
    let qty = Decimal::from_str_exact("0.001").unwrap();
    let tick = Decimal::from_str_exact("0.1").unwrap();
    let max = Decimal::from_str_exact("150").unwrap();
    let asks = [
        (
            Decimal::from_str_exact("83838.40").unwrap(),
            Decimal::from_str_exact("0.0004").unwrap(),
        ),
        (
            Decimal::from_str_exact("83850.00").unwrap(),
            Decimal::from_str_exact("0.002").unwrap(),
        ),
    ];
    let px = limit_buy_from_asks(&asks, qty, tick, max).unwrap();
    assert_eq!(px, Decimal::from_str_exact("83850.00").unwrap());
}

#[test]
fn walk_asks_ceils_to_tick() {
    let qty = Decimal::from_str_exact("0.001").unwrap();
    let tick = Decimal::from_str_exact("0.1").unwrap();
    let max = Decimal::from_str_exact("150").unwrap();
    let asks = [(
        Decimal::from_str_exact("83838.41").unwrap(),
        Decimal::from_str_exact("0.01").unwrap(),
    )];
    let px = limit_buy_from_asks(&asks, qty, tick, max).unwrap();
    assert_eq!(px, Decimal::from_str_exact("83838.50").unwrap());
    assert_eq!(
        align_tick_up(Decimal::from_str_exact("83838.41").unwrap(), tick),
        px
    );
}

#[test]
fn walk_asks_rejects_notional_over_cap() {
    let qty = Decimal::from_str_exact("0.001").unwrap();
    let tick = Decimal::from_str_exact("0.1").unwrap();
    let max = Decimal::from_str_exact("150").unwrap();
    let asks = [(
        Decimal::from_str_exact("200000").unwrap(),
        Decimal::from_str_exact("0.01").unwrap(),
    )];
    let err = limit_buy_from_asks(&asks, qty, tick, max).unwrap_err();
    assert!(err.contains("exceeds max"));
}

#[test]
fn walk_asks_rejects_insufficient_depth() {
    let qty = Decimal::from_str_exact("0.001").unwrap();
    let tick = Decimal::from_str_exact("0.1").unwrap();
    let max = Decimal::from_str_exact("150").unwrap();
    let asks = [(
        Decimal::from_str_exact("83850").unwrap(),
        Decimal::from_str_exact("0.0004").unwrap(),
    )];
    let err = limit_buy_from_asks(&asks, qty, tick, max).unwrap_err();
    assert!(err.contains("ask depth"));
}

#[test]
fn last_plus_five_ticks_can_sit_below_live_ask() {
    let last = Decimal::from_str_exact("83838.40").unwrap();
    let tick = Decimal::from_str_exact("0.10").unwrap();
    let stale = last + tick * Decimal::from(5);
    let qty = Decimal::from_str_exact("0.001").unwrap();
    let walk = limit_buy_from_asks(
        &[(
            Decimal::from_str_exact("83850.00").unwrap(),
            Decimal::from_str_exact("0.02").unwrap(),
        )],
        qty,
        tick,
        Decimal::from_str_exact("150").unwrap(),
    )
    .unwrap();
    assert!(walk > last);
    assert!(walk > stale);
}

#[test]
fn parse_asks_reads_bybit_rows() {
    let body = serde_json::json!({
        "a": [["83840.1", "0.02"], ["83841.0", "0.01"]]
    });
    let asks = parse_asks(&body).unwrap();
    assert_eq!(asks.len(), 2);
    assert_eq!(asks[0].0, Decimal::from_str_exact("83840.1").unwrap());
}

#[test]
fn amend_does_not_add_a_third_journal_id() {
    let jsonl = r#"{"client_order_id":"cid-1","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
{"client_order_id":"cid-2","level_id":"wf_demo_buy_BTCUSDT","state":"submitted"}
{"client_order_id":"cid-2","level_id":"wf_demo_buy_BTCUSDT","state":"acknowledged"}
"#;
    let ids = crate::oracle::distinct_level_client_order_ids(jsonl, "wf_demo_buy_BTCUSDT");
    assert_eq!(ids, vec!["cid-1".to_string(), "cid-2".to_string()]);
}
