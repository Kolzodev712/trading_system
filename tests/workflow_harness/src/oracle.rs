use rust_decimal::Decimal;

use crate::types::ScenarioResult;
use crate::venue::{RestBalance, RestOrder, RestPosition, RunnerSnapshot};

const EPS: &str = "0.000001";

fn close(a: Decimal, b: Decimal) -> bool {
    (a - b).abs() <= Decimal::from_str_exact(EPS).unwrap()
}

pub fn btc_total(balances: &[RestBalance]) -> Decimal {
    balances
        .iter()
        .find(|b| b.asset.eq_ignore_ascii_case("BTC") || b.asset.eq_ignore_ascii_case("XBT"))
        .map(|b| b.total)
        .unwrap_or(Decimal::ZERO)
}

pub fn runner_btc_qty(snap: &RunnerSnapshot) -> Decimal {
    snap.positions
        .iter()
        .find(|(sym, _, _)| sym.contains("BTC"))
        .map(|(_, q, _)| *q)
        .unwrap_or(Decimal::ZERO)
}

pub fn rest_btc_position(positions: &[RestPosition]) -> Decimal {
    positions
        .iter()
        .find(|p| p.instrument.to_uppercase().contains("BTC"))
        .map(|p| p.quantity)
        .unwrap_or(Decimal::ZERO)
}

pub fn unique_ledger_qty(jsonl: &str) -> Decimal {
    let mut seen = std::collections::HashSet::new();
    let mut qty = Decimal::ZERO;
    for line in jsonl.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let fill = if v.get("kind").and_then(|k| k.as_str()) == Some("fill") {
            Some(&v)
        } else {
            v.get("Fill")
        };
        let Some(fill) = fill else { continue };
        let eid = fill
            .get("execution_id")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if eid.is_empty() || !seen.insert(eid.to_string()) {
            continue;
        }
        let q =
            Decimal::from_str_exact(fill.get("quantity").and_then(|x| x.as_str()).unwrap_or("0"))
                .unwrap_or(Decimal::ZERO);
        let side = fill.get("side").and_then(|x| x.as_str()).unwrap_or("");
        if side.eq_ignore_ascii_case("sell") {
            qty -= q;
        } else {
            qty += q;
        }
    }
    qty
}

pub fn catalog_filled_qty(catalog: &serde_json::Value) -> Decimal {
    let mut qty = Decimal::ZERO;
    if let Some(arr) = catalog.get("instances").and_then(|x| x.as_array()) {
        for inst in arr {
            if let Some(s) = inst.get("filled_qty").and_then(|x| x.as_str()) {
                qty += Decimal::from_str_exact(s).unwrap_or(Decimal::ZERO);
            }
        }
    }
    qty
}

pub fn catalog_owns(catalog: &serde_json::Value, instance_id: &str, order_id: &str) -> bool {
    catalog
        .get("instances")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
        .any(|inst| {
            inst.get("id").and_then(|x| x.as_str()) == Some(instance_id)
                && inst
                    .get("owned_exchange_ids")
                    .and_then(|x| x.as_array())
                    .into_iter()
                    .flatten()
                    .any(|v| v.as_str() == Some(order_id))
        })
}

pub fn journal_has_state(jsonl: &str, state: &str) -> bool {
    jsonl.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|v| v.get("state").and_then(|s| s.as_str()).map(|s| s == state))
            .unwrap_or(false)
    })
}

/// Latest journaled client order id for a level. Empty ids are ignored.
/// A later fill/create must wait for a *new* id via [`journaled_client_id_after`].
pub fn latest_level_client_order_id(jsonl: &str, level_id: &str) -> Option<String> {
    let mut found = None;
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let matches_level = v.get("level_id").and_then(|x| x.as_str()) == Some(level_id);
        let Some(cid) = v.get("client_order_id").and_then(|x| x.as_str()) else {
            continue;
        };
        if matches_level && !cid.is_empty() {
            found = Some(cid.to_string());
        }
    }
    found
}

/// Wait target for a subsequent Demo create. The latest existing row is not a new id.
pub fn journaled_client_id_after(
    jsonl: &str,
    level_id: &str,
    after: Option<&str>,
) -> Option<String> {
    let cid = latest_level_client_order_id(jsonl, level_id)?;
    match after {
        Some(prev) if prev == cid => None,
        _ => Some(cid),
    }
}

pub fn halt_is_disagreement(snap: &RunnerSnapshot) -> bool {
    snap.last_block_reason_code == "RECONCILE_DISAGREEMENT"
        || snap
            .exposure_halt_reason
            .to_lowercase()
            .contains("disagrees")
        || snap
            .exposure_halt_reason
            .to_lowercase()
            .contains("disagreement")
}

pub fn working_ids(orders: &[RestOrder]) -> Vec<String> {
    orders
        .iter()
        .filter(|o| o.status == "New" || o.status == "PartiallyFilled")
        .map(|o| o.order_id.clone())
        .collect()
}

pub fn qty_close(a: Decimal, b: Decimal) -> bool {
    close(a, b)
}

pub fn qty_canon(d: Decimal) -> String {
    d.round_dp(8).normalize().to_string()
}

fn canon(d: Decimal) -> String {
    qty_canon(d)
}

pub fn assert_spot_agreement(
    result: &mut ScenarioResult,
    tag: &str,
    expect: Decimal,
    paper_btc: Decimal,
    runner: Decimal,
    catalog: Option<Decimal>,
    ledger: Option<Decimal>,
    halt: &str,
) {
    result.assert_eq(&format!("{tag}.paper_btc"), canon(expect), canon(paper_btc));
    result.assert_eq(&format!("{tag}.runner"), canon(expect), canon(runner));
    if let Some(c) = catalog {
        result.assert_eq(&format!("{tag}.catalog"), canon(expect), canon(c));
    }
    if let Some(l) = ledger {
        result.assert_eq(&format!("{tag}.ledger"), canon(expect), canon(l));
    }
    if halt.to_lowercase().contains("spot position disagrees") {
        result.fail(
            &format!("{tag}.no_disagreement"),
            "no disagreement halt",
            halt,
        );
    }
}
