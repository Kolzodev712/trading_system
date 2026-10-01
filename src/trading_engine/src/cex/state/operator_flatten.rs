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

/// Standalone CancelWorking: cancel ACK is not venue-empty.
pub fn cancel_working_unverified(err: &str) -> String {
    format!(
        "CancelWorking unverified: {err}. \
         Working orders are not treated as cancelled. \
         Pause remains held."
    )
}

/// FlattenAndPause: TSM flat is not venue flat.
pub fn flatten_position_unverified(err: &str) -> String {
    format!(
        "FlattenAndPause: venue position unverified: {err}. \
         Pause and flatten latch remain held. TSM flat is not venue flat."
    )
}

/// FlattenAndPause: a REST position target cannot be skipped without a verified
/// journaled operator exit on the venue working book.
pub fn flatten_exit_unverified(err: &str) -> String {
    format!(
        "FlattenAndPause: venue exit unverified: {err}. \
         Pause and flatten latch remain held. REST position target was not dropped."
    )
}

/// Local TSM `is_working` rows that are not on the paginated REST working book.
/// These are a reconciliation problem, not proof of a live operator exit.
pub fn tsm_working_absent_from_rest(state: &TradingState, rest: &[Order]) -> Vec<String> {
    let rest_ids: std::collections::HashSet<&str> = rest
        .iter()
        .filter(|order| order.is_working())
        .map(|order| order.order_id.as_str())
        .collect();
    let mut ids: Vec<String> = state
        .active_orders
        .values()
        .filter(|order| order.is_working() && !rest_ids.contains(order.order_id.as_str()))
        .map(|order| order.order_id.as_str().to_string())
        .collect();
    ids.sort();
    ids
}

