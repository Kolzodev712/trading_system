//! Oracle unit tests (no processes).

use crate::demo_levels::{cleanup_verification, rest_order_matches_owned, OwnedOrderSpec};
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
fn latest_level_client_order_id_uses_last_row() {
    let jsonl = r#"{"client_order_id":"old","level_id":"wf_demo_buy","state":"submitted"}
{"client_order_id":"new","level_id":"wf_demo_buy","state":"acknowledged"}
{"client_order_id":"other","level_id":"other_level","state":"submitted"}
"#;
    assert_eq!(
        crate::oracle::latest_level_client_order_id(jsonl, "wf_demo_buy").as_deref(),
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
