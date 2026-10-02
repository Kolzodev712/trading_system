use rust_decimal::Decimal;

use crate::config::{FreshnessConfig, RunnerConfig};
use crate::journal_source::unresolved_rows;
use crate::model::{
    Availability, ExposureSection, IdentitySection, IntentSection, JournalRow, MarketSection,
    Reason, SourceTime, SubjectReport, TrustSection, Verdict,
};
use crate::runner_source::{
    age_ms, completed_candle, epoch_ms_to_utc, proto_orders, proto_positions, proto_strategies,
    RunnerSnapshot,
};
use crate::venue_source::{normalize_symbol, qty_close, VenueSnapshot};

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
        age_ms: Some(0),
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

    let identity = IdentitySection {
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
                age_ms: Some(0),
                ok: true,
                error: None,
            });
            Availability::ok(unresolved_rows(rows))
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
        Ok(d) => Availability::ok(proto_orders(d)),
        Err(e) => Availability::missing((*e).clone()),
    };
    let runner_positions = match monitoring {
        Ok(d) => Availability::ok(proto_positions(d)),
        Err(e) => Availability::missing((*e).clone()),
    };
    let venue_orders = match &venue.orders {
        Ok(v) => Availability::ok(v.clone()),
        Err(e) => Availability::missing(e.clone()),
    };
    let venue_balances = match &venue.balances {
        Ok(v) => Availability::ok(v.clone()),
        Err(e) => Availability::missing(e.clone()),
    };
    let venue_positions = match &venue.positions {
        Ok(v) => Availability::ok(v.clone()),
        Err(e) => Availability::missing(e.clone()),
    };

    let mut differences = Vec::new();
    if let (Some(vo), Some(ro)) = (&venue_orders.value, &runner_orders.value) {
        let v_ids: Vec<_> = vo.iter().map(|o| o.order_id.clone()).collect();
        let r_ids: Vec<_> = ro.iter().map(|o| o.order_id.clone()).collect();
        if v_ids != r_ids {
            differences.push(format!(
                "working order ids differ: venue {:?} runner {:?}",
                v_ids, r_ids
            ));
        }
    }
    if venue.positions_supported {
        if let (Some(vp), Some(rp)) = (&venue_positions.value, &runner_positions.value) {
            let tol = freshness
                .qty_tolerance
                .parse::<Decimal>()
                .unwrap_or(Decimal::new(1, 8));
            let vq = vp
                .iter()
                .filter(|p| normalize_symbol(&p.symbol) == normalize_symbol(&cfg.instrument))
                .map(|p| p.quantity.as_str())
                .next()
                .unwrap_or("0");
            let rq = rp
                .iter()
                .filter(|p| normalize_symbol(&p.symbol) == normalize_symbol(&cfg.instrument))
                .map(|p| p.quantity.as_str())
                .next()
                .unwrap_or("0");
            if !qty_close(vq, rq, tol) {
                differences.push(format!("position qty differs: venue {vq} runner {rq}"));
            }
        }
    } else if cfg.instrument_type.eq_ignore_ascii_case("spot") {
        if let Some(bals) = &venue_balances.value {
            let base = cfg.instrument.split(['/', '-']).next().unwrap_or("");
            if let Some(bal) = bals.iter().find(|b| b.asset.eq_ignore_ascii_case(base)) {
                differences.push(format!(
                    "spot base inventory from balances: {} {}",
                    bal.asset, bal.total
                ));
            }
        }
    }
    if !differences.iter().any(|d| d.contains("differ"))
        && venue_orders.available
        && runner_orders.available
        && (!venue.positions_supported || (venue_positions.available && runner_positions.available))
    {
        // no disagreement
    } else if differences.iter().any(|d| d.contains("differ")) {
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
            Ok(risk) => Availability::ok(risk.clone()),
            Err(e) => Availability::missing(e.clone()),
        },
        mandate,
    }
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
}