fn tsm_working_ids_for_pair(state: &TradingState, pair: &Pair) -> Vec<String> {
    let mut ids: Vec<String> = state
        .active_orders
        .values()
        .filter(|order| &order.instrument.pair == pair && order.is_working())
        .map(|order| order.order_id.as_str().to_string())
        .collect();
    ids.sort();
    ids
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

    let targets = venue_or_tsm_targets(trading_client, instrument_type, tsm_positions).await?;
    let latest = journal.load_latest()?;
    let inflight_any = latest.values().any(|record| {
        is_operator_flatten(record)
            && !matches!(
                record.state,
                SubmissionState::Terminal | SubmissionState::Rejected
            )
    });
    let rest_open = match trading_client.fetch_open_orders().await {
        Ok(orders) => Some(orders),
        Err(err) if inflight_any => {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                flatten_exit_unverified(&format!(
                    "paginated REST working-order lookup failed: {err}"
                ))
            )));
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "flatten REST working book unverified; no journaled operator exit so REST position targets still send"
            );
            None
        }
    };
    if let Some(rest) = rest_open.as_ref() {
        let stale = tsm_working_absent_from_rest(trading_state, rest);
        if !stale.is_empty() {
            tracing::warn!(
                tsm_working_ids = ?stale,
                rest_working_n = rest.iter().filter(|order| order.is_working()).count(),
                "TSM working IDs absent from paginated REST working book; reconciliation mismatch, not a live operator exit"
            );
        }
    }

    let mut report = OperatorFlattenReport::default();
    for (instrument, qty) in targets {
        let tsm_ids = tsm_working_ids_for_pair(trading_state, &instrument.pair);
        let journal_rec = inflight_operator_flatten(&latest, &instrument.pair);
        tracing::info!(
            pair = %instrument.pair,
            rest_position = %qty.normalize(),
            tsm_working_ids = ?tsm_ids,
            journal_id = journal_rec
                .map(|record| record.client_order_id.as_str())
                .unwrap_or("-"),
            "flatten REST position target"
        );
        if let Some(record) = journal_rec {
            let rest = rest_open.as_deref().unwrap_or(&[]);
            match verify_journaled_exit_on_venue(trading_client, record, rest).await? {
                VenueExit::Live { order_id } => {
                    tracing::info!(
                        pair = %instrument.pair,
                        rest_position = %qty.normalize(),
                        tsm_working_ids = ?tsm_ids,
                        journal_id = %record.client_order_id,
                        venue_order_id = %order_id,
                        reason = "verified venue working journaled operator exit",
                        "flatten exit suppressed"
                    );
                    continue;
                }
                VenueExit::Terminal => {
                    journal.append(&record.clone().with_state(SubmissionState::Terminal))?;
                    trading_state.clear_submission(&record.client_order_id);
                    tracing::info!(
                        pair = %instrument.pair,
                        rest_position = %qty.normalize(),
                        tsm_working_ids = ?tsm_ids,
                        journal_id = %record.client_order_id,
                        reason = "journaled operator exit terminal on venue; REST position remains",
                        "flatten sending reduce-only"
                    );
                }
            }
        } else {
            tracing::info!(
                pair = %instrument.pair,
                rest_position = %qty.normalize(),
                tsm_working_ids = ?tsm_ids,
                journal_id = "-",
                reason = "no unresolved journaled operator exit",
                "flatten sending reduce-only"
            );
        }
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
            level_id: None,
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

/// Venue exposure after flatten. Futures uses REST positions. Spot uses the wallet.
/// REST failure or leftover size is unverified — never completed from TSM alone.
pub async fn verify_rest_exposure_flat(
    trading_client: &dyn TradingClient<OrderRequest>,
    instrument_type: InstrumentType,
    pairs: &[Pair],
) -> Result<(), EngineError> {
    match instrument_type {
        InstrumentType::Futures | InstrumentType::Derivitives => {
            match trading_client.fetch_positions().await {
                Ok(positions) => {
                    let leftover: Vec<String> = positions
                        .iter()
                        .filter(|position| position.quantity.abs() > flat_eps())
                        .map(|position| {
                            format!("{}:{}", position.pair(), position.quantity.normalize())
                        })
                        .collect();
                    if leftover.is_empty() {
                        Ok(())
                    } else {
                        Err(EngineError::CustomError(anyhow::anyhow!(
                            "REST positions still open ({})",
                            leftover.join(",")
                        )))
                    }
                }
                Err(err) => Err(EngineError::CustomError(anyhow::anyhow!(
                    "REST GET /positions failed: {err}"
                ))),
            }
        }
        InstrumentType::Spot => match trading_client.fetch_balance().await {
            Ok(balances) => {
                let leftover = leftover_spot_wallet_assets(&balances, pairs);
                if leftover.is_empty() {
                    Ok(())
                } else {
                    Err(EngineError::CustomError(anyhow::anyhow!(
                        "REST spot wallet still shows base inventory ({})",
                        leftover.join(",")
                    )))
                }
            }
            Err(err) => Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /wallet failed: {err}"
            ))),
        },
    }
}

fn is_spot_quote_asset(symbol: &str) -> bool {
    matches!(
        symbol.to_ascii_uppercase().as_str(),
        "USDT" | "USDC" | "USD"
    )
}

