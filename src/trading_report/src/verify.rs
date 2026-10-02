use rust_decimal::Decimal;

use crate::config::{FreshnessConfig, RunnerConfig};
use crate::journal_source::unresolved_rows;
use crate::model::{
    Availability, ExposureSection, IdentitySection, IntentSection, JournalRow, MarketSection,
    Reason, SourceTime, SubjectReport, TrustSection, Verdict,
};
use crate::runner_source::{
    age_ms, completed_candle, epoch_ms_to_utc, proto_balances, proto_orders, proto_positions,
    proto_strategies, RunnerSnapshot,
};
use crate::venue_source::{
    instrument_base, matches_instrument, normalize_symbol, qty_close, VenueSnapshot,
};

pub fn subject_report(
    cfg: &RunnerConfig,
    freshness: &FreshnessConfig,
    mandate: crate::model::MandateView,
    generated_at_utc: &str,
    now_ms: i64,
    runner: &RunnerSnapshot,
    venue: &VenueSnapshot,
    journal: Result<Vec<JournalRow>, String>,
) -> SubjectReport {
    let mut reasons = Vec::new();
    let mut sources = runner.sources.clone();
    sources.push(SourceTime {
        source: "venue_rest".into(),
        observed_at_utc: Some(venue.observed_at_utc.clone()),
        age_ms: None,
        ok: venue.orders.is_ok()
            && venue.balances.is_ok()
            && (venue.positions.is_ok() || !venue.positions_supported),
        error: venue
            .orders
            .as_ref()
            .err()
            .cloned()
            .or_else(|| venue.balances.as_ref().err().cloned())
            .or_else(|| {
                if venue.positions_supported {
                    venue.positions.as_ref().err().cloned()
                } else {
                    None
                }
            }),
    });

    let status = runner.status.as_ref();
    let monitoring = runner.monitoring.as_ref();

    let venue_avail = match status {
        Ok(s) if !s.venue.is_empty() => Availability::ok(s.venue.clone()),
        Ok(_) => {
            reasons.push(Reason::new(
                "IDENTITY_DEFAULT",
                "GetTradingStatus venue is empty",
            ));
            Availability::missing("empty venue")
        }
        Err(e) => Availability::missing(e.clone()),
    };
    let env_avail = match status {
        Ok(s) if !s.environment.is_empty() => Availability::ok(s.environment.clone()),
        Ok(_) => {
            reasons.push(Reason::new(
                "IDENTITY_DEFAULT",
                "GetTradingStatus environment is empty",
            ));
            Availability::missing("empty environment")
        }
        Err(e) => Availability::missing(e.clone()),
    };
    let mode_avail = match status {
        Ok(s) if !s.execution_mode.is_empty() => Availability::ok(s.execution_mode.clone()),
        Ok(_) => Availability::missing("empty execution_mode"),
        Err(e) => Availability::missing(e.clone()),
    };

    if let (Some(v), Some(e), Some(m)) = (
        venue_avail.value.as_ref(),
        env_avail.value.as_ref(),
        mode_avail.value.as_ref(),
    ) {
        if !v.eq_ignore_ascii_case(&cfg.venue)
            || !e.eq_ignore_ascii_case(&cfg.environment)
            || !m.eq_ignore_ascii_case(&cfg.execution_mode)
        {
            reasons.push(Reason::new(
                "IDENTITY_MISMATCH",
                format!(
                    "configured {}/{}/{} vs live {}/{}/{}",
                    cfg.venue, cfg.environment, cfg.execution_mode, v, e, m
                ),
            ));
        }
    }

    let runner_mode = match monitoring {
        Ok(d) if !d.runner_mode.is_empty() => Availability::ok(d.runner_mode.clone()),
        Ok(_) => Availability::missing("empty runner_mode"),
        Err(e) => Availability::missing(e.clone()),
    };
    if let Some(mode) = runner_mode.value.as_ref() {
        if !mode.eq_ignore_ascii_case(&cfg.mode) {
            reasons.push(Reason::new(
                "IDENTITY_MISMATCH",
                format!("configured mode {} vs live {mode}", cfg.mode),
            ));
        }
    }

    let mut identity = IdentitySection {
        generated_at_utc: generated_at_utc.to_string(),
        runner_label: cfg.label.clone(),
        runner_mode,
        venue: venue_avail,
        environment: env_avail,
        execution_mode: mode_avail,
        account_alias: cfg.account_alias.clone(),
        account_alias_venue_verified: false,
        instrument: cfg.instrument.clone(),
        instrument_type: cfg.instrument_type.clone(),
        executable_revision: cfg
            .executable_revision
            .clone()
            .filter(|s| !s.trim().is_empty()),
        config_identity: match monitoring {
            Ok(d) => d
                .strategies
                .first()
                .filter(|s| !s.config_hash.is_empty())
                .map(|s| Availability::ok(s.config_hash.clone()))
                .unwrap_or_else(|| Availability::missing("config hash not supplied")),
            Err(e) => Availability::missing(e.clone()),
        },
        source_times: sources.clone(),
    };

    let market = market_section(cfg, freshness, now_ms, monitoring, &mut reasons);
    let exposure = exposure_section(cfg, freshness, monitoring, venue, &mut reasons);
    let intent = intent_section(cfg, mandate, status, monitoring, runner, &mut reasons);

    let journal_rows = match &journal {
        Ok(rows) => {
            sources.push(SourceTime {
                source: "journal".into(),
                observed_at_utc: Some(generated_at_utc.to_string()),
                age_ms: None,
                ok: true,
                error: None,
            });
            let unresolved = unresolved_rows(rows);
            if !unresolved.is_empty() {
                reasons.push(Reason::new(
                    "JOURNAL_UNRESOLVED",
                    format!(
                        "{} unresolved journal submission(s); report is not actionable",
                        unresolved.len()
                    ),
                ));
            }
            Availability::ok(unresolved)
        }
        Err(err) => {
            reasons.push(Reason::new("JOURNAL_UNAVAILABLE", err.clone()));
            sources.push(SourceTime {
                source: "journal".into(),
                observed_at_utc: Some(generated_at_utc.to_string()),
                age_ms: None,
                ok: false,
                error: Some(err.clone()),
            });
            Availability::missing(err.clone())
        }
    };

    apply_source_ages(&mut sources, now_ms, freshness, &mut reasons);
    identity.source_times = sources.clone();

    let verdict = decide_verdict(&reasons);
    let trust = TrustSection {
        sources,
        unresolved_submissions: journal_rows,
        verdict,
        actionable_for_analysis: verdict == Verdict::Verified,
        reasons: reasons.clone(),
    };

    SubjectReport {
        identity,
        market,
        exposure,
        intent,
        trust,
        verdict,
        actionable_for_analysis: verdict == Verdict::Verified,
        reasons,
    }
}

