use crate::model::{Availability, SubjectReport, TradingReport};

pub fn render_markdown(report: &TradingReport) -> String {
    let mut out = String::new();
    out.push_str("# Trading report v1\n\n");
    out.push_str(&format!("Generated (UTC): {}\n\n", report.generated_at_utc));
    out.push_str(&format!(
        "Overall verdict: **{}** (actionable_for_analysis: {})\n\n",
        report.verdict.as_str(),
        report.actionable_for_analysis
    ));
    if !report.reasons.is_empty() {
        out.push_str("Overall reasons:\n");
        for reason in &report.reasons {
            out.push_str(&format!("- `{}`: {}\n", reason.code, reason.message));
        }
        out.push('\n');
    }
    for subject in &report.subjects {
        out.push_str(&render_subject(subject));
        out.push('\n');
    }
    out
}

fn render_subject(subject: &SubjectReport) -> String {
    let mut out = String::new();
    let id = &subject.identity;
    out.push_str(&format!(
        "## Subject `{}` — {} / {}\n\n",
        id.runner_label, id.instrument, id.account_alias
    ));
    out.push_str(&format!(
        "Subject verdict: **{}** (actionable_for_analysis: {})\n\n",
        subject.verdict.as_str(),
        subject.actionable_for_analysis
    ));

    out.push_str("### 1. Where and when?\n\n");
    out.push_str(&format!("- Report UTC: {}\n", id.generated_at_utc));
    out.push_str(&format!(
        "- Runner: {} ({})\n",
        id.runner_label,
        avail(&id.runner_mode)
    ));
    out.push_str(&format!(
        "- Venue / environment / mode: {} / {} / {}\n",
        avail(&id.venue),
        avail(&id.environment),
        avail(&id.execution_mode)
    ));
    out.push_str(&format!(
        "- Account alias: {} (venue-verified: {})\n",
        id.account_alias, id.account_alias_venue_verified
    ));
    out.push_str(&format!(
        "- Instrument: {} ({})\n",
        id.instrument, id.instrument_type
    ));
    out.push_str(&format!(
        "- Executable revision: {}\n",
        id.executable_revision.as_deref().unwrap_or("not supplied")
    ));
    out.push_str(&format!(
        "- Config identity: {}\n\n",
        avail(&id.config_identity)
    ));

    let m = &subject.market;
    out.push_str("### 2. What is the market doing?\n\n");
    out.push_str(&format!("- Instrument: {}\n", m.instrument));
    out.push_str(&format!(
        "- Best bid / ask: {} / {}\n",
        avail(&m.best_bid),
        avail(&m.best_ask)
    ));
    out.push_str(&format!("- Spread: {}\n", avail(&m.spread)));
    out.push_str(&format!(
        "- Depth (bid/ask): {} / {}\n",
        avail(&m.bid_depth),
        avail(&m.ask_depth)
    ));
    out.push_str(&format!(
        "- Book time / age: {} / {}\n",
        avail(&m.book_timestamp_utc),
        avail_i64(&m.book_age_ms)
    ));
    out.push_str(&format!(
        "- Completed candle: {}\n\n",
        avail_display(&m.completed_candle)
    ));

    let e = &subject.exposure;
    out.push_str("### 3. What exposure exists?\n\n");
    out.push_str(&format!(
        "- Instrument / account: {} / {}\n",
        e.instrument, id.account_alias
    ));
    out.push_str(&format!(
        "- Venue orders: {}\n",
        list_or_missing(&e.venue_orders)
    ));
    out.push_str(&format!(
        "- Venue balances: {}\n",
        list_or_missing(&e.venue_balances)
    ));
    out.push_str(&format!(
        "- Venue positions: {}\n",
        list_or_missing(&e.venue_positions)
    ));
    out.push_str(&format!(
        "- Runner orders: {}\n",
        list_or_missing(&e.runner_orders)
    ));
    out.push_str(&format!(
        "- Runner positions: {}\n",
        list_or_missing(&e.runner_positions)
    ));
    if e.differences.is_empty() {
        out.push_str("- Differences: none recorded\n\n");
    } else {
        out.push_str("- Differences:\n");
        for diff in &e.differences {
            out.push_str(&format!("  - {diff}\n"));
        }
        out.push('\n');
    }

    let i = &subject.intent;
    out.push_str("### 4. What is the system trying and allowed to do?\n\n");
    out.push_str(&format!(
        "- Global enable / entries paused / flatten latch: {} / {} / {}\n",
        avail_bool(&i.trading_enabled),
        avail_bool(&i.entries_paused),
        avail_bool(&i.flatten_latched)
    ));
    out.push_str(&format!(
        "- Exposure halt: {}\n",
        avail(&i.exposure_halt_reason)
    ));
    out.push_str(&format!(
        "- Strategies: {}\n",
        list_or_missing(&i.strategies)
    ));
    out.push_str(&format!("- Levels: {}\n", list_or_missing(&i.levels)));
    out.push_str(&format!("- Risk: {}\n", avail_display(&i.risk)));
    out.push_str(&format!(
        "- Operator mandate (advisory={}, enforced_by_runner={}): instruments {:?}, direction {}, leverage {}, exposure {}, loss/trade {}, loss/day {}, horizon {}\n\n",
        i.mandate.advisory,
        i.mandate.enforced_by_runner,
        i.mandate.allowed_instruments,
        i.mandate.allowed_direction,
        i.mandate.max_leverage,
        i.mandate.max_exposure,
        i.mandate.max_loss_per_trade,
        i.mandate.max_loss_per_day,
        i.mandate.time_horizon
    ));

    let t = &subject.trust;
    out.push_str("### 5. Can this snapshot be trusted?\n\n");
    out.push_str(&format!(
        "- Verdict: **{}** (actionable_for_analysis: {})\n",
        t.verdict.as_str(),
        t.actionable_for_analysis
    ));
    out.push_str("- Sources:\n");
    for src in &t.sources {
        out.push_str(&format!(
            "  - {} ok={} age_ms={:?} error={}\n",
            src.source,
            src.ok,
            src.age_ms,
            src.error.as_deref().unwrap_or("")
        ));
    }
    out.push_str(&format!(
        "- Unresolved submissions: {}\n",
        list_or_missing(&t.unresolved_submissions)
    ));
    if t.reasons.is_empty() {
        out.push_str("- Reasons: none\n");
    } else {
        out.push_str("- Reasons:\n");
        for reason in &t.reasons {
            out.push_str(&format!("  - `{}`: {}\n", reason.code, reason.message));
        }
    }
    out
}

