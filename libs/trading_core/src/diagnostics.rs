//! Stable workflow decision codes and structured tracing helpers.
//!
//! Production logs must carry these codes at decision and durable-state
//! boundaries. Never attach API secrets, operator tokens, or credential-bearing
//! config to the emitted events.

use std::fmt;
use std::time::Instant;

use tracing::field;

/// Stable reason codes for post/hold/risk/ACK/journal/fill/recon/cancel/flatten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReasonCode {
    HoldStaleBook,
    HoldEntriesPaused,
    HoldFlattenLatched,
    HoldTradingDisabled,
    HoldFormingBar,
    HoldNoCandidates,
    RiskRejected,
    AckUncorrelated,
    AckLost,
    JournalWriteFailed,
    JournalTransition,
    FillDuplicate,
    FillApplied,
    LedgerWriteFailed,
    ReconcileDisagreement,
    RestLookupFailed,
    CancelUnverified,
    FlattenUnverified,
    ExposureHalted,
    UnknownCostBasis,
    OwnershipChanged,
    ShadowForbidden,
}

impl ReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HoldStaleBook => "HOLD_STALE_BOOK",
            Self::HoldEntriesPaused => "HOLD_ENTRIES_PAUSED",
            Self::HoldFlattenLatched => "HOLD_FLATTEN_LATCHED",
            Self::HoldTradingDisabled => "HOLD_TRADING_DISABLED",
            Self::HoldFormingBar => "HOLD_FORMING_BAR",
            Self::HoldNoCandidates => "HOLD_NO_CANDIDATES",
            Self::RiskRejected => "RISK_REJECTED",
            Self::AckUncorrelated => "ACK_UNCORRELATED",
            Self::AckLost => "ACK_LOST",
            Self::JournalWriteFailed => "JOURNAL_WRITE_FAILED",
            Self::JournalTransition => "JOURNAL_TRANSITION",
            Self::FillDuplicate => "FILL_DUPLICATE",
            Self::FillApplied => "FILL_APPLIED",
            Self::LedgerWriteFailed => "LEDGER_WRITE_FAILED",
            Self::ReconcileDisagreement => "RECONCILE_DISAGREEMENT",
            Self::RestLookupFailed => "REST_LOOKUP_FAILED",
            Self::CancelUnverified => "CANCEL_UNVERIFIED",
            Self::FlattenUnverified => "FLATTEN_UNVERIFIED",
            Self::ExposureHalted => "EXPOSURE_HALTED",
            Self::UnknownCostBasis => "UNKNOWN_COST_BASIS",
            Self::OwnershipChanged => "OWNERSHIP_CHANGED",
            Self::ShadowForbidden => "SHADOW_FORBIDDEN",
        }
    }

    pub const fn is_hold(self) -> bool {
        matches!(
            self,
            Self::HoldStaleBook
                | Self::HoldEntriesPaused
                | Self::HoldFlattenLatched
                | Self::HoldTradingDisabled
                | Self::HoldFormingBar
                | Self::HoldNoCandidates
        )
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Emit one decision/state-transition event. Callers add correlation fields
/// (`run_id`, `client_order_id`, …) on the same span or at the call site.
pub fn emit_decision(action: &'static str, code: ReasonCode, detail: &str) {
    tracing::info!(
        event = "decision",
        action,
        reason_code = %code,
        detail = %field::display(redact_secrets(detail)),
        "workflow decision"
    );
}

pub fn emit_state_transition(kind: &'static str, code: ReasonCode, detail: &str) {
    tracing::info!(
        event = "state_transition",
        kind,
        reason_code = %code,
        detail = %field::display(redact_secrets(detail)),
        "workflow state transition"
    );
}

/// Strip values that look like tokens, keys, or URIs with userinfo.
pub fn redact_secrets(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    for needle in [
        "api_key",
        "apikey",
        "api_secret",
        "apisecret",
        "secret_key",
        "operator_token",
        "x-operator-token",
        "trading_operator_token",
        "password",
        "mongodb://",
        "mongodb+srv://",
    ] {
        if lower.contains(needle) {
            return "[redacted]".to_string();
        }
    }
    if input.contains("://") && input.contains('@') {
        return "[redacted-uri]".to_string();
    }
    input.to_string()
}

/// Collapses identical hot-path Hold/skip decisions so INFO is not flooded.
#[derive(Debug, Default)]
pub struct HoldAggregator {
    last: Option<(ReasonCode, String)>,
    count: u32,
    first_at: Option<Instant>,
}

impl HoldAggregator {
    pub fn note(&mut self, code: ReasonCode, key: impl Into<String>) {
        let key = key.into();
        match &self.last {
            Some((last_code, last_key)) if *last_code == code && *last_key == key => {
                self.count = self.count.saturating_add(1);
                if should_emit_aggregate(self.count) {
                    tracing::info!(
                        event = "decision_aggregate",
                        action = "hold",
                        reason_code = %code,
                        key = %key,
                        count = self.count,
                        "hold repeated"
                    );
                }
            }
            _ => {
                self.flush();
                self.last = Some((code, key));
                self.count = 1;
                self.first_at = Some(Instant::now());
                tracing::debug!(
                    event = "decision",
                    action = "hold",
                    reason_code = %code,
                    "hold"
                );
            }
        }
    }

    pub fn flush(&mut self) {
        if let Some((code, key)) = self.last.take() {
            if self.count > 1 {
                tracing::info!(
                    event = "decision_aggregate",
                    action = "hold",
                    reason_code = %code,
                    key = %key,
                    count = self.count,
                    elapsed_ms = self
                        .first_at
                        .map(|t| t.elapsed().as_millis() as u64)
                        .unwrap_or(0),
                    "hold aggregate flush"
                );
            }
        }
        self.count = 0;
        self.first_at = None;
    }
}

fn should_emit_aggregate(count: u32) -> bool {
    matches!(count, 8 | 32 | 128) || (count >= 256 && count.is_power_of_two())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_codes_are_stable_screaming_snake() {
        assert_eq!(ReasonCode::AckUncorrelated.as_str(), "ACK_UNCORRELATED");
        assert_eq!(ReasonCode::HoldStaleBook.as_str(), "HOLD_STALE_BOOK");
        assert!(ReasonCode::HoldStaleBook.is_hold());
        assert!(!ReasonCode::RiskRejected.is_hold());
    }

    #[test]
    fn redacts_credential_bearing_text() {
        assert_eq!(redact_secrets("api_key=abc"), "[redacted]");
        assert_eq!(
            redact_secrets("mongodb://user:pass@localhost:27017"),
            "[redacted]"
        );
        assert_eq!(redact_secrets("place uncorrelated"), "place uncorrelated");
    }
}
