//! Shadow execution: journal, snapshots, and a trading client that cannot place orders.

use rust_decimal::Decimal;
use serde::Serialize;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{
    error::EngineError,
    types::{
        common::{Exchange, OrderId, Pair},
        ExecutionMode,
    },
    venue::cex::traits::TradingClient,
    venue::cex::types::exchange::{
        BulkAmendRequest, BulkAmendResponse, BulkCancelRequest, BulkCancelResponse,
        BulkOrderRequest, BulkOrderResponse, Order, OrderRequest, Trade,
    },
};

pub const OPERATIONAL_ROLE_SHADOW: &str = "SHADOW";
pub const OPERATIONAL_ROLE_BASELINE: &str = "BASELINE";

/// Operator-visible model fields. Account quantities stay on TradingState.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyRuntimeSnapshot {
    pub strategy_id: String,
    pub strategy_version: String,
    pub config_hash: String,
    pub readiness: String,
    pub readiness_reason: String,
    pub last_completed_start_ms: Option<u64>,
    pub last_completed_end_ms: Option<u64>,
    pub last_close: Option<Decimal>,
    pub geometric_mean: Option<Decimal>,
    pub log_spread: Option<f64>,
    pub model_direction: String,
    pub previous_direction: String,
    pub desired_target: String,
    pub fresh_entry_eligible: bool,
    pub recovery: String,
    pub research_disposition: String,
    pub operational_role: String,
    pub model_reconstructed: bool,
}

/// Operator role comes from strategy metadata, never from `ExecutionMode`.
pub fn operational_role_from_snapshot(snapshot: Option<&StrategyRuntimeSnapshot>) -> String {
    snapshot
        .map(|s| s.operational_role.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct ShadowDecisionRecord {
    pub strategy_id: String,
    pub strategy_version: String,
    pub config_hash: String,
    pub candle_start_ms: Option<u64>,
    pub candle_end_exclusive_ms: Option<u64>,
    pub close: Option<String>,
    pub geometric_mean: Option<String>,
    pub log_spread: Option<f64>,
    pub previous_direction: String,
    pub direction: String,
    pub desired_target: String,
    pub decision_timestamp_ms: u64,
    pub account_filled_qty: String,
    pub working_qty: String,
    pub reserved_qty: String,
    pub effective_qty: String,
    pub entry_freshness: String,
    pub execution_mode: String,
    pub reason: String,
    pub recovery: String,
    pub readiness: String,
    pub would_have_requested_orders: usize,
}

#[derive(Debug, Clone)]
pub struct ShadowJournal {
    path: PathBuf,
}

impl ShadowJournal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, record: &ShadowDecisionRecord) -> Result<(), EngineError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).map_err(|err| {
                    EngineError::new_custom_error(format!("shadow journal dir: {err}").into())
                })?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| {
                EngineError::new_custom_error(format!("shadow journal open: {err}").into())
            })?;
        let mut line = serde_json::to_vec(record).map_err(|err| {
            EngineError::new_custom_error(format!("shadow journal encode: {err}").into())
        })?;
        line.push(b'\n');
        file.write_all(&line).map_err(|err| {
            EngineError::new_custom_error(format!("shadow journal write: {err}").into())
        })?;
        Ok(())
    }
}

fn shadow_forbidden(method: &str) -> EngineError {
    EngineError::new_custom_error(format!("shadow mode forbids exchange {method}").into())
}

/// Forwards market/account reads. Every order mutation fails immediately.
pub struct ShadowGuardClient {
    inner: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    forbidden_calls: Arc<AtomicU64>,
}