fn avail(v: &Availability<String>) -> String {
    if v.available {
        v.value.clone().unwrap_or_else(|| "null".into())
    } else {
        format!("unavailable ({})", v.reason.as_deref().unwrap_or("unknown"))
    }
}

fn avail_bool(v: &Availability<bool>) -> String {
    if v.available {
        v.value
            .map(|b| b.to_string())
            .unwrap_or_else(|| "null".into())
    } else {
        format!("unavailable ({})", v.reason.as_deref().unwrap_or("unknown"))
    }
}

fn avail_i64(v: &Availability<i64>) -> String {
    if v.available {
        v.value
            .map(|b| b.to_string())
            .unwrap_or_else(|| "null".into())
    } else {
        format!("unavailable ({})", v.reason.as_deref().unwrap_or("unknown"))
    }
}

fn avail_display<T: std::fmt::Debug>(v: &Availability<T>) -> String {
    if v.available {
        format!("{:?}", v.value)
    } else {
        format!("unavailable ({})", v.reason.as_deref().unwrap_or("unknown"))
    }
}

fn list_or_missing<T: std::fmt::Debug>(v: &Availability<Vec<T>>) -> String {
    if !v.available {
        return format!("unavailable ({})", v.reason.as_deref().unwrap_or("unknown"));
    }
    match &v.value {
        Some(items) => format!("{} item(s): {:?}", items.len(), items),
        None => "null".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Reason, TradingReport, Verdict};

    #[test]
    fn markdown_keeps_unverified_verdict() {
        let report = TradingReport {
            schema_version: crate::model::SCHEMA_VERSION.into(),
            generated_at_utc: "2026-10-01T00:00:00Z".into(),
            subjects: vec![],
            verdict: Verdict::Unverified,
            actionable_for_analysis: false,
            reasons: vec![Reason::new("VENUE_REST_ERROR", "GET /open_orders failed")],
        };
        let md = render_markdown(&report);
        assert!(md.contains("### 1. Where and when?") || md.contains("Overall verdict"));
        assert!(md.contains("unverified"));
        assert!(md.contains("GET /open_orders failed"));
        assert!(!md.contains("actionable_for_analysis: true"));
    }
}