fn market_section(
    cfg: &RunnerConfig,
    freshness: &FreshnessConfig,
    now_ms: i64,
    monitoring: Result<&crate::runner_source::proto::MonitoringData, &String>,
    reasons: &mut Vec<Reason>,
) -> MarketSection {
    let missing = |why: &str| MarketSection {
        instrument: cfg.instrument.clone(),
        best_bid: Availability::missing(why),
        best_ask: Availability::missing(why),
        spread: Availability::missing(why),
        bid_depth: Availability::missing(why),
        ask_depth: Availability::missing(why),
        book_timestamp_utc: Availability::missing(why),
        book_age_ms: Availability::missing(why),
        completed_candle: Availability::missing(why),
    };
    let Ok(data) = monitoring else {
        let why = monitoring
            .err()
            .cloned()
            .unwrap_or_else(|| "monitoring missing".into());
        reasons.push(Reason::new("MONITORING_MISSING", why.clone()));
        return missing(&why);
    };
    if data.orderbook_source != "ok" {
        let why = if data.orderbook_source.is_empty() {
            "orderbook source not reported"
        } else {
            "orderbook unavailable"
        };
        reasons.push(Reason::new("BOOK_UNAVAILABLE", why));
        let mut section = missing(why);
        section.completed_candle = completed_candle(data);
        return section;
    }
    let book = data
        .orderbooks
        .iter()
        .find(|b| normalize_symbol(&b.pair) == normalize_symbol(&cfg.instrument));
    let Some(book) = book else {
        reasons.push(Reason::new(
            "BOOK_UNAVAILABLE",
            "no orderbook for configured instrument",
        ));
        let mut section = missing("no orderbook for configured instrument");
        section.completed_candle = completed_candle(data);
        return section;
    };
    if book.best_bid.is_empty() || book.best_ask.is_empty() {
        reasons.push(Reason::new(
            "BOOK_UNAVAILABLE",
            "absent book is not a zero-price book",
        ));
        let mut section = missing("absent book");
        section.completed_candle = completed_candle(data);
        return section;
    }
    let age = age_ms(now_ms, book.timestamp);
    if let Some(age) = age {
        if age > freshness.max_book_age_ms {
            reasons.push(Reason::new(
                "BOOK_STALE",
                format!("book age {age}ms exceeds {}", freshness.max_book_age_ms),
            ));
        }
    } else {
        reasons.push(Reason::new("BOOK_UNAVAILABLE", "book timestamp missing"));
    }
    let bid_depth = book
        .bids
        .first()
        .map(|l| l.quantity.clone())
        .unwrap_or_default();
    let ask_depth = book
        .asks
        .first()
        .map(|l| l.quantity.clone())
        .unwrap_or_default();
    MarketSection {
        instrument: cfg.instrument.clone(),
        best_bid: Availability::ok(book.best_bid.clone()),
        best_ask: Availability::ok(book.best_ask.clone()),
        spread: Availability::ok(book.spread.clone()),
        bid_depth: Availability::ok(bid_depth),
        ask_depth: Availability::ok(ask_depth),
        book_timestamp_utc: epoch_ms_to_utc(book.timestamp)
            .map(Availability::ok)
            .unwrap_or_else(|| Availability::missing("book timestamp")),
        book_age_ms: age
            .map(Availability::ok)
            .unwrap_or_else(|| Availability::missing("age")),
        completed_candle: completed_candle(data),
    }
}

