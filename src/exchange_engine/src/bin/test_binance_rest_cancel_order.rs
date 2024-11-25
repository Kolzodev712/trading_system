use core_logic::utils::current_timestamp;
use exchange_engine::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{
        BinanceCancelOrderByIdRequest, BinanceCancelOrderByIdResponse, BinanceOrderStatus,
        BinanceOrderType, BinancePrivateEndpoint, BinanceSide, BinanceSymbol, BinanceTimeInForce,
    },
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("BINANCE_API_KEY")?;
    let secret_key = env::var("BINANCE_SECRET_KEY")?;

    let pair: BinanceSymbol = BinanceSymbol::new("BTC", "USDT");
    let symbol = BinanceSymbol::to_symbol(&pair);
    let side = BinanceSide::Sell;
    let order_id = 32973046410;
    let order_type = BinanceOrderType::Limit;
    let time_in_force = BinanceTimeInForce::Gtc;
    let status = BinanceOrderStatus::Filled;

    let cancel_order_request = BinanceCancelOrderByIdRequest {
        symbol,
        order_id,
        side: side.clone(),
        timestamp: current_timestamp(),
        status: status.clone(),
        time_in_force: time_in_force.clone(),
        order_type: order_type.clone(),
    };

    let cancel_order_handler = BinanceExchangeHandler::new_private(
        BinancePrivateEndpoint::CancelOrder {
            symbol: cancel_order_request.symbol.clone().into(),
            order_id: cancel_order_request.order_id,
            side,
            status,
            time_in_force,
            order_type,
        },
        None,
        Some(cancel_order_request.timestamp),
        api_key.clone().into(),
        secret_key.clone().into(),
    );

    // Send the POST request to cancel the order
    let cancel_response = cancel_order_handler
        .post::<BinanceCancelOrderByIdResponse>(None)
        .await?;

    println!("Order cancelled successfully: {:?}", cancel_response);

    Ok(())
}