/// Spot flatten proof comes from the wallet, not TSM rows. Empty `pairs`
/// still flags leftover non-quote inventory (TSM-flat is not wallet-flat).
fn leftover_spot_wallet_assets(
    balances: &[trading_core::types::exchange::Balance],
    pairs: &[Pair],
) -> Vec<String> {
    balances
        .iter()
        .filter(|balance| {
            let requested = pairs
                .iter()
                .any(|pair| pair.base.eq_ignore_ascii_case(balance.symbol.as_ref()));
            let inventory = requested || !is_spot_quote_asset(balance.symbol.as_ref());
            inventory && (balance.total.abs() > flat_eps() || balance.free.abs() > flat_eps())
        })
        .map(|balance| format!("{}:{}", balance.symbol, balance.total.normalize()))
        .collect()
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

/// Journal `Terminal`/`Rejected` when a verified fill (or other terminal status)
/// matches an in-flight operator-flatten client ID. Call from the private stream
/// so settlement does not wait for the next FlattenAndPause.
pub fn settle_operator_flatten_on_order(
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
    order: &Order,
) -> Result<bool, EngineError> {
    let Some(cid) = order.client_order_id.as_ref() else {
        return Ok(false);
    };
    let is_terminal = matches!(
        order.status,
        OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired
    );
    if !is_terminal {
        return Ok(false);
    }
    let latest = journal.load_latest()?;
    let Some(record) = latest.get(cid.as_str()) else {
        return Ok(false);
    };
    if !is_operator_flatten(record)
        || matches!(
            record.state,
            SubmissionState::Terminal | SubmissionState::Rejected
        )
    {
        return Ok(false);
    }
    trading_state.clear_submission(cid);
    trading_state.active_orders.remove(&order.order_id);
    let next = match order.status {
        OrderStatus::Rejected => SubmissionState::Rejected,
        _ => SubmissionState::Terminal,
    };
    journal.append(&record.clone().with_state(next))?;
    tracing::info!(
        client_order_id = %cid,
        status = ?order.status,
        "operator flatten settled from verified order"
    );
    Ok(true)
}

/// Journal `Terminal` for in-flight operator flattens when TSM already shows the
/// pair flat and no working exit remains. Covers a missed fill while the account
/// is still flat, before Demo order history (about seven days) expires.
pub fn settle_operator_flatten_when_local_flat(
    journal: &SubmissionJournal,
    trading_state: &mut TradingState,
) -> Result<usize, EngineError> {
    let latest = journal.load_latest()?;
    let mut settled = 0usize;
    for record in latest.into_values() {
        if !is_operator_flatten(&record)
            || matches!(
                record.state,
                SubmissionState::Terminal | SubmissionState::Rejected
            )
        {
            continue;
        }
        let Ok(pending) = record.to_pending() else {
            continue;
        };
        if trading_state.position_sync_pending(&pending.symbol)
            || trading_state.position_disagreement(&pending.symbol)
        {
            continue;
        }
        if has_working_exit(trading_state, &pending.symbol) {
            continue;
        }
        let qty = trading_state
            .get_position(&pending.symbol)
            .map(|position| position.quantity.abs())
            .unwrap_or(Decimal::ZERO);
        if qty > flat_eps() {
            continue;
        }
        trading_state.clear_submission(&record.client_order_id);
        let client_order_id = record.client_order_id.clone();
        journal.append(&record.with_state(SubmissionState::Terminal))?;
        settled += 1;
        tracing::info!(
            client_order_id = %client_order_id,
            pair = %pending.symbol,
            "operator flatten settled while local position is flat"
        );
    }
    Ok(settled)
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

fn inflight_operator_flatten<'a>(
    latest: &'a std::collections::HashMap<String, SubmissionRecord>,
    pair: &Pair,
) -> Option<&'a SubmissionRecord> {
    latest.values().find(|record| {
        is_operator_flatten(record)
            && record.symbol_base == pair.base.as_str()
            && record.symbol_quote == pair.quote.as_str()
            && !matches!(
                record.state,
                SubmissionState::Terminal | SubmissionState::Rejected
            )
    })
}

enum VenueExit {
    Live { order_id: String },
    Terminal,
}