fn exposure_section(
    cfg: &RunnerConfig,
    freshness: &FreshnessConfig,
    monitoring: Result<&crate::runner_source::proto::MonitoringData, &String>,
    venue: &VenueSnapshot,
    reasons: &mut Vec<Reason>,
) -> ExposureSection {
    if let Err(err) = &venue.orders {
        reasons.push(Reason::new("VENUE_REST_ERROR", err.clone()));
    }
    if let Err(err) = &venue.balances {
        reasons.push(Reason::new("VENUE_REST_ERROR", err.clone()));
    }
    if venue.positions_supported {
        if let Err(err) = &venue.positions {
            reasons.push(Reason::new("VENUE_REST_ERROR", err.clone()));
        }
    }
    let runner_orders = match monitoring {
        Ok(d) => Availability::ok(scoped_orders(&proto_orders(d), &cfg.instrument)),
        Err(e) => Availability::missing((*e).clone()),
    };
    let runner_positions = match monitoring {
        Ok(d) => Availability::ok(scoped_positions(&proto_positions(d), &cfg.instrument)),
        Err(e) => Availability::missing((*e).clone()),
    };
    let runner_balances = match monitoring {
        Ok(d) => proto_balances(d),
        Err(_) => Vec::new(),
    };
    let venue_orders = match &venue.orders {
        Ok(v) => Availability::ok(scoped_orders(v, &cfg.instrument)),
        Err(e) => Availability::missing(e.clone()),
    };
    let venue_balances = match &venue.balances {
        Ok(v) => Availability::ok(v.clone()),
        Err(e) => Availability::missing(e.clone()),
    };
    let venue_positions = match &venue.positions {
        Ok(v) => Availability::ok(scoped_positions(v, &cfg.instrument)),
        Err(e) => Availability::missing(e.clone()),
    };

    let mut differences = Vec::new();
    let tol = freshness
        .qty_tolerance
        .parse::<Decimal>()
        .unwrap_or(Decimal::new(1, 8));
    if let (Some(vo), Some(ro)) = (&venue_orders.value, &runner_orders.value) {
        let v_ids: std::collections::BTreeSet<_> = vo.iter().map(|o| o.order_id.clone()).collect();
        let r_ids: std::collections::BTreeSet<_> = ro.iter().map(|o| o.order_id.clone()).collect();
        if v_ids != r_ids {
            differences.push(format!(
                "working order ids differ: venue {v_ids:?} runner {r_ids:?}"
            ));
        }
    }
    if venue.positions_supported {
        if let (Some(vp), Some(rp)) = (&venue_positions.value, &runner_positions.value) {
            let vq = inventory_qty(vp.iter().map(|p| p.quantity.as_str()));
            let rq = inventory_qty(rp.iter().map(|p| p.quantity.as_str()));
            match compare_qty(vq, rq, tol) {
                Ok(false) => {
                    differences.push(format!("position qty differs: venue {vq} runner {rq}"));
                }
                Ok(true) => {}
                Err(err) => reasons.push(Reason::new("QTY_MALFORMED", err)),
            }
        }
    }
    if cfg.instrument_type.eq_ignore_ascii_case("spot") {
        if let (Some(bals), Ok(_)) = (&venue_balances.value, monitoring) {
            let base = instrument_base(&cfg.instrument);
            let venue_qty = bals
                .iter()
                .find(|b| b.asset.eq_ignore_ascii_case(base))
                .map(|b| b.total.as_str())
                .unwrap_or("0");
            let runner_qty = runner_inventory_qty(
                runner_positions.value.as_deref().unwrap_or(&[]),
                &runner_balances,
                &cfg.instrument,
            );
            match compare_qty(venue_qty, &runner_qty, tol) {
                Ok(false) => {
                    differences.push(format!(
                        "spot base inventory differs: venue {base}={venue_qty} runner {runner_qty}"
                    ));
                }
                Ok(true) => {}
                Err(err) => reasons.push(Reason::new("QTY_MALFORMED", err)),
            }
        }
    }
    if differences.iter().any(|d| d.contains("differ")) {
        reasons.push(Reason::new("EXPOSURE_DISAGREEMENT", differences.join("; ")));
    }

    ExposureSection {
        instrument: cfg.instrument.clone(),
        venue_orders,
        venue_balances,
        venue_positions,
        runner_orders,
        runner_positions,
        differences,
    }
}

