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
fn matching_qty_passes() {
    let mut r = ScenarioResult::new("ok", "PAPER", "Spot", "exchange");
    let q = Decimal::from_str_exact("0.001").unwrap();
    assert_spot_agreement(&mut r, "ok", q, q, q, Some(q), Some(q), "");
    assert_eq!(r.verdict, Verdict::Pass);
}
