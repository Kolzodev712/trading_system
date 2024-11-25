use core_logic::data::base_data::Side;
use exchange_engine::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{
        BinanceDataType, BinanceOrderRequest, BinanceOrderResponse, BinanceOrderType,
        BinancePrivateEndpoint, BinanceSymbol, BinanceTimeInForce,
    },
};
use rust_decimal::Decimal;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("BINANCE_API_KEY")?;
    let secret_key = env::var("BINANCE_SECRET_KEY")?;

    let public_handler = BinanceExchangeHandler::new_public(
        BinanceDataType::ServerTime,
        BinanceSymbol::new("BTC", "USDT"),
        None,
        None,
        None,
    );

    let pair: BinanceSymbol = BinanceSymbol::new("BTC", "USDT");
    let symbol = BinanceSymbol::to_symbol(&pair);
    let side = Side::Buy;
    let order_type = BinanceOrderType::Limit;
    let price = Some(Decimal::new(90000, 0));
    let min_notional = Decimal::new(5, 0);

    let quantity = (min_notional / price.unwrap())
        .round_dp_with_strategy(5, rust_decimal::RoundingStrategy::MidpointAwayFromZero);

    let time_in_force = Some(BinanceTimeInForce::Gtc);

    let order_request = BinanceOrderRequest {
        symbol,
        side: side.into(),
        order_type,
        time_in_force,
        quantity: Some(quantity),
        price,
        timestamp: public_handler.server_time().await?,
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
        Some(60000),
        Some(order_request.timestamp),
        api_key.clone().into(),
        secret_key.clone().into(),
    );

    // Step 3: Send the POST request to place the order
    order_handler.post::<BinanceOrderResponse>(None).await?;

    println!("Order placed successfully");

    Ok(())
}
