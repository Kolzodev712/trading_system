//! Shared health server for both levels and strategies modes
use crate::state::TradingState;
use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

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
}

impl HealthServer {
    pub fn new(trading_state: Arc<RwLock<TradingState>>) -> Self {
        Self { trading_state }
    }

    pub async fn start(
        &self,
        bind_address: &str,
        port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let trading_state = Arc::clone(&self.trading_state);

        let app = Router::new()
            .route("/health", get(health_check))
            .route(
                "/positions",
                get({
                    let state = Arc::clone(&trading_state);
                    move || get_positions(State(state))
                }),
            )
            .route(
                "/state",
                get({
                    let state = Arc::clone(&trading_state);
                    move || get_state(State(state))
                }),
            )
            .route(
                "/orders",
                get({
                    let state = Arc::clone(&trading_state);
                    move || get_orders(State(state))
                }),
            )
            .route("/", get(health_check))
            .with_state(trading_state);

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

async fn get_positions(
    State(trading_state): State<Arc<RwLock<TradingState>>>,
) -> Result<Json<PositionResponse>, StatusCode> {
    let state = trading_state.read().await;

    let mut positions = Vec::new();
    for (pair, position) in &state.positions {
        positions.push(PositionInfo {
            symbol: format!("{}{}", pair.base, pair.quote),
            quantity: position.quantity.to_string(),
            avg_price: position.avg_price.to_string(),
        });
    }

    Ok(Json(PositionResponse { positions }))
}

async fn get_state(
    State(trading_state): State<Arc<RwLock<TradingState>>>,
) -> Result<Json<StateResponse>, StatusCode> {
    let state = trading_state.read().await;

    let active_orders = state.active_orders.len();
    let pending_orders = state.pending_orders.len();
    let positions_count = state.positions.len();

    let total_value = state
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
    State(trading_state): State<Arc<RwLock<TradingState>>>,
) -> Result<Json<OrdersResponse>, StatusCode> {
    let state = trading_state.read().await;

    let mut orders = Vec::new();
    for (order_id, order) in &state.active_orders {
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
