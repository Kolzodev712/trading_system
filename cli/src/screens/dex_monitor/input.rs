//! DEX monitor input: refresh, quote toggle, cancel-all.

use crate::app::App;
use crate::error::CliError;
use crossterm::event::KeyCode;

pub async fn handle_input_async(app: &mut App, key: KeyCode) -> Result<(), CliError> {
    match key {
        KeyCode::Esc => {
            app.go_to_main_menu();
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let _ = app.refresh_monitoring_data().await;
        }
        KeyCode::Left => {
            app.select_orderbook_pair_left();
        }
        KeyCode::Right => {
            app.select_orderbook_pair_right();
        }
        KeyCode::Up => {
            app.select_dex_quote_up();
        }
        KeyCode::Down => {
            app.select_dex_quote_down();
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            app.toggle_selected_dex_quote().await?;
        }
        KeyCode::Char('[') => {
            app.select_dex_strategy_up();
        }
        KeyCode::Char(']') => {
            app.select_dex_strategy_down();
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.toggle_selected_dex_strategy().await?;
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.cancel_dex_orders_for_pool().await?;
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            app.quit();
        }
        _ => {}
    }
    Ok(())
}
