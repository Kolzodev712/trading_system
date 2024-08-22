use core_logic::{data::base_data::Side, utils::current_timestamp};
use exchange_engine::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{
        BinanceOrderRequest, BinanceOrderResponse, BinanceOrderType, BinancePrivateEndpoint,
        BinanceSymbol, BinanceTimeInForce,
    },
};
use rust_decimal::Decimal;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("BINANCE_API_KEY")?;
    let secret_key = env::var("BINANCE_SECRET_KEY")?;

    let pair: BinanceSymbol = BinanceSymbol::new("BTC", "USDT");
    let symbol = BinanceSymbol::to_symbol(&pair);
    let side = Side::Buy;
    let order_type = BinanceOrderType::Limit;
    let quantity = Decimal::new(9, 5);
    let price = Some(Decimal::new(6092347, 2));
    let time_in_force = Some(BinanceTimeInForce::Gtc);

    let order_request = BinanceOrderRequest {
        symbol,
        side: side.into(),
        order_type,
        time_in_force,
        quantity: Some(quantity),
        price,
        timestamp: current_timestamp(),
    };

    let order_handler = BinanceExchangeHandler::new_private(
        BinancePrivateEndpoint::PlaceOrder {
            symbol: order_request.symbol.clone().into(),
            side: order_request.side.clone().into(),
            order_type: order_request.order_type.clone().into(),
            quantity: order_request.quantity.unwrap_or_default(),
            price: order_request.price,
            time_in_force: order_request.time_in_force.map(|tif| tif.into()),
        },
        None,
        Some(order_request.timestamp),
        api_key.clone().into(),
        secret_key.clone().into(),
    );

    // Send the POST request to place the order
    order_handler.post::<BinanceOrderResponse>(None).await?;

    println!("Order placed successfully");

    Ok(())
}