async fn verify_journaled_exit_on_venue(
    trading_client: &dyn TradingClient<OrderRequest>,
    record: &SubmissionRecord,
    rest_open: &[Order],
) -> Result<VenueExit, EngineError> {
    if let Some(order) = rest_open.iter().find(|order| {
        order.is_working()
            && order
                .client_order_id
                .as_deref()
                .is_some_and(|cid| cid == record.client_order_id)
    }) {
        return Ok(VenueExit::Live {
            order_id: order.order_id.as_str().to_string(),
        });
    }
    match trading_client
        .fetch_order_by_client_id(&record.client_order_id)
        .await
    {
        Ok(ClientOrderLookup::Working(order)) => Ok(VenueExit::Live {
            order_id: order.order_id.as_str().to_string(),
        }),
        Ok(ClientOrderLookup::Terminal(_)) => Ok(VenueExit::Terminal),
        Ok(ClientOrderLookup::Inconclusive) => Err(EngineError::CustomError(anyhow::anyhow!(
            flatten_exit_unverified(&format!(
                "journaled operator exit {} not on paginated REST working book and orderLinkId open/history lookup is inconclusive",
                record.client_order_id
            ))
        ))),
        Err(err) => Err(EngineError::CustomError(anyhow::anyhow!(
            flatten_exit_unverified(&format!(
                "REST lookup by orderLinkId {} failed: {err}",
                record.client_order_id
            ))
        ))),
    }
}