fn intent_section(
    cfg: &RunnerConfig,
    mandate: crate::model::MandateView,
    status: Result<&crate::runner_source::proto::GetTradingStatusResponse, &String>,
    monitoring: Result<&crate::runner_source::proto::MonitoringData, &String>,
    runner: &RunnerSnapshot,
    reasons: &mut Vec<Reason>,
) -> IntentSection {
    if let Err(err) = &runner.risk {
        reasons.push(Reason::new("RISK_UNAVAILABLE", err.clone()));
    }
    if cfg.mode.eq_ignore_ascii_case("levels") {
        match &runner.levels {
            Ok(None) => reasons.push(Reason::new("LEVELS_MISSING", "GetLevels not called")),
            Err(err) => reasons.push(Reason::new("LEVELS_MISSING", err.clone())),
            Ok(Some(_)) => {}
        }
    }
    IntentSection {
        trading_enabled: status
            .map(|s| Availability::ok(s.enabled))
            .unwrap_or_else(|e| Availability::missing(e.clone())),
        entries_paused: status
            .map(|s| Availability::ok(s.entries_paused))
            .unwrap_or_else(|e| Availability::missing(e.clone())),
        flatten_latched: status
            .map(|s| Availability::ok(s.flatten_latched))
            .unwrap_or_else(|e| Availability::missing(e.clone())),
        exposure_halt_reason: status
            .map(|s| Availability::ok(s.exposure_halt_reason.clone()))
            .unwrap_or_else(|e| Availability::missing(e.clone())),
        strategies: monitoring
            .map(|d| Availability::ok(proto_strategies(d)))
            .unwrap_or_else(|e| Availability::missing(e.clone())),
        levels: match &runner.levels {
            Ok(Some(levels)) => Availability::ok(levels.clone()),
            Ok(None) => Availability::missing("not required outside levels mode"),
            Err(e) => Availability::missing(e.clone()),
        },
        risk: match &runner.risk {
            Ok(risk) => {
                if risk.instrument.is_empty()
                    || !matches_instrument(&risk.instrument, &cfg.instrument)
                {
                    reasons.push(Reason::new(
                        "RISK_INSTRUMENT_MISMATCH",
                        format!(
                            "risk snapshot instrument '{}' is not bound to {}",
                            risk.instrument, cfg.instrument
                        ),
                    ));
                }
                Availability::ok(risk.clone())
            }
            Err(e) => Availability::missing(e.clone()),
        },
        mandate,
    }
}

