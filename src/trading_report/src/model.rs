use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "trading-report-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Verified,
    Stale,
    Unverified,
    Disagreement,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Stale => "stale",
            Self::Unverified => "unverified",
            Self::Disagreement => "disagreement",
        }
    }

    pub fn worse(self, other: Self) -> Self {
        use Verdict::*;
        let rank = |v| match v {
            Verified => 0,
            Stale => 1,
            Unverified => 2,
            Disagreement => 3,
        };
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Availability<T> {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl<T> Availability<T> {
    pub fn ok(value: T) -> Self {
        Self {
            available: true,
            value: Some(value),
            reason: None,
        }
    }

    pub fn missing(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            value: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub code: String,
    pub message: String,
}

impl Reason {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySection {
    pub generated_at_utc: String,
    pub runner_label: String,
    pub runner_mode: Availability<String>,
    pub venue: Availability<String>,
    pub environment: Availability<String>,
    pub execution_mode: Availability<String>,
    pub account_alias: String,
    pub account_alias_venue_verified: bool,
    pub instrument: String,
    pub instrument_type: String,
    pub executable_revision: Option<String>,
    pub config_identity: Availability<String>,
    pub source_times: Vec<SourceTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTime {
    pub source: String,
    pub observed_at_utc: Option<String>,
    pub age_ms: Option<i64>,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketSection {
    pub instrument: String,
    pub best_bid: Availability<String>,
    pub best_ask: Availability<String>,
    pub spread: Availability<String>,
    pub bid_depth: Availability<String>,
    pub ask_depth: Availability<String>,
    pub book_timestamp_utc: Availability<String>,
    pub book_age_ms: Availability<i64>,
    pub completed_candle: Availability<CompletedCandle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedCandle {
    pub start_ms: i64,
    pub end_ms: i64,
    pub close: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExposureSection {
    pub instrument: String,
    pub venue_orders: Availability<Vec<OrderView>>,
    pub venue_balances: Availability<Vec<BalanceView>>,
    pub venue_positions: Availability<Vec<PositionView>>,
    pub runner_orders: Availability<Vec<OrderView>>,
    pub runner_positions: Availability<Vec<PositionView>>,
    pub differences: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderView {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
    pub status: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceView {
    pub asset: String,
    pub total: String,
    pub free: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionView {
    pub symbol: String,
    pub quantity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentSection {
    pub trading_enabled: Availability<bool>,
    pub entries_paused: Availability<bool>,
    pub flatten_latched: Availability<bool>,
    pub exposure_halt_reason: Availability<String>,
    pub strategies: Availability<Vec<StrategyView>>,
    pub levels: Availability<Vec<LevelView>>,
    pub risk: Availability<RiskView>,
    pub mandate: MandateView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyView {
    pub strategy_id: String,
    pub enabled: bool,
    pub last_signal: String,
    pub desired_target: String,
    pub model_reconstructed: bool,
    pub entry_freshness: String,
    pub research_disposition: String,
    pub strategy_version: String,
    pub config_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelView {
    pub id: String,
    pub enabled: bool,
    pub price: String,
    pub quantity: String,
    pub owned_orders: Vec<String>,
    pub fill_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskView {
    pub total_max_position: Option<String>,
    pub max_total_orders: Option<String>,
    pub max_notional: Option<String>,
    pub max_order_qty: Option<String>,
    pub daily_loss_limit_all_pairs: Option<String>,
    pub position_qty: String,
    pub working_qty: String,
    pub reserved_qty: String,
    pub daily_pnl: Availability<String>,
    pub headroom: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MandateView {
    pub advisory: bool,
    pub allowed_instruments: Vec<String>,
    pub allowed_direction: String,
    pub max_leverage: String,
    pub max_exposure: String,
    pub max_loss_per_trade: String,
    pub max_loss_per_day: String,
    pub time_horizon: String,
    pub enforced_by_runner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustSection {
    pub sources: Vec<SourceTime>,
    pub unresolved_submissions: Availability<Vec<JournalRow>>,
    pub verdict: Verdict,
    pub actionable_for_analysis: bool,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRow {
    pub client_order_id: String,
    pub ownership: Option<String>,
    pub state: String,
    pub created_at_ms: u64,
    pub last_check_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectReport {
    pub identity: IdentitySection,
    pub market: MarketSection,
    pub exposure: ExposureSection,
    pub intent: IntentSection,
    pub trust: TrustSection,
    pub verdict: Verdict,
    pub actionable_for_analysis: bool,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingReport {
    pub schema_version: String,
    pub generated_at_utc: String,
    pub subjects: Vec<SubjectReport>,
    pub verdict: Verdict,
    pub actionable_for_analysis: bool,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportManifest {
    pub schema_version: String,
    pub generated_at_utc: String,
    pub source_revisions: Vec<String>,
    pub config_hash: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worst_case_verdict_order() {
        assert_eq!(Verdict::Verified.worse(Verdict::Stale), Verdict::Stale);
        assert_eq!(
            Verdict::Stale.worse(Verdict::Unverified),
            Verdict::Unverified
        );
        assert_eq!(
            Verdict::Unverified.worse(Verdict::Disagreement),
            Verdict::Disagreement
        );
    }

    #[test]
    fn missing_is_not_zero() {
        let qty: Availability<String> = Availability::missing("venue rest failed");
        assert!(!qty.available);
        assert!(qty.value.is_none());
    }
}
