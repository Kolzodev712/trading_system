//! Persist-before-send for a single level order. Individual submit: no `zip` on bulk results.
use super::manager::LevelManager;
use super::LevelState;
use crate::cex::state::{
    PendingSubmission, SubmissionJournal, SubmissionRecord, SubmissionState, TradingState,
};
use smol_str::SmolStr;
use trading_core::error::EngineError;
use trading_core::types::exchange::{Order, OrderRequest};
use trading_core::venue::cex::traits::TradingClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelSubmitResult {
    Posted(Order),
    Rejected,
    Unknown,
    Aborted,
    MustCancel(Order),
}

pub struct PreparedLevelSubmit {
    pub pending: PendingSubmission,
    pub generation: u64,
    pub request: OrderRequest,
}

fn persist(
    journal: &SubmissionJournal,
    pending: &PendingSubmission,
    state: SubmissionState,
) -> Result<(), EngineError> {
    journal.append(&SubmissionRecord::from_pending(pending, state))
}

fn place_error_is_known_reject(err: &EngineError) -> bool {
    let lower = err.to_string().to_lowercase();
    lower.contains("lab_reject")
        || lower.contains("duplicate client_order_id")
        || lower.contains("order rejected")
}

pub fn new_level_client_order_id() -> SmolStr {
    SmolStr::new(uuid::Uuid::new_v4().to_string())
}

/// Latch submitting, persist intent, reserve exposure. Does not send.
pub fn prepare_level_submission(
    manager: &mut LevelManager,
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
    level_id: &SmolStr,
    mut request: OrderRequest,
) -> Result<PreparedLevelSubmit, EngineError> {
    if trading_state.blocks_new_exposure(&request.instrument.pair) {
        return Err(EngineError::CustomError(anyhow::anyhow!(
            "new exposure blocked for {}",
            request.instrument.pair
        )));
    }
    let generation = manager.begin_submit(level_id)?;
    let client_order_id = new_level_client_order_id();
    request
        .exchange_params
        .insert(SmolStr::new("orderLinkId"), client_order_id.clone());
    let pending = PendingSubmission {
        client_order_id: client_order_id.clone(),
        symbol: request.instrument.pair.clone(),
        side: request.side.clone(),
        amount: request.amount,
        price: request.price,
        strategy_id: None,
        level_id: Some(level_id.to_string()),
    };
    if let Err(err) = persist(journal, &pending, SubmissionState::Prepared) {
        manager.abort_submit(level_id, generation);
        trading_state.halt_new_exposure(format!("level journal write failed: {err}"));
        return Err(err);
    }
    trading_state.begin_submission(pending.clone());
    if let Err(err) = persist(journal, &pending, SubmissionState::Submitted) {
        trading_state.clear_submission(pending.client_order_id.as_str());
        manager.abort_submit(level_id, generation);
        trading_state.halt_new_exposure(format!("level journal write failed: {err}"));
        return Err(err);
    }
    if let Err(err) = manager.record_inflight_client_id(level_id, &client_order_id) {
        trading_state.halt_new_exposure(format!("level catalog write failed: {err}"));
        return Err(err);
    }
    Ok(PreparedLevelSubmit {
        pending,
        generation,
        request,
    })
}

pub fn abort_unsent_submission(
    manager: &mut LevelManager,
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
    prepared: &PreparedLevelSubmit,
    level_id: &SmolStr,
) -> Result<LevelSubmitResult, EngineError> {
    if let Err(err) = persist(journal, &prepared.pending, SubmissionState::Rejected) {
        trading_state.halt_new_exposure(format!("level journal write failed: {err}"));
        return Err(err);
    }
    if let Err(err) = manager.clear_inflight_client_id(level_id) {
        trading_state.halt_new_exposure(format!("level catalog write failed: {err}"));
        return Err(err);
    }
    trading_state.clear_submission(prepared.pending.client_order_id.as_str());
    manager.abort_submit(level_id, prepared.generation);
    Ok(LevelSubmitResult::Aborted)
}

