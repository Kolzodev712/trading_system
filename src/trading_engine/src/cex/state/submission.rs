//! Persistent unknown-submission journal and restart recovery by client order ID.
//!
//! JSONL on disk (not Mongo). Persist `Submitted` before the exchange send so a crash
//! cannot lose an in-flight order. Missing lookup stays ambiguous and keeps the reservation.

use super::{PendingSubmission, TradingState};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use trading_core::{
    error::EngineError,
    types::{
        common::{OrderStatus, Pair, Side},
        exchange::{ClientOrderLookup, OrderRequest},
    },
    venue::cex::traits::TradingClient,
};

/// Durable states for a submission whose exchange outcome may be unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionState {
    Prepared,
    Submitted,
    Unknown,
    Acknowledged,
    Rejected,
    Terminal,
    RecoveryRequired,
}

impl SubmissionState {
    pub fn is_unresolved(self) -> bool {
        matches!(
            self,
            Self::Prepared | Self::Submitted | Self::Unknown | Self::RecoveryRequired
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmissionRecord {
    pub client_order_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_id: Option<String>,
    pub symbol_base: String,
    pub symbol_quote: String,
    pub side: String,
    pub amount: String,
    pub price: String,
    pub created_at_ms: u64,
    pub last_check_at_ms: u64,
    pub state: SubmissionState,
}

impl SubmissionRecord {
    pub fn from_pending(pending: &PendingSubmission, state: SubmissionState) -> Self {
        let now = now_ms();
        Self {
            client_order_id: pending.client_order_id.to_string(),
            strategy_id: pending.strategy_id.clone(),
            level_id: pending.level_id.clone(),
            symbol_base: pending.symbol.base.to_string(),
            symbol_quote: pending.symbol.quote.to_string(),
            side: format!("{:?}", pending.side),
            amount: pending.amount.to_string(),
            price: pending.price.to_string(),
            created_at_ms: now,
            last_check_at_ms: now,
            state,
        }
    }

    pub fn to_pending(&self) -> Result<PendingSubmission, EngineError> {
        let amount = self
            .amount
            .parse::<Decimal>()
            .map_err(|err| EngineError::CustomError(smol_str::format_smolstr!("amount: {err}")))?;
        let price = self
            .price
            .parse::<Decimal>()
            .map_err(|err| EngineError::CustomError(smol_str::format_smolstr!("price: {err}")))?;
        let side = match self.side.to_ascii_lowercase().as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            other => {
                return Err(EngineError::CustomError(smol_str::format_smolstr!(
                    "unknown side {other}"
                )))
            }
        };
        Ok(PendingSubmission {
            client_order_id: SmolStr::new(&self.client_order_id),
            symbol: Pair::new(&self.symbol_base, &self.symbol_quote),
            side,
            amount,
            price,
            strategy_id: self.strategy_id.clone(),
            level_id: self.level_id.clone(),
        })
    }

    pub fn with_state(mut self, state: SubmissionState) -> Self {
        self.state = state;
        self.last_check_at_ms = now_ms();
        self
    }
}

/// Append-only JSONL journal. Latest record per `client_order_id` wins on load.
#[derive(Debug)]
pub struct SubmissionJournal {
    path: PathBuf,
    lock: Mutex<()>,
}

impl SubmissionJournal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).map_err(|err| {
                    EngineError::CustomError(smol_str::format_smolstr!(
                        "submission journal dir {}: {err}",
                        parent.display()
                    ))
                })?;
            }
        }
        Ok(Self {
            path,
            lock: Mutex::new(()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, record: &SubmissionRecord) -> Result<(), EngineError> {
        let _guard = self.lock.lock().unwrap_or_else(|err| err.into_inner());
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "open submission journal {}: {err}",
                    self.path.display()
                ))
            })?;
        let line = serde_json::to_string(record).map_err(|err| {
            EngineError::CustomError(smol_str::format_smolstr!("serialize submission: {err}"))
        })?;
        writeln!(file, "{line}").map_err(|err| {
            EngineError::CustomError(smol_str::format_smolstr!("write submission journal: {err}"))
        })?;
        file.flush().map_err(|err| {
            EngineError::CustomError(smol_str::format_smolstr!("flush journal: {err}"))
        })?;
        file.sync_all().map_err(|err| {
            EngineError::CustomError(smol_str::format_smolstr!("fsync journal: {err}"))
        })?;
        Ok(())
    }

    /// Latest record per client order id.
    pub fn load_latest(&self) -> Result<HashMap<String, SubmissionRecord>, EngineError> {
        let _guard = self.lock.lock().unwrap_or_else(|err| err.into_inner());
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .map_err(|err| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "read submission journal {}: {err}",
                    self.path.display()
                ))
            })?;
        let mut latest = HashMap::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|err| {
                EngineError::CustomError(smol_str::format_smolstr!("journal line: {err}"))
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let record: SubmissionRecord = serde_json::from_str(&line).map_err(|err| {
                EngineError::CustomError(smol_str::format_smolstr!(
                    "parse submission journal: {err}"
                ))
            })?;
            latest.insert(record.client_order_id.clone(), record);
        }
        Ok(latest)
    }

    pub fn unresolved(&self) -> Result<Vec<SubmissionRecord>, EngineError> {
        Ok(self
            .load_latest()?
            .into_values()
            .filter(|record| record.state.is_unresolved())
            .collect())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub imported_working: usize,
    pub resolved_terminal: usize,
    pub still_ambiguous: usize,
}

