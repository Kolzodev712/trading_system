use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use trading_engine::strategy::Strategy;

#[allow(dead_code)] // Used in health_check function
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
    uptime_secs: Option<u64>,
}

#[allow(dead_code)] // Used in get_positions function
#[derive(Debug, Serialize)]
struct PositionResponse {
    positions: Vec<PositionInfo>,
}

#[allow(dead_code)] // Used in get_positions function
#[derive(Debug, Serialize)]
struct PositionInfo {
    symbol: String,
    quantity: String,
    avg_price: String,
}

#[allow(dead_code)] // Used in get_state function
#[derive(Debug, Serialize)]
struct StateResponse {
    active_orders: usize,
    pending_orders: usize,
    positions_count: usize,
    total_position_value: String,
}

#[allow(dead_code)] // Used in get_orders function
#[derive(Debug, Serialize)]
struct OrdersResponse {
    orders: Vec<OrderInfo>,
}

#[allow(dead_code)] // Used in get_orders function
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

#[allow(dead_code)] // Used in main.rs
pub struct HealthServer {
    strategy: Arc<RwLock<dyn Strategy>>,
}

impl HealthServer {
    #[allow(dead_code)] // Used in main.rs
    pub fn new(strategy: Arc<RwLock<dyn Strategy>>) -> Self {
        Self { strategy }
    }

    #[allow(dead_code)] // Used in main.rs
    pub async fn start(
        &self,
        bind_address: &str,
        port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let strategy = Arc::clone(&self.strategy);

        let app = Router::new()
            .route("/health", get(health_check))
            .route(
                "/positions",
                get({
                    let strat = Arc::clone(&strategy);
                    move || get_positions(State(strat))
                }),
            )
            .route(
                "/state",
                get({
                    let strat = Arc::clone(&strategy);
                    move || get_state(State(strat))
                }),
            )
            .route(
                "/orders",
                get({
                    let strat = Arc::clone(&strategy);
                    move || get_orders(State(strat))
                }),
            )
            .route("/", get(health_check));

        let addr = format!("{bind_address}:{port}");
        info!("Health check server listening on {addr}");

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

#[allow(dead_code)] // Used as route handler
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        uptime_secs: None, // Could be enhanced with actual uptime tracking
    })
}

#[allow(dead_code)] // Used as route handler
async fn get_positions(
    State(strategy): State<Arc<RwLock<dyn Strategy>>>,
) -> Result<Json<PositionResponse>, StatusCode> {
    let strategy_guard = strategy.read().await;
    let state = strategy_guard.get_state();

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

#[allow(dead_code)] // Used as route handler
async fn get_state(
    State(strategy): State<Arc<RwLock<dyn Strategy>>>,
) -> Result<Json<StateResponse>, StatusCode> {
    let strategy_guard = strategy.read().await;
    let state = strategy_guard.get_state();

    let active_orders = state.active_orders.len();
    let pending_orders = state.pending_orders.len();
    let positions_count = state.positions.len();

    // Calculate total position value (simplified - would need current prices)
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

#[allow(dead_code)] // Used as route handler
async fn get_orders(
    State(strategy): State<Arc<RwLock<dyn Strategy>>>,
) -> Result<Json<OrdersResponse>, StatusCode> {
    let strategy_guard = strategy.read().await;
    let state = strategy_guard.get_state();

    let orders: Vec<OrderInfo> = state
        .active_orders
        .values()
        .map(|order| OrderInfo {
            order_id: order.order_id.as_str().to_string(),
            symbol: order.symbol.as_ref().to_string(),
            side: format!("{:?}", order.side),
            price: order.price.to_string(),
            amount: order.amount.to_string(),
            status: format!("{:?}", order.status),
            filled_amount: order.filled_amount.to_string(),
        })
        .collect();

    Ok(Json(OrdersResponse { orders }))
}