fn scoped_orders(
    orders: &[crate::model::OrderView],
    instrument: &str,
) -> Vec<crate::model::OrderView> {
    orders
        .iter()
        .filter(|o| matches_instrument(&o.symbol, instrument))
        .cloned()
        .collect()
}

fn scoped_positions(
    positions: &[crate::model::PositionView],
    instrument: &str,
) -> Vec<crate::model::PositionView> {
    positions
        .iter()
        .filter(|p| matches_instrument(&p.symbol, instrument))
        .cloned()
        .collect()
}

fn runner_inventory_qty(
    positions: &[crate::model::PositionView],
    balances: &[crate::model::BalanceView],
    instrument: &str,
) -> String {
    if let Some(pos) = positions
        .iter()
        .find(|p| matches_instrument(&p.symbol, instrument))
    {
        return pos.quantity.clone();
    }
    let base = instrument_base(instrument);
    balances
        .iter()
        .find(|b| b.asset.eq_ignore_ascii_case(base))
        .map(|b| b.total.clone())
        .unwrap_or_else(|| "0".into())
}

fn inventory_qty<'a>(mut qtys: impl Iterator<Item = &'a str>) -> &'a str {
    qtys.next().unwrap_or("0")
}

fn compare_qty(a: &str, b: &str, tolerance: Decimal) -> Result<bool, String> {
    qty_close(a, b, tolerance)
}

fn apply_source_ages(
    sources: &mut [SourceTime],
    now_ms: i64,
    freshness: &FreshnessConfig,
    reasons: &mut Vec<Reason>,
) {
    for src in sources.iter_mut() {
        src.age_ms = source_age_ms(now_ms, &src.observed_at_utc);
        if src.ok {
            if let Some(age) = src.age_ms {
                if age > freshness.max_source_age_ms {
                    reasons.push(Reason::new(
                        "SOURCE_STALE",
                        format!(
                            "{} age {age}ms exceeds {}",
                            src.source, freshness.max_source_age_ms
                        ),
                    ));
                }
            }
        }
    }
}

fn source_age_ms(now_ms: i64, observed: &Option<String>) -> Option<i64> {
    let raw = observed.as_ref()?;
    let dt = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    Some((now_ms - dt.timestamp_millis()).max(0))
}