/// Reconcile unresolved journal rows against venue lookup by client order ID.
///
/// Does not send replacement orders. A missing lookup keeps the reservation.
pub async fn recover_unknown_submissions(
    journal: &SubmissionJournal,
    trading_client: &dyn TradingClient<OrderRequest>,
    trading_state: &mut TradingState,
) -> Result<RecoveryReport, EngineError> {
    let mut report = RecoveryReport::default();
    let unresolved = journal.unresolved()?;
    for record in unresolved {
        let pending = record.to_pending()?;
        if !trading_state
            .pending_submissions()
            .contains_key(pending.client_order_id.as_str())
        {
            trading_state.begin_submission(pending.clone());
        }
        let lookup = match trading_client
            .fetch_order_by_client_id(&record.client_order_id)
            .await
        {
            Ok(lookup) => lookup,
            Err(err) => {
                tracing::warn!(
                    client_order_id = %record.client_order_id,
                    "submission lookup failed (keeping reservation): {err}"
                );
                journal.append(&record.with_state(SubmissionState::RecoveryRequired))?;
                report.still_ambiguous += 1;
                continue;
            }
        };
        if matches!(lookup, ClientOrderLookup::Inconclusive)
            && super::operator_flatten::is_operator_flatten(&record)
        {
            if super::operator_flatten::resolve_inconclusive_flatten(
                journal,
                trading_client,
                trading_state,
                &record,
            )
            .await?
            {
                report.resolved_terminal += 1;
                continue;
            }
            journal.append(&record.with_state(SubmissionState::RecoveryRequired))?;
            report.still_ambiguous += 1;
            continue;
        }
        apply_lookup(journal, trading_state, &record, lookup, &mut report)?;
    }
    Ok(report)
}

