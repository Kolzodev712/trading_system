use core_logic::{
    data::base_data::{OrderType, Pair, Side},
    utils::current_timestamp,
};
use exchange_engine::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{BinanceOrderRequest, BinancePrivateEndpoint, BinanceTimeInForce},
};
use rust_decimal::Decimal;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("BINANCE_API_KEY")?; // $env:BINANCE_API_KEY = "YQkiaZqcvbLdtqJ2V50xzkq2y44vX8HLCxALeQeSn35eyd0UpCdG6hhHS68L6GFI"
    let secret_key = env::var("BINANCE_SECRET_KEY")?; // $env:BINANCE_SECRET_KEY = "IVhqo9ZdafbgdxGmXex6tpx77JqmTKwp57dadWv06VIk4Ce4WNQBQRM3cKqkXMCn"

    let symbol = Pair {
        base: "BTC".into(),
        quote: "USDT".into(),
    };
    let side = Side::Sell;
    let order_type = OrderType::Limit;
    let quantity = Decimal::new(1, 4);
    let price = Some(Decimal::new(5823147, 2));
    let time_in_force = Some(BinanceTimeInForce::Gtc);

    let order_request = BinanceOrderRequest {
        symbol,
        side,
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
    order_handler.post::<()>(None).await?;

    println!("Order placed successfully");

    Ok(())
}