pub fn decide_verdict(reasons: &[Reason]) -> Verdict {
    if reasons.iter().any(|r| r.code == "EXPOSURE_DISAGREEMENT") {
        return Verdict::Disagreement;
    }
    if reasons.iter().any(|r| r.code == "BOOK_STALE")
        && reasons
            .iter()
            .all(|r| matches!(r.code.as_str(), "BOOK_STALE" | "SOURCE_STALE"))
    {
        return Verdict::Stale;
    }
    if reasons.iter().any(|r| {
        matches!(
            r.code.as_str(),
            "MONITORING_MISSING"
                | "LEVELS_MISSING"
                | "RISK_UNAVAILABLE"
                | "VENUE_REST_ERROR"
                | "VENUE_PAGINATION_INCOMPLETE"
                | "BOOK_UNAVAILABLE"
                | "IDENTITY_MISMATCH"
                | "IDENTITY_DEFAULT"
                | "JOURNAL_UNAVAILABLE"
                | "JOURNAL_MALFORMED"
                | "JOURNAL_UNRESOLVED"
                | "QTY_MALFORMED"
                | "RISK_INSTRUMENT_MISMATCH"
        )
    }) {
        return Verdict::Unverified;
    }
    if reasons
        .iter()
        .any(|r| r.code == "BOOK_STALE" || r.code == "SOURCE_STALE")
    {
        return Verdict::Stale;
    }
    if reasons.is_empty() {
        Verdict::Verified
    } else {
        Verdict::Unverified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Reason;
    use chrono::TimeZone;

    #[test]
    fn missing_monitoring_is_unverified_not_empty_success() {
        let reasons = vec![Reason::new("MONITORING_MISSING", "no payload")];
        assert_eq!(decide_verdict(&reasons), Verdict::Unverified);
    }

    #[test]
    fn stale_book_is_stale() {
        let reasons = vec![Reason::new("BOOK_STALE", "age")];
        assert_eq!(decide_verdict(&reasons), Verdict::Stale);
    }

    #[test]
    fn disagreement_beats_stale() {
        let reasons = vec![
            Reason::new("BOOK_STALE", "age"),
            Reason::new("EXPOSURE_DISAGREEMENT", "qty"),
        ];
        assert_eq!(decide_verdict(&reasons), Verdict::Disagreement);
    }

    #[test]
    fn default_fields_are_not_verified() {
        let reasons = vec![Reason::new("IDENTITY_DEFAULT", "empty venue")];
        assert_eq!(decide_verdict(&reasons), Verdict::Unverified);
    }

    #[test]
    fn unresolved_journal_is_not_actionable() {
        let reasons = vec![Reason::new("JOURNAL_UNRESOLVED", "submitted")];
        assert_eq!(decide_verdict(&reasons), Verdict::Unverified);
        let report = fixture_subject(FixtureOverrides {
            journal: Some(Ok(vec![crate::model::JournalRow {
                client_order_id: "c1".into(),
                ownership: None,
                state: "submitted".into(),
                created_at_ms: 1,
                last_check_at_ms: 1,
            }])),
            ..FixtureOverrides::default()
        });
        assert_eq!(report.verdict, Verdict::Unverified);
        assert!(!report.actionable_for_analysis);
        assert!(report.reasons.iter().any(|r| r.code == "JOURNAL_UNRESOLVED"));
    }

    #[test]
    fn spot_inventory_mismatch_is_disagreement() {
        let report = fixture_subject(FixtureOverrides {
            venue_balances: Some(Ok(vec![crate::model::BalanceView {
                asset: "BTC".into(),
                total: "0.010".into(),
                free: None,
            }])),
            runner_positions: vec![crate::model::PositionView {
                symbol: "BTC/USDT".into(),
                quantity: "0".into(),
            }],
            ..FixtureOverrides::default()
        });
        assert_eq!(report.verdict, Verdict::Disagreement);
        assert!(report
            .reasons
            .iter()
            .any(|r| r.code == "EXPOSURE_DISAGREEMENT"));
        assert!(report
            .exposure
            .differences
            .iter()
            .any(|d| d.contains("spot base inventory differs")));
    }

    #[test]
    fn order_identities_compare_as_sets() {
        let report = fixture_subject(FixtureOverrides {
            venue_orders: Some(Ok(vec![
                order("b", "BTC/USDT"),
                order("a", "BTC/USDT"),
                order("other", "ETH/USDT"),
            ])),
            runner_orders: vec![order("a", "BTC/USDT"), order("b", "BTC/USDT")],
            ..FixtureOverrides::default()
        });
        assert!(
            !report
                .reasons
                .iter()
                .any(|r| r.code == "EXPOSURE_DISAGREEMENT"),
            "{:?}",
            report.reasons
        );
    }

    #[test]
    fn wrong_instrument_risk_is_unverified() {
        let report = fixture_subject(FixtureOverrides {
            risk_instrument: "ETH/USDT".into(),
            ..FixtureOverrides::default()
        });
        assert_eq!(report.verdict, Verdict::Unverified);
        assert!(report
            .reasons
            .iter()
            .any(|r| r.code == "RISK_INSTRUMENT_MISMATCH"));
    }

    #[test]
    fn stale_source_uses_observation_time() {
        let old = chrono::Utc
            .timestamp_millis_opt(1_700_000_000_000)
            .single()
            .unwrap()
            .to_rfc3339();
        let report = fixture_subject(FixtureOverrides {
            venue_observed_at: old,
            now_ms: 1_700_000_030_000,
            max_source_age_ms: 5_000,
            ..FixtureOverrides::default()
        });
        assert_eq!(report.verdict, Verdict::Stale);
        assert!(report.reasons.iter().any(|r| r.code == "SOURCE_STALE"));
        let venue_age = report
            .trust
            .sources
            .iter()
            .find(|s| s.source == "venue_rest")
            .and_then(|s| s.age_ms);
        assert_eq!(venue_age, Some(30_000));
    }

    #[test]
    fn malformed_rest_quantity_is_unverified() {
        let report = fixture_subject(FixtureOverrides {
            venue_balances: Some(Ok(vec![crate::model::BalanceView {
                asset: "BTC".into(),
                total: "".into(),
                free: None,
            }])),
            runner_positions: vec![crate::model::PositionView {
                symbol: "BTC/USDT".into(),
                quantity: "0".into(),
            }],
            ..FixtureOverrides::default()
        });
        assert_eq!(report.verdict, Verdict::Unverified);
        assert!(report.reasons.iter().any(|r| r.code == "QTY_MALFORMED"));
    }

    fn order(id: &str, symbol: &str) -> crate::model::OrderView {
        crate::model::OrderView {
            order_id: id.into(),
            client_order_id: None,
            symbol: symbol.into(),
            side: "Buy".into(),
            price: "1".into(),
            quantity: "0.001".into(),
            status: "New".into(),
            created_at: None,
        }
    }

    #[derive(Default)]
    struct FixtureOverrides {
        journal: Option<Result<Vec<crate::model::JournalRow>, String>>,
        venue_orders: Option<Result<Vec<crate::model::OrderView>, String>>,
        venue_balances: Option<Result<Vec<crate::model::BalanceView>, String>>,
        runner_orders: Vec<crate::model::OrderView>,
        runner_positions: Vec<crate::model::PositionView>,
        risk_instrument: String,
        venue_observed_at: String,
        now_ms: i64,
        max_source_age_ms: i64,
    }

    fn fixture_subject(over: FixtureOverrides) -> crate::model::SubjectReport {
        use crate::config::{FreshnessConfig, RunnerConfig, VenueSourceConfig};
        use crate::runner_source::proto;

        let now_ms = if over.now_ms == 0 {
            1_700_000_010_000
        } else {
            over.now_ms
        };
        let generated = chrono::Utc
            .timestamp_millis_opt(now_ms)
            .single()
            .unwrap()
            .to_rfc3339();
        let cfg = RunnerConfig {
            label: "paper-levels".into(),
            grpc_endpoint: "http://127.0.0.1:1".into(),
            mode: "levels".into(),
            venue: "PAPER".into(),
            environment: "LOCAL".into(),
            execution_mode: "exchange".into(),
            account_alias: "paper".into(),
            instrument: "BTC/USDT".into(),
            instrument_type: "spot".into(),
            journal_path: std::path::PathBuf::from("/tmp/j.jsonl"),
            executable_revision: None,
            venue_source: VenueSourceConfig {
                kind: "paper".into(),
                rest_base: "http://127.0.0.1:2".into(),
                api_key_env: None,
                api_secret_env: None,
                category: None,
            },
        };
        let freshness = FreshnessConfig {
            max_book_age_ms: 60_000,
            max_source_age_ms: if over.max_source_age_ms == 0 {
                60_000
            } else {
                over.max_source_age_ms
            },
            qty_tolerance: "0.00000001".into(),
        };
        let status = proto::GetTradingStatusResponse {
            enabled: true,
            execution_mode: "exchange".into(),
            venue: "PAPER".into(),
            environment: "LOCAL".into(),
            ..Default::default()
        };
        let mut data = proto::MonitoringData {
            runner_mode: "levels".into(),
            orderbook_source: "ok".into(),
            snapshot_unix_ms: now_ms,
            ..Default::default()
        };
        data.orderbooks.push(proto::Orderbook {
            pair: "BTC/USDT".into(),
            timestamp: now_ms,
            best_bid: "49999".into(),
            best_ask: "50001".into(),
            spread: "2".into(),
            bids: vec![proto::OrderbookLevel {
                price: "49999".into(),
                quantity: "1".into(),
            }],
            asks: vec![proto::OrderbookLevel {
                price: "50001".into(),
                quantity: "1".into(),
            }],
            ..Default::default()
        });
        data.orders = over
            .runner_orders
            .iter()
            .map(|o| proto::Order {
                order_id: o.order_id.clone(),
                symbol: o.symbol.clone(),
                side: o.side.clone(),
                price: o.price.clone(),
                quantity: o.quantity.clone(),
                status: o.status.clone(),
                ..Default::default()
            })
            .collect();
        data.positions = over
            .runner_positions
            .iter()
            .map(|p| proto::Position {
                symbol: p.symbol.clone(),
                quantity: p.quantity.clone(),
                ..Default::default()
            })
            .collect();
        let risk_instrument = if over.risk_instrument.is_empty() {
            "BTC/USDT".into()
        } else {
            over.risk_instrument
        };
        let runner = crate::runner_source::RunnerSnapshot {
            observed_at_utc: generated.clone(),
            status: Ok(status),
            monitoring: Ok(data),
            levels: Ok(Some(vec![])),
            risk: Ok(crate::model::RiskView {
                total_max_position: Some("0.05".into()),
                max_total_orders: Some("8".into()),
                max_notional: None,
                max_order_qty: None,
                daily_loss_limit_all_pairs: None,
                position_qty: "0".into(),
                working_qty: "0".into(),
                reserved_qty: "0".into(),
                daily_pnl: crate::model::Availability::missing("unavailable"),
                headroom: "unknown".into(),
                instrument: risk_instrument,
            }),
            sources: vec![],
        };
        let venue_observed = if over.venue_observed_at.is_empty() {
            generated.clone()
        } else {
            over.venue_observed_at
        };
        let venue = VenueSnapshot {
            observed_at_utc: venue_observed,
            http_methods: vec!["GET".into()],
            orders: over.venue_orders.unwrap_or_else(|| Ok(vec![])),
            balances: over.venue_balances.unwrap_or_else(|| {
                Ok(vec![crate::model::BalanceView {
                    asset: "BTC".into(),
                    total: "0".into(),
                    free: None,
                }])
            }),
            positions: Err("spot /positions unsupported; not proof of flatness".into()),
            positions_supported: false,
        };
        subject_report(
            &cfg,
            &freshness,
            crate::model::MandateView {
                advisory: true,
                allowed_instruments: vec!["BTC/USDT".into()],
                allowed_direction: "both".into(),
                max_leverage: "1".into(),
                max_exposure: "0.05".into(),
                max_loss_per_trade: "50".into(),
                max_loss_per_day: "100".into(),
                time_horizon: "intraday".into(),
                enforced_by_runner: false,
            },
            &generated,
            now_ms,
            &runner,
            &venue,
            over.journal.unwrap_or_else(|| Ok(vec![])),
        )
    }
}
