//! Panel rendering functions for Trading Monitor
use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

/// Render the orders panel
pub fn render_orders(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let rows = if let Some(md) = data {
        md.orders
            .iter()
            .take(15)
            .map(|o| {
                Row::new(vec![
                    Cell::from(o.order_id.as_str()),
                    Cell::from(o.symbol.as_str()),
                    Cell::from(o.side.as_str()),
                    Cell::from(o.price.as_str()),
                    Cell::from(o.quantity.as_str()),
                    Cell::from(o.status.as_str()),
                ])
            })
            .collect::<Vec<_>>()
    } else {
        vec![Row::new(vec![Cell::from("No orders data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(14),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(6),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Order ID"),
            Cell::from("Symbol"),
            Cell::from("Side"),
            Cell::from("Price"),
            Cell::from("Qty"),
            Cell::from("Status"),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Open Orders"));
    frame.render_widget(table, area);
    Ok(())
}

/// Render the levels panel
pub fn render_levels(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let levels = app.levels();
    let selected_idx = app.selected_level_index();
    let rows = if let Some(levels_vec) = levels {
        if levels_vec.is_empty() {
            vec![Row::new(vec![Cell::from("No active levels")])]
        } else {
            levels_vec
                .iter()
                .take(10)
                .enumerate()
                .map(|(idx, l)| {
                    let is_selected = selected_idx == Some(idx);
                    let status = match l.status.as_str() {
                        "Created" => "●",
                        "Posted" => "●",
                        "Filled" => "✓",
                        "Cancelled" => "✗",
                        "Paused" => "⏸",
                        _ => "?",
                    };
                    let status_color = match l.status.as_str() {
                        "Created" | "Posted" => Color::Yellow,
                        "Filled" => Color::Green,
                        "Cancelled" => Color::Red,
                        "Paused" => Color::Magenta,
                        _ => Color::White,
                    };
                    // Get pair from config
                    let pair_str: String = if let Some(config) = &l.config {
                        if let Some(symbol) = &config.symbol {
                            format!("{}/{}", symbol.base, symbol.quote)
                        } else {
                            "N/A".to_string()
                        }
                    } else {
                        "N/A".to_string()
                    };

                    // Level enabled state (separate from global trading enabled)
                    let enabled_text = if l.enabled { "✓" } else { "✗" };
                    let enabled_color = if l.enabled { Color::Green } else { Color::Red };

                    // Highlight selected row
                    let row_style = if is_selected {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    Row::new(vec![
                        Cell::from(l.id.as_str()).style(row_style),
                        Cell::from(pair_str).style(row_style),
                        Cell::from(enabled_text).style(Style::default().fg(enabled_color).bg(
                            if is_selected {
                                Color::Cyan
                            } else {
                                Color::Reset
                            },
                        )),
                        Cell::from(status).style(Style::default().fg(status_color).bg(
                            if is_selected {
                                Color::Cyan
                            } else {
                                Color::Reset
                            },
                        )),
                        Cell::from(l.active_orders.len().to_string()).style(row_style),
                    ])
                })
                .collect::<Vec<_>>()
        }
    } else {
        vec![Row::new(vec![Cell::from("Loading levels...")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(12), // Level ID
        ratatui::layout::Constraint::Length(10), // Pair
        ratatui::layout::Constraint::Length(8),  // Enabled
        ratatui::layout::Constraint::Length(6),  // Status
        ratatui::layout::Constraint::Length(8),  // Orders
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Level ID"),
            Cell::from("Pair"),
            Cell::from("Enabled"),
            Cell::from("Status"),
            Cell::from("Orders"),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Active Levels"),
        );
    frame.render_widget(table, area);
    Ok(())
}

/// Render the balances panel
pub fn render_balances(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let rows = if let Some(md) = data {
        md.balances
            .iter()
            .take(10)
            .map(|b| {
                Row::new(vec![
                    Cell::from(b.asset.as_str()),
                    Cell::from(b.free.as_str()),
                    Cell::from(b.used.as_str()),
                    Cell::from(b.total.as_str()),
                ])
            })
            .collect::<Vec<_>>()
    } else {
        vec![Row::new(vec![Cell::from("No balances data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Asset"),
            Cell::from("Free"),
            Cell::from("Used"),
            Cell::from("Total"),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Balances"));
    frame.render_widget(table, area);
    Ok(())
}

/// Render the indicators panel
pub fn render_indicators(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let rows = if let Some(md) = data {
        md.indicators
            .iter()
            .take(10)
            .map(|i| {
                Row::new(vec![
                    Cell::from(i.indicator_name.as_str()),
                    Cell::from(i.pair.as_str()),
                    Cell::from(i.value.as_str()),
                    Cell::from(i.period.to_string()),
                ])
            })
            .collect::<Vec<_>>()
    } else {
        vec![Row::new(vec![Cell::from("No indicators data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(14),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(8),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Indicator"),
            Cell::from("Pair"),
            Cell::from("Value"),
            Cell::from("Period"),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Indicators"));
    frame.render_widget(table, area);
    Ok(())
}

/// Render the trades panel
pub fn render_trades(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let rows = if let Some(md) = data {
        md.trades
            .iter()
            .rev() // show latest first
            .take(15)
            .map(|t| {
                Row::new(vec![
                    Cell::from(t.trade_id.as_str()),
                    Cell::from(t.order_id.as_str()),
                    Cell::from(t.symbol.as_str()),
                    Cell::from(t.side.as_str()),
                    Cell::from(t.price.as_str()),
                    Cell::from(t.quantity.as_str()),
                ])
            })
            .collect::<Vec<_>>()
    } else {
        vec![Row::new(vec![Cell::from("No trades data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(6),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Trade ID"),
            Cell::from("Order ID"),
            Cell::from("Symbol"),
            Cell::from("Side"),
            Cell::from("Price"),
            Cell::from("Qty"),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Recent Trades"),
        );
    frame.render_widget(table, area);
    Ok(())
}
