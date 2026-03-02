//! Shared health server for both levels and strategies modes
use crate::state::TradingState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// State for the health server: trading state plus optional Prometheus metrics provider.
pub struct HealthState {
    pub trading_state: Arc<RwLock<TradingState>>,
    /// If set, GET /metrics will return Prometheus exposition format.
    pub metrics_provider: Option<Arc<dyn Fn() -> Vec<u8> + Send + Sync>>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
    uptime_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct PositionResponse {
    positions: Vec<PositionInfo>,
}

#[derive(Debug, Serialize)]
struct PositionInfo {
    symbol: String,
    quantity: String,
    avg_price: String,
}

#[derive(Debug, Serialize)]
struct StateResponse {
    active_orders: usize,
    pending_orders: usize,
    positions_count: usize,
    total_position_value: String,
}

#[derive(Debug, Serialize)]
struct OrdersResponse {
    orders: Vec<OrderInfo>,
}

#[derive(Debug, Serialize)]
struct OrderInfo {
    order_id: String,
    symbol: String,
    side: String,
    price: String,
    amount: String,
    status: String,
    filled_amount: String,
}

/// Health server that works with TradingState directly
pub struct HealthServer {
    trading_state: Arc<RwLock<TradingState>>,
    /// If set, GET /metrics will return Prometheus metrics.
    metrics_provider: Option<Arc<dyn Fn() -> Vec<u8> + Send + Sync>>,
}

impl HealthServer {
    pub fn new(trading_state: Arc<RwLock<TradingState>>) -> Self {
        Self {
            trading_state,
            metrics_provider: None,
        }
    }

    /// Add a Prometheus metrics endpoint. The closure will be called on each GET /metrics.
    pub fn with_metrics(mut self, provider: Arc<dyn Fn() -> Vec<u8> + Send + Sync>) -> Self {
        self.metrics_provider = Some(provider);
        self
    }

    pub async fn start(
        &self,
        bind_address: &str,
        port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = Arc::new(HealthState {
            trading_state: Arc::clone(&self.trading_state),
            metrics_provider: self.metrics_provider.clone(),
        });

        let app = Router::new()
            .route("/health", get(health_check))
            .route("/positions", get(get_positions))
            .route("/state", get(get_state))
            .route("/orders", get(get_orders))
            .route("/", get(health_check))
            .route("/metrics", get(serve_metrics))
            .with_state(state);

        let addr = format!("{bind_address}:{port}");
        info!("Health check server listening on {addr}");

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        uptime_secs: None,
    })
}

async fn serve_metrics(State(state): State<Arc<HealthState>>) -> impl IntoResponse {
    match &state.metrics_provider {
        Some(provider) => {
            let body = provider();
            (
                [(
                    header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )],
                body,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "Metrics not configured").into_response(),
    }
}

async fn get_positions(
    State(state): State<Arc<HealthState>>,
) -> Result<Json<PositionResponse>, StatusCode> {
    let trading_state = state.trading_state.read().await;

    let mut positions = Vec::new();
    for (pair, position) in &trading_state.positions {
        positions.push(PositionInfo {
            symbol: format!("{}{}", pair.base, pair.quote),
            quantity: position.quantity.to_string(),
            avg_price: position.avg_price.to_string(),
        });
    }

    Ok(Json(PositionResponse { positions }))
}

async fn get_state(
    State(state): State<Arc<HealthState>>,
) -> Result<Json<StateResponse>, StatusCode> {
    let trading_state = state.trading_state.read().await;

    let active_orders = trading_state.active_orders.len();
    let pending_orders = trading_state.pending_orders.len();
    let positions_count = trading_state.positions.len();

    let total_value = trading_state
        .positions
        .values()
        .map(|p| p.quantity * p.avg_price)
        .sum::<rust_decimal::Decimal>();

    Ok(Json(StateResponse {
        active_orders,
        pending_orders,
        positions_count,
        total_position_value: total_value.to_string(),
    }))
}

async fn get_orders(
    State(state): State<Arc<HealthState>>,
) -> Result<Json<OrdersResponse>, StatusCode> {
    let trading_state = state.trading_state.read().await;

    let mut orders = Vec::new();
    for (order_id, order) in &trading_state.active_orders {
        orders.push(OrderInfo {
            order_id: order_id.as_str().to_string(),
            symbol: order.instrument.symbol().to_string(),
            side: format!("{:?}", order.side),
            price: order.price.to_string(),
            amount: order.amount.to_string(),
            status: format!("{:?}", order.status),
            filled_amount: order.filled_amount.to_string(),
        });
    }

    Ok(Json(OrdersResponse { orders }))
}