pub fn apply_level_submit_result(
    manager: &mut LevelManager,
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
    prepared: &PreparedLevelSubmit,
    level_id: &SmolStr,
    result: Result<Order, EngineError>,
) -> Result<LevelSubmitResult, EngineError> {
    match result {
        Ok(mut order) => {
            if order.client_order_id.is_none() {
                order.client_order_id = Some(prepared.pending.client_order_id.clone());
            }
            if let Err(err) = persist(journal, &prepared.pending, SubmissionState::Acknowledged) {
                trading_state.halt_new_exposure(format!("level journal write failed: {err}"));
                return Err(err);
            }
            trading_state.import_working_order(order.clone());
            let keep = manager.submit_still_allowed(level_id, prepared.generation);
            manager.attach_owned_order(
                level_id,
                &prepared.pending.client_order_id,
                &order.order_id,
                keep,
            )?;
            if keep {
                Ok(LevelSubmitResult::Posted(order))
            } else {
                Ok(LevelSubmitResult::MustCancel(order))
            }
        }
        Err(err) if place_error_is_known_reject(&err) => {
            if let Err(persist_err) = persist(journal, &prepared.pending, SubmissionState::Rejected)
            {
                trading_state
                    .halt_new_exposure(format!("level journal write failed: {persist_err}"));
                return Err(persist_err);
            }
            if let Err(persist_err) = manager.clear_inflight_client_id(level_id) {
                trading_state
                    .halt_new_exposure(format!("level catalog write failed: {persist_err}"));
                return Err(persist_err);
            }
            trading_state.clear_submission(prepared.pending.client_order_id.as_str());
            manager.abort_submit(level_id, prepared.generation);
            Ok(LevelSubmitResult::Rejected)
        }
        Err(_) => {
            if let Err(err) = persist(journal, &prepared.pending, SubmissionState::Unknown) {
                trading_state.halt_new_exposure(format!("level journal write failed: {err}"));
                return Err(err);
            }
            Ok(LevelSubmitResult::Unknown)
        }
    }
}

pub async fn send_prepared_level_order(
    trading_client: &dyn TradingClient<OrderRequest>,
    prepared: &PreparedLevelSubmit,
) -> Result<Order, EngineError> {
    trading_client.open_order(prepared.request.clone()).await
}

pub fn attach_recovered_level_orders(
    journal: &SubmissionJournal,
    trading_state: &TradingState,
    manager: &mut LevelManager,
) -> Result<(), EngineError> {
    for record in journal.load_latest()?.into_values() {
        let Some(level_id) = record.level_id.as_ref() else {
            continue;
        };
        let level_id = SmolStr::new(level_id);
        if manager.get_level(&level_id).is_none() {
            continue;
        }
        if let Some(order) = trading_state.active_orders.values().find(|order| {
            order
                .client_order_id
                .as_ref()
                .is_some_and(|cid| cid.as_str() == record.client_order_id)
        }) {
            let keep = manager.get_level(&level_id).is_some_and(|level| {
                level.config.enabled
                    && !matches!(
                        level.state,
                        LevelState::Deleting | LevelState::Paused | LevelState::Removed
                    )
            });
            manager.attach_owned_order(
                &level_id,
                &SmolStr::new(&record.client_order_id),
                &order.order_id,
                keep,
            )?;
        } else if record.state.is_unresolved() {
            if let Some(level) = manager.get_level_mut(&level_id) {
                if !level
                    .owned_client_ids
                    .iter()
                    .any(|id| id.as_str() == record.client_order_id)
                {
                    level
                        .owned_client_ids
                        .push(SmolStr::new(&record.client_order_id));
                }
                level.inflight_client_order_id = Some(SmolStr::new(&record.client_order_id));
                if matches!(level.state, LevelState::Created) {
                    level.state = LevelState::Submitting;
                }
            }
        }
    }
    Ok(())
}