impl std::fmt::Debug for ShadowGuardClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShadowGuardClient")
            .field(
                "forbidden_calls",
                &self.forbidden_calls.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl ShadowGuardClient {
    pub fn wrap(
        inner: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    ) -> Arc<RwLock<dyn TradingClient<OrderRequest>>> {
        Arc::new(RwLock::new(Self {
            inner,
            forbidden_calls: Arc::new(AtomicU64::new(0)),
        }))
    }

    pub fn wrap_counted(
        inner: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        forbidden_calls: Arc<AtomicU64>,
    ) -> Arc<RwLock<dyn TradingClient<OrderRequest>>> {
        Arc::new(RwLock::new(Self {
            inner,
            forbidden_calls,
        }))
    }

    fn bump_forbidden(&self) {
        self.forbidden_calls.fetch_add(1, Ordering::Relaxed);
    }
}

#[async_trait::async_trait]
impl TradingClient<OrderRequest> for ShadowGuardClient {
    fn exchange_id(&self) -> Exchange {
        // Best-effort; tests use a dummy inner.
        Exchange::Bybit
    }

    async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("test_open_order"))
    }

    async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("open_order"))
    }

    async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("cancel_order_by_id"))
    }

    async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("cancel_all_orders"))
    }

    async fn fetch_balance(
        &self,
    ) -> Result<Vec<trading_core::venue::cex::types::exchange::Balance>, EngineError> {
        self.inner.read().await.fetch_balance().await
    }

    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        self.inner.read().await.fetch_open_orders().await
    }

    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        self.inner.read().await.fetch_trades(pair).await
    }

    async fn fetch_market_data_snapshot(&self, pair: &Pair) -> Result<(), EngineError> {
        self.inner
            .read()
            .await
            .fetch_market_data_snapshot(pair)
            .await
    }

    async fn amend_order(
        &self,
        _order_id: OrderId,
        _new_request: OrderRequest,
    ) -> Result<Order, EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("amend_order"))
    }

    async fn bulk_open_orders(
        &self,
        _req: BulkOrderRequest,
    ) -> Result<BulkOrderResponse, EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("bulk_open_orders"))
    }

    async fn bulk_amend_orders(
        &self,
        _req: BulkAmendRequest,
    ) -> Result<BulkAmendResponse, EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("bulk_amend_orders"))
    }

    async fn bulk_cancel_orders(
        &self,
        _req: BulkCancelRequest,
    ) -> Result<BulkCancelResponse, EngineError> {
        self.bump_forbidden();
        Err(shadow_forbidden("bulk_cancel_orders"))
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn record_from_snapshot(
    snapshot: &StrategyRuntimeSnapshot,
    execution_mode: ExecutionMode,
    filled: Decimal,
    working: Decimal,
    reserved: Decimal,
    effective: Decimal,
    would_have_requested_orders: usize,
) -> ShadowDecisionRecord {
    ShadowDecisionRecord {
        strategy_id: snapshot.strategy_id.clone(),
        strategy_version: snapshot.strategy_version.clone(),
        config_hash: snapshot.config_hash.clone(),
        candle_start_ms: snapshot.last_completed_start_ms,
        candle_end_exclusive_ms: snapshot.last_completed_end_ms,
        close: snapshot.last_close.map(|d| d.to_string()),
        geometric_mean: snapshot.geometric_mean.map(|d| d.to_string()),
        log_spread: snapshot.log_spread,
        previous_direction: snapshot.previous_direction.clone(),
        direction: snapshot.model_direction.clone(),
        desired_target: snapshot.desired_target.clone(),
        decision_timestamp_ms: now_ms(),
        account_filled_qty: filled.to_string(),
        working_qty: working.to_string(),
        reserved_qty: reserved.to_string(),
        effective_qty: effective.to_string(),
        entry_freshness: if snapshot.fresh_entry_eligible {
            "fresh_eligible".to_string()
        } else {
            "not_eligible".to_string()
        },
        execution_mode: execution_mode.as_str().to_string(),
        reason: snapshot.readiness_reason.clone(),
        recovery: snapshot.recovery.clone(),
        readiness: snapshot.readiness.clone(),
        would_have_requested_orders,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::venue::cex::types::exchange::Balance;

    #[derive(Debug)]
    struct DummyClient;

    #[async_trait::async_trait]
    impl TradingClient<OrderRequest> for DummyClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Bybit
        }

        async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
            panic!("inner test_open_order");
        }

        async fn open_order(&self, _req: OrderRequest) -> Result<Order, EngineError> {
            panic!("inner open_order");
        }

        async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
            panic!("inner cancel");
        }

        async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
            panic!("inner cancel_all");
        }

        async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
            Ok(vec![])
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
            panic!("inner amend");
        }

        async fn bulk_open_orders(
            &self,
            _req: BulkOrderRequest,
        ) -> Result<BulkOrderResponse, EngineError> {
            panic!("inner bulk_open");
        }

        async fn bulk_amend_orders(
            &self,
            _req: BulkAmendRequest,
        ) -> Result<BulkAmendResponse, EngineError> {
            panic!("inner bulk_amend");
        }

        async fn bulk_cancel_orders(
            &self,
            _req: BulkCancelRequest,
        ) -> Result<BulkCancelResponse, EngineError> {
            panic!("inner bulk_cancel");
        }
    }

    #[tokio::test]
    async fn shadow_guard_rejects_order_methods_without_calling_inner() {
        let calls = Arc::new(AtomicU64::new(0));
        let client =
            ShadowGuardClient::wrap_counted(Arc::new(RwLock::new(DummyClient)), calls.clone());
        let guard = client.read().await;
        let err = guard
            .open_order(OrderRequest {
                instrument: trading_core::types::common::Instrument::new(
                    trading_core::types::common::InstrumentType::Futures,
                    Pair::new("BTC", "USDT"),
                ),
                side: trading_core::types::common::Side::Buy,
                order_type: trading_core::types::common::OrderType::Market,
                price: Decimal::ONE,
                amount: Decimal::ONE,
                timestamp: None,
                exchange_params: Default::default(),
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("shadow mode forbids"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(guard
            .cancel_all_orders(&Pair::new("BTC", "USDT"))
            .await
            .unwrap_err()
            .to_string()
            .contains("shadow mode forbids"));
        assert_eq!(
            calls.load(Ordering::Relaxed),
            2,
            "guard counts forbidden attempts; DummyClient panics if the inner exchange is reached"
        );
        assert!(guard.fetch_balance().await.is_ok());
        let open = guard.fetch_open_orders().await.expect("exchange-side read");
        assert!(
            open.is_empty(),
            "guarded fetch_open_orders is the venue read; it must not invent orders: {open:?}"
        );
    }

    #[test]
    fn operational_role_is_not_inferred_from_execution_mode() {
        assert_eq!(operational_role_from_snapshot(None), "");
        let snap = StrategyRuntimeSnapshot {
            strategy_id: "BTCPriceMALongFlatV1:BTC/USDT".into(),
            strategy_version: "1".into(),
            config_hash: "h".into(),
            readiness: "ready".into(),
            readiness_reason: "ok".into(),
            last_completed_start_ms: None,
            last_completed_end_ms: None,
            last_close: None,
            geometric_mean: None,
            log_spread: None,
            model_direction: "long".into(),
            previous_direction: "flat".into(),
            desired_target: "0".into(),
            fresh_entry_eligible: false,
            recovery: "clear".into(),
            research_disposition: "REJECTED AS VALIDATED ALPHA".into(),
            operational_role: OPERATIONAL_ROLE_BASELINE.to_string(),
            model_reconstructed: true,
        };
        assert_eq!(
            operational_role_from_snapshot(Some(&snap)),
            OPERATIONAL_ROLE_BASELINE
        );
        assert_eq!(ExecutionMode::Shadow.as_str(), "shadow");
        assert_ne!(
            operational_role_from_snapshot(Some(&snap)),
            ExecutionMode::Shadow.as_str()
        );
    }

    #[test]
    fn journal_appends_json_lines() {
        let dir = std::env::temp_dir().join(format!("shadow-journal-{}", std::process::id()));
        let path = dir.join("journal.jsonl");
        let journal = ShadowJournal::new(&path);
        let record = ShadowDecisionRecord {
            strategy_id: "BTCPriceMALongFlatV1:BTC/USDT".into(),
            strategy_version: "1".into(),
            config_hash: "sha256:test".into(),
            candle_start_ms: Some(1),
            candle_end_exclusive_ms: Some(2),
            close: Some("100".into()),
            geometric_mean: Some("99".into()),
            log_spread: Some(0.01),
            previous_direction: "flat".into(),
            direction: "long".into(),
            desired_target: "0.001".into(),
            decision_timestamp_ms: 3,
            account_filled_qty: "0".into(),
            working_qty: "0".into(),
            reserved_qty: "0".into(),
            effective_qty: "0".into(),
            entry_freshness: "fresh_eligible".into(),
            execution_mode: "shadow".into(),
            reason: "enter".into(),
            recovery: "clear".into(),
            readiness: "ready".into(),
            would_have_requested_orders: 0,
        };
        journal.append(&record).unwrap();
        journal.append(&record).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 2);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