fn has_working_exit(state: &TradingState, pair: &Pair) -> bool {
    let qty = state
        .get_position(pair)
        .map(|position| position.quantity)
        .unwrap_or(Decimal::ZERO);
    state.active_orders.values().any(|order| {
        if &order.instrument.pair != pair || !order.is_working() {
            return false;
        }
        if qty > flat_eps() {
            order.side == Side::Sell
        } else if qty < -flat_eps() {
            order.side == Side::Buy
        } else {
            false
        }
    })
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
        lookup_err: Mutex<Option<String>>,
        open_err: Mutex<Option<String>>,
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
                lookup_err: Mutex::new(None),
                open_err: Mutex::new(None),
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
            if let Some(err) = self.open_err.lock().await.clone() {
                return Err(EngineError::CustomError(anyhow::anyhow!(err)));
            }
            Ok(self.rest_open.lock().await.clone())
        }

        async fn fetch_order_by_client_id(
            &self,
            client_order_id: &str,
        ) -> Result<ClientOrderLookup, EngineError> {
            if let Some(err) = self.lookup_err.lock().await.clone() {
                return Err(EngineError::CustomError(anyhow::anyhow!(err)));
            }
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

    #[derive(Debug)]
    struct PositionsFail;

    #[async_trait]
    impl TradingClient<OrderRequest> for PositionsFail {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }
        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }
        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Err(EngineError::CustomError(anyhow::anyhow!("wallet 503")))
        }
        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(vec![])
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
            Err(EngineError::CustomError(anyhow::anyhow!(
                "REST GET /positions: 503"
            )))
        }
    }

    #[derive(Debug)]
    struct SpotWallet {
        balances: Vec<Balance>,
    }

    #[async_trait]
    impl TradingClient<OrderRequest> for SpotWallet {
        fn exchange_id(&self) -> Exchange {
            Exchange::Paper
        }
        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            Ok(())
        }
        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            Err(EngineError::NotSupported("test".into()))
        }
        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            Ok(())
        }
        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            Ok(())
        }
        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(self.balances.clone())
        }
        async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
            Ok(vec![])
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
            level_id: None,
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
        assert!(
            err.to_string().contains("unverified"),
            "must fail closed unverified, got {err}"
        );
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

    fn stale_working_buy() -> Order {
        Order {
            order_id: OrderId::new("stale-entry".into()),
            client_order_id: Some(SmolStr::new("stale-entry-cid")),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Buy,
            price: Decimal::new(41780, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    fn stale_working_sell() -> Order {
        Order {
            order_id: OrderId::new("stale-sell".into()),
            client_order_id: Some(SmolStr::new("stale-sell-cid")),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Sell,
            price: Decimal::ZERO,
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    fn live_operator_exit(cid: &str, exchange_id: &str) -> Order {
        Order {
            order_id: OrderId::new(exchange_id.into()),
            client_order_id: Some(SmolStr::new(cid)),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Sell,
            price: Decimal::ZERO,
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    #[tokio::test]
    async fn stale_cached_entry_does_not_suppress_reduce_only() {
        let path = journal_path("stale-entry");
        let journal = SubmissionJournal::open(&path).unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        state.set_position(pair(), Decimal::new(1, 3), Decimal::new(83556, 0));
        state.import_working_order(stale_working_buy());
        let report = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(1, 3))],
        )
        .await
        .expect("stale cached Buy must not skip flatten");
        assert_eq!(client.placed.load(Ordering::SeqCst), 1);
        assert_eq!(report.client_order_ids.len(), 1);
        let link = client.last_link.lock().await.clone().unwrap();
        let latest = journal.load_latest().unwrap();
        assert_eq!(
            latest[&link].strategy_id.as_deref(),
            Some(OPERATOR_FLATTEN_INTENT)
        );
        assert_eq!(latest[&link].side, "Sell");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn stale_cached_sell_with_rest_working_empty_sends() {
        let path = journal_path("stale-sell");
        let journal = SubmissionJournal::open(&path).unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        state.set_position(pair(), Decimal::new(1, 3), Decimal::new(83556, 0));
        state.import_working_order(stale_working_sell());
        assert_eq!(
            tsm_working_absent_from_rest(&state, &[]),
            vec!["stale-sell".to_string()]
        );
        let report = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(1, 3))],
        )
        .await
        .expect("stale cached Sell with REST working [] must send");
        assert_eq!(client.placed.load(Ordering::SeqCst), 1, "sends");
        assert_eq!(report.client_order_ids.len(), 1);
        let latest = journal.load_latest().unwrap();
        assert!(latest.values().any(|record| record.strategy_id.as_deref()
            == Some(OPERATOR_FLATTEN_INTENT)
            && record.side == "Sell"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn genuine_live_operator_exit_waits() {
        let path = journal_path("live-exit");
        let journal = SubmissionJournal::open(&path).unwrap();
        journal
            .append(&SubmissionRecord::from_pending(
                &pending_flatten("live-exit-cid"),
                SubmissionState::Acknowledged,
            ))
            .unwrap();
        let live = live_operator_exit("live-exit-cid", "ex-live");
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        client.lookups.lock().await.insert(
            "live-exit-cid".into(),
            ClientOrderLookup::Working(live.clone()),
        );
        *client.rest_open.lock().await = vec![live.clone()];
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        state.set_position(pair(), Decimal::new(1, 3), Decimal::new(83556, 0));
        state.import_working_order(live);
        let report = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(1, 3))],
        )
        .await
        .expect("verified live operator exit waits");
        assert_eq!(client.placed.load(Ordering::SeqCst), 0, "waits");
        assert!(report.client_order_ids.is_empty());
        let latest = journal.load_latest().unwrap();
        assert_eq!(latest["live-exit-cid"].state, SubmissionState::Acknowledged);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rest_lookup_failure_fails_closed() {
        let path = journal_path("lookup-fail");
        let journal = SubmissionJournal::open(&path).unwrap();
        journal
            .append(&SubmissionRecord::from_pending(
                &pending_flatten("flat-lookup"),
                SubmissionState::Acknowledged,
            ))
            .unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        *client.lookup_err.lock().await = Some("orderLinkId 503".into());
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        let err = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(1, 3))],
        )
        .await
        .expect_err("REST lookup failure must fail closed");
        assert!(
            err.to_string().contains("unverified"),
            "expected unverified, got {err}"
        );
        assert!(err.to_string().contains("flat-lookup"));
        assert_eq!(client.placed.load(Ordering::SeqCst), 0, "does not send");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rest_working_book_failure_with_inflight_fails_closed() {
        let path = journal_path("open-fail");
        let journal = SubmissionJournal::open(&path).unwrap();
        journal
            .append(&SubmissionRecord::from_pending(
                &pending_flatten("flat-open"),
                SubmissionState::Acknowledged,
            ))
            .unwrap();
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        *client.open_err.lock().await = Some("open orders 503".into());
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        let err = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(1, 3))],
        )
        .await
        .expect_err("REST working-book failure must fail closed");
        assert!(
            err.to_string().contains("unverified"),
            "expected unverified, got {err}"
        );
        assert!(err.to_string().contains("working-order"));
        assert_eq!(client.placed.load(Ordering::SeqCst), 0, "does not send");
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

    fn filled_sell(cid: &str, exchange_id: &str) -> Order {
        Order {
            order_id: OrderId::new(exchange_id.into()),
            client_order_id: Some(SmolStr::new(cid)),
            instrument: Instrument::new(InstrumentType::Futures, pair()),
            side: Side::Sell,
            price: Decimal::new(1, 0),
            amount: Decimal::new(1, 3),
            internal_created_at: 1,
            exchange_created_at: Some(1),
            status: OrderStatus::Filled,
            filled_amount: Decimal::new(1, 3),
        }
    }

    #[test]
    fn acknowledged_flatten_settles_on_verified_fill_without_next_flatten() {
        let path = journal_path("fill-settle");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        let pending = pending_flatten("flatten-fill-1");
        persist(
            &journal,
            &mut state,
            &pending,
            SubmissionState::Acknowledged,
        )
        .unwrap();
        let filled = filled_sell("flatten-fill-1", "ex-fill");
        assert!(settle_operator_flatten_on_order(&journal, &mut state, &filled).unwrap());
        assert_eq!(
            journal.load_latest().unwrap()["flatten-fill-1"].state,
            SubmissionState::Terminal
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn acknowledged_flatten_settles_when_local_position_is_flat() {
        let path = journal_path("flat-settle");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        persist(
            &journal,
            &mut state,
            &pending_flatten("flatten-flat-1"),
            SubmissionState::Acknowledged,
        )
        .unwrap();
        state.set_position(pair(), Decimal::ZERO, Decimal::ZERO);
        assert_eq!(
            settle_operator_flatten_when_local_flat(&journal, &mut state).unwrap(),
            1
        );
        assert_eq!(
            journal.load_latest().unwrap()["flatten-flat-1"].state,
            SubmissionState::Terminal
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn local_flat_does_not_settle_while_position_sync_pending() {
        let path = journal_path("flat-pending");
        let journal = SubmissionJournal::open(&path).unwrap();
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        persist(
            &journal,
            &mut state,
            &pending_flatten("flatten-pending-1"),
            SubmissionState::Acknowledged,
        )
        .unwrap();
        state.mark_position_sync_pending(pair());
        assert_eq!(
            settle_operator_flatten_when_local_flat(&journal, &mut state).unwrap(),
            0
        );
        assert_eq!(
            journal.load_latest().unwrap()["flatten-pending-1"].state,
            SubmissionState::Acknowledged
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn recover_then_settle_terminals_acknowledged_flatten_without_new_send() {
        let path = journal_path("recover-settle");
        let journal = SubmissionJournal::open(&path).unwrap();
        let client = Scripted::new(vec![]);
        let mut state = TradingState::new().with_instrument_type(InstrumentType::Futures);
        persist(
            &journal,
            &mut state,
            &pending_flatten("flatten-recover-1"),
            SubmissionState::Acknowledged,
        )
        .unwrap();
        client.lookups.lock().await.insert(
            "flatten-recover-1".into(),
            ClientOrderLookup::Terminal(filled_sell("flatten-recover-1", "ex-recover")),
        );
        recover_unknown_submissions(&journal, &client, &mut state)
            .await
            .expect("recover skips acknowledged");
        assert_eq!(
            journal.load_latest().unwrap()["flatten-recover-1"].state,
            SubmissionState::Acknowledged
        );
        settle_operator_flatten_exits(&journal, &client, &mut state)
            .await
            .expect("settle on recover");
        assert_eq!(
            journal.load_latest().unwrap()["flatten-recover-1"].state,
            SubmissionState::Terminal
        );
        assert_eq!(client.placed.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn filled_then_flat_then_new_position_uses_new_client_id() {
        let path = journal_path("fill-then-reopen");
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
        let first_id = first.client_order_ids[0].clone();
        let filled = filled_sell(&first_id, "ex-1");
        assert!(settle_operator_flatten_on_order(&journal, &mut state, &filled).unwrap());
        *client.positions.lock().await = vec![long_pos(Decimal::new(2, 3))];
        let second = submit_operator_reduce_only_exits(
            &journal,
            &client,
            &mut state,
            InstrumentType::Futures,
            &[(pair(), Decimal::new(2, 3))],
        )
        .await
        .expect("second flatten after fill settle");
        assert_eq!(client.placed.load(Ordering::SeqCst), 2);
        assert_ne!(second.client_order_ids[0], first_id);
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

    #[tokio::test]
    async fn rest_futures_position_leftover_is_unverified() {
        let client = Scripted::new(vec![long_pos(Decimal::new(1, 3))]);
        let err = verify_rest_exposure_flat(&client, InstrumentType::Futures, &[])
            .await
            .expect_err("TSM-flat REST-nonflat must fail");
        assert!(err.to_string().contains("still open"));
    }

    #[tokio::test]
    async fn rest_futures_positions_error_is_unverified() {
        let err = verify_rest_exposure_flat(&PositionsFail, InstrumentType::Futures, &[])
            .await
            .expect_err("REST error is unverified");
        assert!(err.to_string().contains("GET /positions failed"));
    }

    #[tokio::test]
    async fn rest_spot_wallet_leftover_is_unverified() {
        let client = SpotWallet {
            balances: vec![Balance {
                symbol: "BTC".into(),
                free: Decimal::new(1, 3),
                used: Decimal::ZERO,
                total: Decimal::new(1, 3),
            }],
        };
        let err = verify_rest_exposure_flat(&client, InstrumentType::Spot, &[pair()])
            .await
            .expect_err("spot wallet leftover must fail");
        assert!(err.to_string().contains("wallet still shows"));
    }

    #[tokio::test]
    async fn rest_spot_wallet_leftover_without_tsm_pairs_is_unverified() {
        let client = SpotWallet {
            balances: vec![
                Balance {
                    symbol: "USDT".into(),
                    free: Decimal::new(100, 0),
                    used: Decimal::ZERO,
                    total: Decimal::new(100, 0),
                },
                Balance {
                    symbol: "BTC".into(),
                    free: Decimal::new(1, 3),
                    used: Decimal::ZERO,
                    total: Decimal::new(1, 3),
                },
            ],
        };
        let err = verify_rest_exposure_flat(&client, InstrumentType::Spot, &[])
            .await
            .expect_err("TSM-empty must still fail when wallet holds BTC");
        assert!(err.to_string().contains("BTC"));
        let leftover = leftover_spot_wallet_assets(
            &[
                Balance {
                    symbol: "USDT".into(),
                    free: Decimal::new(100, 0),
                    used: Decimal::ZERO,
                    total: Decimal::new(100, 0),
                },
                Balance {
                    symbol: "BTC".into(),
                    free: Decimal::new(1, 3),
                    used: Decimal::ZERO,
                    total: Decimal::new(1, 3),
                },
            ],
            &[],
        );
        assert!(leftover.iter().any(|row| row.starts_with("BTC:")));
        assert!(!leftover.iter().any(|row| row.starts_with("USDT:")));
    }

    #[tokio::test]
    async fn rest_spot_wallet_error_is_unverified() {
        let err = verify_rest_exposure_flat(&PositionsFail, InstrumentType::Spot, &[pair()])
            .await
            .expect_err("wallet REST error is unverified");
        assert!(err.to_string().contains("GET /wallet failed"));
    }

    #[tokio::test]
    async fn rest_futures_empty_positions_is_flat() {
        let client = Scripted::new(vec![]);
        verify_rest_exposure_flat(&client, InstrumentType::Futures, &[])
            .await
            .expect("empty REST positions are flat");
    }
}
