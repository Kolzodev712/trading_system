//! Operator reduce-only exits: persist client ID and intent before send.
//!
//! Lives in `trading_engine`, not `trading_core`. FlattenAndPause, TRADING_FLATTEN,
//! restart recovery, and the gRPC TSM fallback must use this path.

use super::{
    recover_unknown_submissions, PendingSubmission, SubmissionJournal, SubmissionRecord,
    SubmissionState, TradingState,
};
use rust_decimal::Decimal;
use smol_str::SmolStr;
use trading_core::{
    error::EngineError,
    types::{
        common::{Instrument, InstrumentType, OrderStatus, OrderType, Pair, Side},
        exchange::{ClientOrderLookup, Order, OrderRequest, Position},
    },
    venue::cex::traits::TradingClient,
};

/// Journal `strategy_id` for operator flatten rows (distinct from strategy entries).
pub const OPERATOR_FLATTEN_INTENT: &str = "operator_flatten";

fn flat_eps() -> Decimal {
    Decimal::new(1, 6)
}

pub fn is_operator_flatten(record: &SubmissionRecord) -> bool {
    record.strategy_id.as_deref() == Some(OPERATOR_FLATTEN_INTENT)
}

/// FlattenAndPause holds the pause/flatten latch first. Unverified CancelWorking
/// must not be treated as cancelled working orders, and must not send reduce-only.
pub fn flatten_cancel_working_failed(err: &str) -> String {
    format!(
        "FlattenAndPause: CancelWorking unverified: {err}. \
         Working orders are not treated as cancelled. \
         Pause and flatten latch remain held. Reduce-only exit was not sent."
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperatorFlattenReport {
    pub client_order_ids: Vec<String>,
    pub exchange_order_ids: Vec<String>,
}

/// Place reduce-only market exits for open positions using the submission journal.
///
/// Does not claim flat. Does not retry while an operator-flatten acknowledgement
/// is still ambiguous. Journal append failure does not send.
pub async fn submit_operator_reduce_only_exits(
    journal: &SubmissionJournal,
    trading_client: &dyn TradingClient<OrderRequest>,
    trading_state: &mut TradingState,
    instrument_type: InstrumentType,
    tsm_positions: &[(Pair, Decimal)],
) -> Result<OperatorFlattenReport, EngineError> {
    recover_unknown_submissions(journal, trading_client, trading_state).await?;
    settle_operator_flatten_exits(journal, trading_client, trading_state).await?;

    let unresolved: Vec<SubmissionRecord> = journal
        .unresolved()?
        .into_iter()
        .filter(is_operator_flatten)
        .collect();
    if let Some(record) = unresolved.first() {
        return Err(EngineError::CustomError(anyhow::anyhow!(
            "ambiguous flatten acknowledgement for {}; not retrying until client-id lookup or venue position resolves it",
            record.client_order_id
        )));
    }

    let mut targets = venue_or_tsm_targets(trading_client, instrument_type, tsm_positions).await?;
    let latest = journal.load_latest()?;
    targets.retain(|(instrument, _)| {
        !has_inflight_flatten(&latest, &instrument.pair)
            && !has_working_exit(trading_state, &instrument.pair)
    });

    let mut report = OperatorFlattenReport::default();
    for (instrument, qty) in targets {
        let side = if qty > Decimal::ZERO {
            Side::Sell
        } else {
            Side::Buy
        };
        let amount = qty.abs();
        let client_order_id = uuid::Uuid::new_v4().to_string();
        let pending = PendingSubmission {
            client_order_id: SmolStr::new(&client_order_id),
            symbol: instrument.pair.clone(),
            side: side.clone(),
            amount,
            price: Decimal::ZERO,
            strategy_id: Some(OPERATOR_FLATTEN_INTENT.to_string()),
        };
        persist(journal, trading_state, &pending, SubmissionState::Prepared)?;
        persist(journal, trading_state, &pending, SubmissionState::Submitted)?;
        trading_state.begin_submission(pending.clone());

        let mut exchange_params = std::collections::HashMap::new();
        exchange_params.insert(SmolStr::new("reduceOnly"), SmolStr::new("true"));
        exchange_params.insert(SmolStr::new("orderLinkId"), SmolStr::new(&client_order_id));
        let request = OrderRequest {
            instrument: instrument.clone(),
            side,
            order_type: OrderType::Market,
            price: Decimal::ZERO,
            amount,
            timestamp: None,
            exchange_params,
        };
        match trading_client.open_order(request).await {
            Ok(mut order) => {
                if order.client_order_id.is_none() {
                    order.client_order_id = Some(SmolStr::new(&client_order_id));
                }
                trading_state.import_working_order(order.clone());
                journal.append(&SubmissionRecord::from_pending(
                    &pending,
                    SubmissionState::Acknowledged,
                ))?;
                report.client_order_ids.push(client_order_id);
                report
                    .exchange_order_ids
                    .push(order.order_id.as_str().to_string());
            }
            Err(err) => {
                journal.append(&SubmissionRecord::from_pending(
                    &pending,
                    SubmissionState::Unknown,
                ))?;
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "flatten send unknown after journaled {client_order_id}: {err}"
                )));
            }
        }
    }
    Ok(report)
}

