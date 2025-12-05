use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_engine::strategy::Strategy;
use tracing::info;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
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

pub struct HealthServer {
    strategy: Arc<RwLock<dyn Strategy>>,
}

impl HealthServer {
    pub fn new(strategy: Arc<RwLock<dyn Strategy>>) -> Self {
        Self { strategy }
    }

    pub async fn start(&self, bind_address: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let strategy = Arc::clone(&self.strategy);

        let app = Router::new()
            .route("/health", get(health_check))
            .route("/positions", get({
                let strat = Arc::clone(&strategy);
                move || get_positions(State(strat))
            }))
            .route("/", get(health_check));

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
    })
}

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