fn apply_lookup(
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
    record: &SubmissionRecord,
    lookup: ClientOrderLookup,
    report: &mut RecoveryReport,
) -> Result<(), EngineError> {
    match lookup {
        ClientOrderLookup::Working(order) => {
            trading_state.import_working_order(order);
            journal.append(&record.clone().with_state(SubmissionState::Acknowledged))?;
            report.imported_working += 1;
        }
        ClientOrderLookup::Terminal(order) => {
            match order.status {
                OrderStatus::Filled => {
                    trading_state.apply_recovered_fill(&order);
                    trading_state.clear_submission(&record.client_order_id);
                    journal.append(&record.clone().with_state(SubmissionState::Terminal))?;
                }
                OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired => {
                    trading_state.apply_recovered_fill(&order);
                    trading_state.clear_submission(&record.client_order_id);
                    let state = if order.status == OrderStatus::Rejected {
                        SubmissionState::Rejected
                    } else {
                        SubmissionState::Terminal
                    };
                    journal.append(&record.clone().with_state(state))?;
                }
                _ => {
                    trading_state.apply_recovered_fill(&order);
                    trading_state.clear_submission(&record.client_order_id);
                    journal.append(&record.clone().with_state(SubmissionState::Terminal))?;
                }
            }
            report.resolved_terminal += 1;
        }
        ClientOrderLookup::Inconclusive => {
            journal.append(&record.clone().with_state(SubmissionState::RecoveryRequired))?;
            report.still_ambiguous += 1;
        }
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rust_decimal::Decimal;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;
    use trading_core::types::common::{Exchange, Instrument, InstrumentType, OrderId};
    use trading_core::types::exchange::{
        Balance, BulkOrderRequest, BulkOrderResponse, Order, Trade,
    };

    #[derive(Debug)]
    struct ScriptedClient {
        lookups: Mutex<HashMap<String, ClientOrderLookup>>,
        lookup_count: AtomicUsize,
        placed: AtomicUsize,
    }

    impl ScriptedClient {
        fn new(lookups: HashMap<String, ClientOrderLookup>) -> Self {
            Self {
                lookups: Mutex::new(lookups),
                lookup_count: AtomicUsize::new(0),
                placed: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for ScriptedClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            self.placed.fetch_add(1, Ordering::SeqCst);
            Err(EngineError::CustomError(smol_str::format_smolstr!(
                "recovery must not send replacement exposure"
            )))
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_order_by_client_id(
            &self,
            client_order_id: &str,
        ) -> Result<ClientOrderLookup, EngineError> {
            self.lookup_count.fetch_add(1, Ordering::SeqCst);
            let lookups = self.lookups.lock().await;
            Ok(lookups
                .get(client_order_id)
                .cloned()
                .unwrap_or(ClientOrderLookup::Inconclusive))
        }

        async fn fetch_trades(&self, _pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }

        async fn amend_order(
            &self,
            _order_id: OrderId,
            _new_request: OrderRequest,
        ) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_open_orders(
            &self,
            _req: BulkOrderRequest,
        ) -> Result<BulkOrderResponse, EngineError> {
            self.placed.fetch_add(1, Ordering::SeqCst);
            Err(EngineError::CustomError(smol_str::format_smolstr!(
                "recovery must not send replacement exposure"
            )))
        }

        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::CustomError(smol_str::format_smolstr!(
                "recovery must not amend"
            )))
        }
    }

    fn pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn pending(cid: &str) -> PendingSubmission {
        PendingSubmission {
            client_order_id: SmolStr::new(cid),
            symbol: pair(),
            side: Side::Buy,
            amount: Decimal::new(1, 3),
            price: Decimal::new(50000, 0),
            strategy_id: Some("s1".into()),
            level_id: None,
        }
    }

    fn working_order(cid: &str, id: &str) -> Order {
        Order {
            order_id: OrderId::new(id.into()),
            client_order_id: Some(SmolStr::new(cid)),
            instrument: Instrument::new(InstrumentType::Spot, pair()),
            side: Side::Buy,
            price: Decimal::new(50000, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    fn journal_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("inc6-{}-{}.jsonl", name, std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn persist_before_crash_reloads_unresolved() {
        let path = journal_path("persist");
        let journal = SubmissionJournal::open(&path).expect("open");
        let record = SubmissionRecord::from_pending(&pending("cid-1"), SubmissionState::Submitted);
        journal.append(&record).expect("append");
        drop(journal);

        let reopened = SubmissionJournal::open(&path).expect("reopen");
        let unresolved = reopened.unresolved().expect("load");
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].client_order_id, "cid-1");
        assert_eq!(unresolved[0].state, SubmissionState::Submitted);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn recover_imports_working_order_without_duplicate_exposure() {
        let path = journal_path("import");
        let journal = SubmissionJournal::open(&path).expect("open");
        let pending = pending("cid-work");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending,
                SubmissionState::Unknown,
            ))
            .expect("append");

        let mut state = TradingState::new();
        state.begin_submission(pending);
        let reserved = state.reserved_submission_qty(&pair());
        assert_eq!(reserved, Decimal::new(1, 3));
        assert_eq!(state.working_qty(&pair()), Decimal::ZERO);
        let effective = state.effective_position_qty(&pair());

        let order = working_order("cid-work", "ex-1");
        let client = ScriptedClient::new(HashMap::from([(
            "cid-work".into(),
            ClientOrderLookup::Working(order.clone()),
        )]));

        let report = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("recover");
        assert_eq!(report.imported_working, 1);
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert_eq!(state.reserved_submission_qty(&pair()), Decimal::ZERO);
        assert_eq!(state.working_qty(&pair()), Decimal::new(1, 3));
        assert_eq!(state.effective_position_qty(&pair()), effective);
        assert!(state.active_orders.contains_key(&order.order_id));
        let latest = journal.load_latest().expect("latest");
        assert_eq!(
            latest.get("cid-work").map(|r| r.state),
            Some(SubmissionState::Acknowledged)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn missing_lookup_keeps_reservation_and_does_not_resend() {
        let path = journal_path("ambiguous");
        let journal = SubmissionJournal::open(&path).expect("open");
        let pending = pending("cid-miss");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending,
                SubmissionState::Submitted,
            ))
            .expect("append");
        let mut state = TradingState::new();
        state.begin_submission(pending);
        let client = ScriptedClient::new(HashMap::new());

        let report = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("recover");
        assert_eq!(report.still_ambiguous, 1);
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert_eq!(state.reserved_submission_qty(&pair()), Decimal::new(1, 3));
        assert!(state.has_unresolved_submission(&pair()));
        let latest = journal.load_latest().expect("latest");
        assert_eq!(
            latest.get("cid-miss").map(|r| r.state),
            Some(SubmissionState::RecoveryRequired)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn terminal_reject_clears_reservation() {
        let path = journal_path("reject");
        let journal = SubmissionJournal::open(&path).expect("open");
        let pending = pending("cid-rej");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending,
                SubmissionState::Unknown,
            ))
            .expect("append");
        let mut state = TradingState::new();
        state.begin_submission(pending);
        let mut order = working_order("cid-rej", "ex-rej");
        order.status = OrderStatus::Rejected;
        let client = ScriptedClient::new(HashMap::from([(
            "cid-rej".into(),
            ClientOrderLookup::Terminal(order),
        )]));

        let report = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("recover");
        assert_eq!(report.resolved_terminal, 1);
        assert_eq!(state.reserved_submission_qty(&pair()), Decimal::ZERO);
        assert!(!state.has_unresolved_submission(&pair()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_working_is_idempotent() {
        let mut state = TradingState::new();
        let pending = pending("cid-id");
        state.begin_submission(pending);
        let order = working_order("cid-id", "ex-id");
        state.import_working_order(order.clone());
        state.import_working_order(order);
        assert_eq!(state.working_qty(&pair()), Decimal::new(1, 3));
        assert_eq!(state.reserved_submission_qty(&pair()), Decimal::ZERO);
        assert_eq!(state.active_orders.len(), 1);
    }

    fn partial_working_order(cid: &str, id: &str) -> Order {
        let mut order = working_order(cid, id);
        order.status = OrderStatus::PartiallyFilled;
        order.filled_amount = Decimal::new(4, 4);
        order
    }

    #[tokio::test]
    async fn recover_partial_working_fill_is_counted() {
        let path = journal_path("partial-work");
        let journal = SubmissionJournal::open(&path).expect("open");
        let pending = pending("cid-pf");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending,
                SubmissionState::Unknown,
            ))
            .expect("append");
        let mut state = TradingState::new();
        state.begin_submission(pending);
        let reserved = state.reserved_submission_qty(&pair());
        assert_eq!(reserved, Decimal::new(1, 3));

        let order = partial_working_order("cid-pf", "ex-pf");
        let client = ScriptedClient::new(HashMap::from([(
            "cid-pf".into(),
            ClientOrderLookup::Working(order.clone()),
        )]));

        let report = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("recover");
        assert_eq!(report.imported_working, 1);
        assert_eq!(state.position_qty(&pair()), Decimal::new(4, 4));
        assert_eq!(state.working_qty(&pair()), Decimal::new(6, 4));
        assert_eq!(state.reserved_submission_qty(&pair()), Decimal::ZERO);
        assert_eq!(state.effective_position_qty(&pair()), Decimal::new(1, 3));
        assert!(!state.has_unresolved_submission(&pair()));

        let report2 = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("repeat");
        assert_eq!(report2.imported_working, 0);
        state.import_working_order(order);
        assert_eq!(state.position_qty(&pair()), Decimal::new(4, 4));
        assert_eq!(state.effective_position_qty(&pair()), Decimal::new(1, 3));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn recover_partial_canceled_fill_is_counted() {
        let path = journal_path("partial-cxl");
        let journal = SubmissionJournal::open(&path).expect("open");
        let pending = pending("cid-pc");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending,
                SubmissionState::Unknown,
            ))
            .expect("append");
        let mut state = TradingState::new();
        state.begin_submission(pending);
        let mut order = working_order("cid-pc", "ex-pc");
        order.status = OrderStatus::Canceled;
        order.filled_amount = Decimal::new(4, 4);
        let client = ScriptedClient::new(HashMap::from([(
            "cid-pc".into(),
            ClientOrderLookup::Terminal(order.clone()),
        )]));

        let report = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("recover");
        assert_eq!(report.resolved_terminal, 1);
        assert_eq!(state.position_qty(&pair()), Decimal::new(4, 4));
        assert_eq!(state.working_qty(&pair()), Decimal::ZERO);
        assert_eq!(state.reserved_submission_qty(&pair()), Decimal::ZERO);
        assert_eq!(state.effective_position_qty(&pair()), Decimal::new(4, 4));
        assert!(!state.has_unresolved_submission(&pair()));

        recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("repeat journal");
        state.apply_fill(&order);
        assert_eq!(state.position_qty(&pair()), Decimal::new(4, 4));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unwritable_journal_append_fails() {
        let mut path = std::env::temp_dir();
        path.push(format!("inc6-unwritable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("dir");
        let journal = SubmissionJournal::open(&path).expect("open records a path");
        let err = journal
            .append(&SubmissionRecord::from_pending(
                &pending("cid-uw"),
                SubmissionState::Prepared,
            ))
            .expect_err("append to a directory must fail");
        assert!(
            err.to_string().contains("submission journal")
                || err.to_string().contains("Is a directory")
                || err.to_string().contains("open"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn malformed_journal_load_fails() {
        let path = journal_path("torn");
        std::fs::write(&path, "{\"client_order_id\":\"cid-torn\"\nnot-json\n")
            .expect("write torn journal");
        let journal = SubmissionJournal::open(&path).expect("open");
        let err = journal.load_latest().expect_err("parse must fail");
        assert!(
            err.to_string().contains("parse submission journal"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn malformed_journal_blocks_recovery() {
        let path = journal_path("torn-recover");
        std::fs::write(&path, "not-json\n").expect("write");
        let journal = SubmissionJournal::open(&path).expect("open");
        let client = ScriptedClient::new(HashMap::new());
        let mut state = TradingState::new();
        let err = recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect_err("damaged journal must fail recovery");
        assert!(err.to_string().contains("parse submission journal"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn restart_with_damaged_journal_halts_new_exposure() {
        let path = journal_path("restart-torn");
        let journal = SubmissionJournal::open(&path).expect("open");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending("cid-rst"),
                SubmissionState::Submitted,
            ))
            .expect("append");
        drop(journal);

        std::fs::write(&path, "not-json\n").expect("damage after crash");
        let reopened = SubmissionJournal::open(&path).expect("reopen");
        let mut state = TradingState::new();
        let client = ScriptedClient::new(HashMap::new());
        let err = recover_unknown_submissions(&reopened, &client, &mut state)
            .await
            .expect_err("restart recovery must fail on torn journal");
        assert!(err.to_string().contains("parse submission journal"));
        state.halt_new_exposure(format!("submission recovery failed: {err}"));
        assert!(state.blocks_new_exposure(&pair()));
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_file(&path);
    }
}