/// REST-check remaining working orders after a cancel acknowledgement.
/// A successful cancelAll is not venue-empty until this query succeeds with none working.
pub async fn verify_rest_working_orders_cleared(
    trading_client: &dyn TradingClient<OrderRequest>,
) -> Result<(), EngineError> {
    match trading_client.fetch_open_orders().await {
        Ok(orders) => {
            let remaining: Vec<_> = orders
                .iter()
                .filter(|order| order.is_working())
                .map(|order| order.order_id.as_str().to_string())
                .collect();
            if remaining.is_empty() {
                Ok(())
            } else {
                Err(EngineError::CustomError(anyhow::anyhow!(
                    "CancelWorking unverified: REST still shows working orders after cancel ({})",
                    remaining.join(",")
                )))
            }
        }
        Err(err) => Err(EngineError::CustomError(anyhow::anyhow!(
            "CancelWorking unverified: REST GET /open_orders failed: {err}"
        ))),
    }
}

pub fn merge_rest_working_orders(
    order_ids: &mut Vec<trading_core::types::common::OrderId>,
    pairs: &mut Vec<Pair>,
    rest: &[Order],
) {
    for order in rest.iter().filter(|order| order.is_working()) {
        if !order_ids.iter().any(|id| id == &order.order_id) {
            order_ids.push(order.order_id.clone());
        }
        if !pairs.iter().any(|pair| pair == &order.instrument.pair) {
            pairs.push(order.instrument.pair.clone());
        }
    }
}

