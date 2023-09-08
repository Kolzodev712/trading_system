use crate::parser::{Config, FileConfigParser};
use crate::ws_requests::make_ws_request;

pub async fn binance_orderbook_ws_stream(base: &str, quote: &str) {
    let file_parser = FileConfigParser::new('=');
    let config = Config::new(file_parser, "config.cfg").unwrap();

    let base_data = config
        .get("binance_ws_base")
        .unwrap_or_else(|| String::new());

    let endpoint_data = config
        .get("binance_ws_endopint")
        .unwrap_or_else(|| String::new());

    make_ws_request(&base_data, &endpoint_data, base, quote, "depth").await;
}

pub async fn binance_trades_ws_stream(base: &str, quote: &str) {
    let file_parser = FileConfigParser::new('=');
    let config = Config::new(file_parser, "config.cfg").unwrap();

    let base_data = config
        .get("binance_ws_base")
        .unwrap_or_else(|| String::new());

    let endpoint_data = config
        .get("binance_ws_endopint")
        .unwrap_or_else(|| String::new());

    make_ws_request(&base_data, &endpoint_data, base, quote, "trades").await;
}
