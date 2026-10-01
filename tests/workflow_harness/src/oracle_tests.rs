//! Oracle unit tests (no processes).

use crate::oracle::assert_spot_agreement;
use crate::types::{ScenarioResult, Verdict};
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
fn matching_qty_passes() {
    let mut r = ScenarioResult::new("ok", "PAPER", "Spot", "exchange");
    let q = Decimal::from_str_exact("0.001").unwrap();
    assert_spot_agreement(&mut r, "ok", q, q, q, Some(q), Some(q), "");
    assert_eq!(r.verdict, Verdict::Pass);
}