/// Move a journaled flatten off `Acknowledged` once the venue fill or flat position is verified.
pub async fn settle_operator_flatten_exits(
    journal: &SubmissionJournal,
    trading_client: &dyn TradingClient<OrderRequest>,
    trading_state: &mut TradingState,
) -> Result<(), EngineError> {
    let latest = journal.load_latest()?;
    let mut positions_cache: Option<Vec<Position>> = None;
    for record in latest.into_values() {
        if !is_operator_flatten(&record)
            || matches!(
                record.state,
                SubmissionState::Terminal | SubmissionState::Rejected
            )
        {
            continue;
        }
        match trading_client
            .fetch_order_by_client_id(&record.client_order_id)
            .await
        {
            Ok(ClientOrderLookup::Terminal(order)) => {
                trading_state.apply_fill(&order);
                trading_state.clear_submission(&record.client_order_id);
                trading_state.active_orders.remove(&order.order_id);
                journal.append(&record.clone().with_state(match order.status {
                    OrderStatus::Rejected => SubmissionState::Rejected,
                    _ => SubmissionState::Terminal,
                }))?;
            }
            Ok(ClientOrderLookup::Working(order)) => {
                trading_state.import_working_order(order);
            }
            Ok(ClientOrderLookup::Inconclusive) => {
                if positions_cache.is_none() {
                    match trading_client.fetch_positions().await {
                        Ok(positions) => positions_cache = Some(positions),
                        Err(_) => continue,
                    }
                }
                let Some(positions) = positions_cache.as_ref() else {
                    continue;
                };
                if let Ok(pending) = record.to_pending() {
                    if pair_is_flat(positions, &pending.symbol) {
                        trading_state.clear_submission(&record.client_order_id);
                        journal.append(&record.clone().with_state(SubmissionState::Terminal))?;
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    client_order_id = %record.client_order_id,
                    "flatten settle lookup failed (keeping in-flight): {err}"
                );
            }
        }
    }
    Ok(())
}

pub async fn resolve_inconclusive_flatten(
    journal: &SubmissionJournal,
    trading_client: &dyn TradingClient<OrderRequest>,
    trading_state: &mut TradingState,
    record: &SubmissionRecord,
) -> Result<bool, EngineError> {
    let pending = record.to_pending()?;
    match trading_client.fetch_positions().await {
        Ok(positions) if pair_is_flat(&positions, &pending.symbol) => {
            trading_state.clear_submission(&record.client_order_id);
            journal.append(&record.clone().with_state(SubmissionState::Terminal))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn persist(
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
    pending: &PendingSubmission,
    state: SubmissionState,
) -> Result<(), EngineError> {
    journal
        .append(&SubmissionRecord::from_pending(pending, state))
        .map_err(|err| {
            trading_state.halt_new_exposure(format!("operator flatten journal: {err}"));
            err
        })
}

fn has_inflight_flatten(
    latest: &std::collections::HashMap<String, SubmissionRecord>,
    pair: &Pair,
) -> bool {
    latest.values().any(|record| {
        is_operator_flatten(record)
            && record.symbol_base == pair.base.as_str()
            && record.symbol_quote == pair.quote.as_str()
            && !matches!(
                record.state,
                SubmissionState::Terminal | SubmissionState::Rejected
            )
    })
}

fn has_working_exit(state: &TradingState, pair: &Pair) -> bool {
    state
        .active_orders
        .values()
        .any(|order| &order.instrument.pair == pair && order.is_working())
}

fn pair_is_flat(positions: &[Position], pair: &Pair) -> bool {
    positions
        .iter()
        .find(|position| position.pair() == pair)
        .map(|position| position.quantity.abs() <= flat_eps())
        .unwrap_or(true)
}

async fn venue_or_tsm_targets(
    trading_client: &dyn TradingClient<OrderRequest>,
    instrument_type: InstrumentType,
    tsm_positions: &[(Pair, Decimal)],
) -> Result<Vec<(Instrument, Decimal)>, EngineError> {
    let mut targets = Vec::new();
    match trading_client.fetch_positions().await {
        Ok(positions) => {
            for position in positions {
                if position.quantity.abs() > flat_eps() {
                    targets.push((position.instrument.clone(), position.quantity));
                }
            }
        }
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            if !msg.contains("not supported") {
                return Err(err);
            }
        }
    }
    if targets.is_empty() {
        for (pair, qty) in tsm_positions {
            if qty.abs() > flat_eps() {
                targets.push((Instrument::new(instrument_type.clone(), pair.clone()), *qty));
            }
        }
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;
    use trading_core::types::common::InstrumentType;
    use trading_core::types::common::{Exchange, OrderId, OrderStatus};
    use trading_core::types::exchange::{
        Balance, BulkAmendRequest, BulkAmendResponse, BulkOrderRequest, BulkOrderResponse,
        ClientOrderLookup, Order, Trade,
    };

    #[derive(Debug)]
    struct Scripted {
        lookups: Mutex<HashMap<String, ClientOrderLookup>>,
        positions: Mutex<Vec<Position>>,
        placed: AtomicUsize,
        place_err: Mutex<Option<String>>,
        last_link: Mutex<Option<String>>,
        rest_open: Mutex<Vec<Order>>,
    }

    impl Scripted {
        fn new(positions: Vec<Position>) -> Self {
            Self {
                lookups: Mutex::new(HashMap::new()),
                positions: Mutex::new(positions),
                placed: AtomicUsize::new(0),
                place_err: Mutex::new(None),
                last_link: Mutex::new(None),
                rest_open: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for Scripted {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }

        async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
            let link = req
                .exchange_params
                .get("orderLinkId")
                .map(|s| s.to_string())
                .ok_or_else(|| EngineError::CustomError(anyhow::anyhow!("missing orderLinkId")))?;
            *self.last_link.lock().await = Some(link.clone());
            if let Some(err) = self.place_err.lock().await.clone() {
                self.placed.fetch_add(1, Ordering::SeqCst);
                return Err(EngineError::CustomError(anyhow::anyhow!(err)));
            }
            let n = self.placed.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Order {
                order_id: OrderId::new(format!("ex-{n}").into()),
                client_order_id: Some(SmolStr::new(&link)),
                instrument: req.instrument,
                side: req.side,
                price: Decimal::new(1, 0),
                amount: req.amount,
                internal_created_at: 1,
                exchange_created_at: Some(1),
                status: OrderStatus::New,
                filled_amount: Decimal::ZERO,
            })
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
            Ok(self.rest_open.lock().await.clone())
        }

        async fn fetch_order_by_client_id(
            &self,
            client_order_id: &str,
        ) -> Result<ClientOrderLookup, EngineError> {
            Ok(self
                .lookups
                .lock()
                .await
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
            Err(EngineError::NotSupported("test".into()))
        }

        async fn bulk_amend_orders(
            &self,
            _req: BulkAmendRequest,
        ) -> Result<BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }

        async fn fetch_positions(&self) -> Result<Vec<Position>, EngineError> {
            Ok(self.positions.lock().await.clone())
        }
    }

    fn pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn long_pos(qty: Decimal) -> Position {
        Position {
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            quantity: qty,
            avg_price: Decimal::new(1, 0),
            current_price: Decimal::new(1, 0),
            unrealized_pnl: Decimal::ZERO,
            last_update: 1,
            mark_price: None,
            index_price: None,
            realized_pnl: None,
            initial_margin: None,
            maintenance_margin: None,
            leverage: None,
            position_mode: None,
            margin_type: None,
            liquidation_price: None,
        }
    }

    fn journal_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "opflat-{}-{}-{}.jsonl",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn pending_flatten(cid: &str) -> PendingSubmission {
        PendingSubmission {
            client_order_id: SmolStr::new(cid),
            symbol: pair(),
            side: Side::Sell,
            amount: Decimal::new(1, 3),
            price: Decimal::ZERO,
            strategy_id: Some(OPERATOR_FLATTEN_INTENT.to_string()),
        }
    }

    #[test]
    fn cancel_working_failure_keeps_working_unverified() {
        let msg = flatten_cancel_working_failed("REST GET /open_orders failed");
        assert!(msg.contains("not treated as cancelled"));
        assert!(msg.contains("Reduce-only exit was not sent"));
        assert!(msg.contains("latch remain held"));
    }

    #[test]
    fn persist_failure_halts_without_send() {
        let dir = std::env::temp_dir().join(format!("opflat-persist-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let journal = SubmissionJournal::open(&dir).unwrap();
        let mut state = TradingState::new();
        let err = persist(
            &journal,
            &mut state,
            &pending_flatten("flat-halt"),
            SubmissionState::Prepared,
        )
        .expect_err("append to a directory must fail");
        assert!(
            err.to_string().contains("journal") || err.to_string().contains("directory"),
            "unexpected error: {err}"
        );
        assert!(state
            .exposure_halt_reason()
            .unwrap_or("")
            .contains("operator flatten journal"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn journal_failure_does_not_send() {
        let dir = std::env::temp_dir().join(format!("opflat-ro-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let journal = SubmissionJournal::open(&dir).unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let mut state = TradingState::new();
        let err = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[],
        )
        .await
        .expect_err("journal must fail closed");
        assert!(
            err.to_string().contains("journal")
                || err.to_string().contains("directory")
                || err.to_string().contains("Permission"),
            "unexpected error: {err}"
        );
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lost_ack_restart_does_not_duplicate() {
        let path = journal_path("lost-ack");
        let journal = SubmissionJournal::open(&path).unwrap();
        let pending = pending_flatten("flat-1");
        journal
            .append(&SubmissionRecord::from_pending(
                &pending,
                SubmissionState::Submitted,
            ))
            .unwrap();
        let order = Order {
            order_id: OrderId::new("ex-1".into()),
            client_order_id: Some(SmolStr::new("flat-1")),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Sell,
            price: Decimal::new(1, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        client
            .lookups
            .lock()
            .await
            .insert("flat-1".into(), ClientOrderLookup::Working(order));
        let mut state = TradingState::new();
        let report = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[],
        )
        .await
        .expect("recover working flatten");
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert!(report.client_order_ids.is_empty());
        assert_eq!(state.active_orders.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn duplicate_prevention_reuses_unresolved_client_id() {
        let path = journal_path("dup");
        let journal = SubmissionJournal::open(&path).unwrap();
        journal
            .append(&SubmissionRecord::from_pending(
                &pending_flatten("flat-dup"),
                SubmissionState::Unknown,
            ))
            .unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let mut state = TradingState::new();
        let err = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[],
        )
        .await
        .expect_err("must not mint a second client id");
        assert!(err.to_string().contains("flat-dup"));
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn inconclusive_lookup_plus_flat_venue_resolves_without_retry() {
        let path = journal_path("flat-venue");
        let journal = SubmissionJournal::open(&path).unwrap();
        journal
            .append(&SubmissionRecord::from_pending(
                &pending_flatten("flat-2"),
                SubmissionState::Unknown,
            ))
            .unwrap();
        let client = Scripted::new(vec![]);
        let mut state = TradingState::new();
        let report = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[],
        )
        .await
        .expect("flat venue resolves");
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert!(report.client_order_ids.is_empty());
        let latest = journal.load_latest().unwrap();
        assert_eq!(latest["flat-2"].state, SubmissionState::Terminal);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn places_once_with_client_id() {
        let path = journal_path("place");
        let journal = SubmissionJournal::open(&path).unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let mut state = TradingState::new();
        let report = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[],
        )
        .await
        .expect("place");
        assert_eq!(client.placed.load(Ordering::SeqCst), 1);
        assert_eq!(report.client_order_ids.len(), 1);
        assert_eq!(report.exchange_order_ids, vec!["ex-1".to_string()]);
        let link = client.last_link.lock().await.clone().unwrap();
        let latest = journal.load_latest().unwrap();
        assert_eq!(
            latest[&link].strategy_id.as_deref(),
            Some(OPERATOR_FLATTEN_INTENT)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn filled_acknowledged_flatten_becomes_terminal_and_second_exit_sends() {
        let path = journal_path("two-cycle");
        let journal = SubmissionJournal::open(&path).unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        let first = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[],
        )
        .await
        .expect("first flatten");
        assert_eq!(client.placed.load(Ordering::SeqCst), 1);
        let first_id = first.client_order_ids[0].clone();
        assert_eq!(
            journal.load_latest().unwrap()[&first_id].state,
            SubmissionState::Acknowledged
        );

        state.active_orders.clear();
        let filled = Order {
            order_id: OrderId::new("ex-1".into()),
            client_order_id: Some(SmolStr::new(&first_id)),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Sell,
            price: Decimal::new(1, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::Filled,
            filled_amount: Decimal::new(1, 3),
        };
        client
            .lookups
            .lock()
            .await
            .insert(first_id.clone(), ClientOrderLookup::Terminal(filled));
        *client.positions.lock().await = vec![long_pos(Decimal::new(2, 3))];

        let second = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(2, 3))],
        )
        .await
        .expect("second flatten after verified fill");
        assert_eq!(client.placed.load(Ordering::SeqCst), 2);
        assert_eq!(second.client_order_ids.len(), 1);
        assert_ne!(second.client_order_ids[0], first_id);
        let latest = journal.load_latest().unwrap();
        assert_eq!(latest[&first_id].state, SubmissionState::Terminal);
        assert_eq!(
            latest[&second.client_order_ids[0]].state,
            SubmissionState::Acknowledged
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_rest_includes_orders_missing_from_local_cache() {
        let leftover = Order {
            order_id: OrderId::new("cache-miss".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Buy,
            price: Decimal::new(1, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        let mut ids = vec![];
        let mut pairs = vec![];
        merge_rest_working_orders(&mut ids, &mut pairs, &[leftover]);
        assert_eq!(ids.len(), 1);
        assert_eq!(pairs, vec![pair()]);
    }

    #[tokio::test]
    async fn rest_working_after_cancel_is_unverified() {
        let leftover = Order {
            order_id: OrderId::new("still-open".into()),
            client_order_id: None,
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Buy,
            price: Decimal::new(1, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        };
        let client = Scripted::new(vec![]);
        *client.rest_open.lock().await = vec![leftover];
        let err = verify_rest_working_orders_cleared(&client)
            .await
            .expect_err("remaining REST working must fail closed");
        assert!(err.to_string().contains("still shows working"));
        assert!(err.to_string().contains("still-open"));
    }
}