/// Match REST open orders to durable owners. Unowned orders are external: never adopt or cancel.
pub fn classify_open_orders(
    open_orders: &[Order],
    manager: &mut LevelManager,
    trading_state: &mut TradingState,
) {
    for order in open_orders {
        let owned_level = order
            .client_order_id
            .as_ref()
            .and_then(|cid| manager.owns_client_id(cid.as_str()))
            .or_else(|| manager.owns_exchange_id(&order.order_id));
        if let Some(level_id) = owned_level {
            let keep = manager.get_level(&level_id).is_some_and(|level| {
                level.config.enabled
                    && !matches!(
                        level.state,
                        LevelState::Deleting | LevelState::Paused | LevelState::Removed
                    )
            });
            if let Some(cid) = order.client_order_id.as_ref() {
                let _ = manager.attach_owned_order(&level_id, cid, &order.order_id, keep);
            }
            if !trading_state.active_orders.contains_key(&order.order_id) {
                trading_state.import_working_order(order.clone());
            }
        } else {
            manager.mark_external_order(order.order_id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::levels::config::{
        LevelConfig, LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
    };
    use async_trait::async_trait;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use trading_core::types::common::{Exchange, Instrument, InstrumentType, OrderId, Pair, Side};
    use trading_core::types::exchange::{
        Balance, BulkOrderRequest, BulkOrderResponse, ClientOrderLookup, Trade,
    };

    #[derive(Debug)]
    struct ScriptedClient {
        placed: AtomicUsize,
        open_orders: Mutex<Vec<Order>>,
        place_result: Mutex<Option<Result<Order, String>>>,
        lookups: Mutex<std::collections::HashMap<String, ClientOrderLookup>>,
    }

    impl ScriptedClient {
        fn ok_place(order: Order) -> Self {
            Self {
                placed: AtomicUsize::new(0),
                open_orders: Mutex::new(vec![order.clone()]),
                place_result: Mutex::new(Some(Ok(order))),
                lookups: Mutex::new(std::collections::HashMap::new()),
            }
        }

        fn err_place(msg: &str) -> Self {
            Self {
                placed: AtomicUsize::new(0),
                open_orders: Mutex::new(vec![]),
                place_result: Mutex::new(Some(Err(msg.to_string()))),
                lookups: Mutex::new(std::collections::HashMap::new()),
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
            match self.place_result.lock().unwrap().clone() {
                Some(Ok(order)) => Ok(order),
                Some(Err(msg)) => Err(EngineError::CustomError(anyhow::anyhow!(msg))),
                None => Err(EngineError::CustomError(anyhow::anyhow!("no place script"))),
            }
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            panic!("level submit must not cancel_all_orders");
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
        }

        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(self.open_orders.lock().unwrap().clone())
        }

        async fn fetch_order_by_client_id(
            &self,
            client_order_id: &str,
        ) -> Result<ClientOrderLookup, EngineError> {
            let lookups = self.lookups.lock().unwrap();
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
            panic!("level submit must not bulk_open_orders");
        }

        async fn bulk_amend_orders(
            &self,
            _req: trading_core::types::exchange::BulkAmendRequest,
        ) -> Result<trading_core::types::exchange::BulkAmendResponse, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
    }

    fn cfg(id: &str) -> LevelConfig {
        LevelConfig {
            id: SmolStr::new(id),
            name: None,
            side: Side::Buy,
            order_type: OrderTypeConfig::Limit,
            quantity: QuantityConfig::Fixed(dec!(0.001)),
            price: PriceConfig::Fixed(dec!(50000)),
            risk: LevelRiskConfig::default(),
            lifecycle: LifecycleConfig::default(),
            enabled: true,
            group: None,
        }
    }

    fn pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn request() -> OrderRequest {
        OrderRequest {
            instrument: Instrument::new(InstrumentType::Spot, pair()),
            side: Side::Buy,
            order_type: trading_core::types::common::OrderType::Limit,
            price: dec!(50000),
            amount: dec!(0.001),
            timestamp: None,
            exchange_params: Default::default(),
        }
    }

    fn working(cid: &str, oid: &str) -> Order {
        Order {
            order_id: OrderId::new(oid.into()),
            client_order_id: Some(SmolStr::new(cid)),
            instrument: Instrument::new(InstrumentType::Spot, pair()),
            side: Side::Buy,
            price: dec!(50000),
            amount: dec!(0.001),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: trading_core::types::common::OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    fn journal_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lvl-submit-{}-{}-{}.jsonl",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[tokio::test]
    async fn persist_before_send_and_ack_maps_to_level() {
        let path = journal_path("ack");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let order = working("will-replace", "ex-1");
        let client = ScriptedClient::ok_place(order.clone());

        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert_eq!(
            manager.get_level(&level_id).unwrap().state,
            LevelState::Submitting
        );
        assert!(state.has_unresolved_submission(&pair()));

        let mut placed = send_prepared_level_order(&client, &prepared).await.unwrap();
        placed.client_order_id = Some(prepared.pending.client_order_id.clone());
        let result = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            Ok(placed.clone()),
        )
        .unwrap();
        assert!(matches!(result, LevelSubmitResult::Posted(_)));
        let level = manager.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Posted);
        assert_eq!(level.active_orders[0].as_str(), "ex-1");
        assert!(level
            .owned_client_ids
            .iter()
            .any(|id| id == &prepared.pending.client_order_id));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn journal_write_failure_does_not_send() {
        let dir = std::env::temp_dir().join(format!(
            "lvl-submit-unwritable-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let journal = SubmissionJournal::open(&dir).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let client = ScriptedClient::err_place("should not be called");
        let err =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request());
        assert!(err.is_err());
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert_eq!(
            manager.get_level(&level_id).unwrap().state,
            LevelState::Created
        );
        assert!(!state.has_unresolved_submission(&pair()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn disable_after_prepare_aborts_before_send() {
        let path = journal_path("abort");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let client = ScriptedClient::ok_place(working("x", "ex-x"));
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        manager.disable_level(&level_id).await.unwrap();
        assert!(!manager.submit_still_allowed(&level_id, prepared.generation));
        abort_unsent_submission(&mut manager, &journal, &mut state, &prepared, &level_id).unwrap();
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        assert!(!state.has_unresolved_submission(&pair()));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn lost_ack_keeps_reservation_without_replacement() {
        let path = journal_path("lost");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let client = ScriptedClient::err_place("k-paper: open_order timeout (15s)");
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        let err = send_prepared_level_order(&client, &prepared).await;
        let result = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            err,
        )
        .unwrap();
        assert_eq!(result, LevelSubmitResult::Unknown);
        assert!(state.has_unresolved_submission(&pair()));
        assert_eq!(state.reserved_submission_qty(&pair()), dec!(0.001));
        let latest = journal.load_latest().unwrap();
        assert_eq!(
            latest
                .get(prepared.pending.client_order_id.as_str())
                .map(|r| r.state),
            Some(SubmissionState::Unknown)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn known_reject_releases_reservation() {
        let path = journal_path("rej");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let client = ScriptedClient::err_place("order rejected");
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        let err = send_prepared_level_order(&client, &prepared).await;
        let result = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            err,
        )
        .unwrap();
        assert_eq!(result, LevelSubmitResult::Rejected);
        assert!(!state.has_unresolved_submission(&pair()));
        assert_eq!(
            manager.get_level(&level_id).unwrap().state,
            LevelState::Created
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn recover_imports_into_owning_level_not_sibling() {
        let path = journal_path("recover");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let a = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let b = manager.add_level(cfg("buy_level_2"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let pending = PendingSubmission {
            client_order_id: SmolStr::new("cid-a"),
            symbol: pair(),
            side: Side::Buy,
            amount: dec!(0.001),
            price: dec!(50000),
            strategy_id: None,
            level_id: Some(a.to_string()),
        };
        persist(&journal, &pending, SubmissionState::Unknown).unwrap();
        state.begin_submission(pending);
        let order = working("cid-a", "ex-a");
        state.import_working_order(order.clone());
        attach_recovered_level_orders(&journal, &state, &mut manager).unwrap();
        assert_eq!(
            manager.get_level(&a).unwrap().active_orders[0].as_str(),
            "ex-a"
        );
        assert!(manager.get_level(&b).unwrap().active_orders.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rest_unowned_order_is_external_not_adopted() {
        let mut manager = LevelManager::new();
        let mut state = TradingState::new();
        let ours = working("cid-ours", "ex-ours");
        let foreign = working("cid-foreign", "ex-foreign");
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        manager
            .confirm_posted(&level_id, &SmolStr::new("cid-ours"), &ours.order_id)
            .unwrap();
        classify_open_orders(&[ours.clone(), foreign.clone()], &mut manager, &mut state);
        assert!(manager.is_external_order(&foreign.order_id));
        assert!(!manager.is_external_order(&ours.order_id));
        assert!(!manager
            .get_level(&level_id)
            .unwrap()
            .active_orders
            .iter()
            .any(|id| id.as_str() == "ex-foreign"));
    }

    #[tokio::test]
    async fn classify_open_orders_does_not_mark_deleting_as_posted() {
        let mut manager = LevelManager::new();
        let mut state = TradingState::new();
        let order = working("cid-1", "ex-1");
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        manager
            .confirm_posted(&level_id, &SmolStr::new("cid-1"), &order.order_id)
            .unwrap();
        manager.begin_delete(&level_id).unwrap();
        classify_open_orders(&[order], &mut manager, &mut state);
        assert_eq!(
            manager.get_level(&level_id).unwrap().state,
            LevelState::Deleting
        );
    }

    #[test]
    fn duplicate_fill_does_not_double_count() {
        let mut state = TradingState::new();
        let mut order = working("cid-dup", "ex-dup");
        order.status = trading_core::types::common::OrderStatus::Filled;
        order.filled_amount = dec!(0.001);
        state.import_working_order(order.clone());
        state.apply_fill(&order);
        state.apply_fill(&order);
        assert_eq!(state.position_qty(&pair()), dec!(0.001));
    }

    #[tokio::test]
    async fn abort_journal_write_failure_keeps_reservation_and_halts() {
        let path = journal_path("abort-fail");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let err = abort_unsent_submission(&mut manager, &journal, &mut state, &prepared, &level_id);
        assert!(err.is_err());
        assert!(state.has_unresolved_submission(&pair()));
        assert!(state.blocks_new_exposure(&pair()));
        let _ = std::fs::remove_dir_all(&path);
    }

    #[tokio::test]
    async fn concurrent_disable_after_prepare_requires_cancel_on_ack() {
        let path = journal_path("race");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        manager.disable_level(&level_id).await.unwrap();
        let mut placed = working("x", "ex-race");
        placed.client_order_id = Some(prepared.pending.client_order_id.clone());
        let result = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            Ok(placed),
        )
        .unwrap();
        assert!(matches!(result, LevelSubmitResult::MustCancel(_)));
        let level = manager.get_level(&level_id).unwrap();
        assert_ne!(level.state, LevelState::Posted);
        assert!(!level.config.enabled);
        assert!(level
            .active_orders
            .iter()
            .any(|id| id.as_str() == "ex-race"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn delete_during_inflight_submit_keeps_instance_and_must_cancel_on_ack() {
        let path = journal_path("delete-ack-race");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        manager.begin_delete(&level_id).unwrap();
        let err = manager.complete_delete(&level_id).unwrap_err();
        assert!(err.to_string().contains("pending submission"));
        assert!(manager.get_level(&level_id).is_some());

        let mut placed = working("x", "ex-delete-race");
        placed.client_order_id = Some(prepared.pending.client_order_id.clone());
        let result = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            Ok(placed),
        )
        .unwrap();
        assert!(matches!(result, LevelSubmitResult::MustCancel(_)));
        let level = manager.get_level(&level_id).unwrap();
        assert_eq!(level.state, LevelState::Deleting);
        assert!(level
            .active_orders
            .iter()
            .any(|id| id.as_str() == "ex-delete-race"));
        let err = manager.complete_delete(&level_id).unwrap_err();
        assert!(err.to_string().contains("owned orders"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn ack_journal_write_failure_keeps_reservation_and_halts() {
        let path = journal_path("ack-fail");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let mut placed = working("x", "ex-ack-fail");
        placed.client_order_id = Some(prepared.pending.client_order_id.clone());
        let err = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            Ok(placed),
        );
        assert!(err.is_err());
        assert!(state.has_unresolved_submission(&pair()));
        assert!(state.blocks_new_exposure(&pair()));
        let _ = std::fs::remove_dir_all(&path);
    }

    #[tokio::test]
    async fn unknown_journal_write_failure_keeps_reservation_and_halts() {
        let path = journal_path("unknown-fail");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut manager = LevelManager::new();
        let level_id = manager.add_level(cfg("buy_level_1"), pair()).await.unwrap();
        let mut state = TradingState::new();
        let prepared =
            prepare_level_submission(&mut manager, &journal, &mut state, &level_id, request())
                .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let err = apply_level_submit_result(
            &mut manager,
            &journal,
            &mut state,
            &prepared,
            &level_id,
            Err(EngineError::CustomError(anyhow::anyhow!(
                "k-paper: open_order timeout (15s)"
            ))),
        );
        assert!(err.is_err());
        assert!(state.has_unresolved_submission(&pair()));
        assert!(state.blocks_new_exposure(&pair()));
        let _ = std::fs::remove_dir_all(&path);
    }
}
