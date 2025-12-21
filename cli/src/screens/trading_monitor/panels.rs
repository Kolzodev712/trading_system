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
                    Cell::from(if o.level_id.is_empty() {
                        "N/A"
                    } else {
                        o.level_id.as_str()
                    }),
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
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Order ID"),
            Cell::from("Symbol"),
            Cell::from("Side"),
            Cell::from("Price"),
            Cell::from("Qty"),
            Cell::from("Level"),
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

/// Render the strategy panel (list view like levels)
pub fn render_strategy(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let strategies = app.strategies();
    let selected_idx = app.selected_strategy_index();

    let rows = if let Some(strategies_vec) = strategies {
        if strategies_vec.is_empty() {
            vec![Row::new(vec![Cell::from("No strategies active")])]
        } else {
            strategies_vec
                .iter()
                .take(10)
                .enumerate()
                .map(|(idx, strat)| {
                    let is_selected = selected_idx == Some(idx);
                    // Use strategy's enabled status instead of global trading_status
                    let strategy_enabled = strat.enabled;
                    let status_color = if strategy_enabled {
                        Color::Green
                    } else {
                        Color::Red
                    };

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
                        Cell::from(strat.strategy_name.as_str()).style(row_style),
                        Cell::from(strat.symbol.as_str()).style(row_style),
                        Cell::from(if strategy_enabled { "●" } else { "⏸" }).style(
                            Style::default().fg(status_color).bg(if is_selected {
                                Color::Cyan
                            } else {
                                Color::Reset
                            }),
                        ),
                        Cell::from(format!(
                            "{}/{}",
                            strat.active_orders, strat.max_active_orders
                        ))
                        .style(row_style),
                        Cell::from(if strat.position_quantity != "0" {
                            format!("{}", strat.position_quantity)
                        } else {
                            "0".to_string()
                        })
                        .style(row_style),
                    ])
                })
                .collect::<Vec<_>>()
        }
    } else {
        vec![Row::new(vec![Cell::from("No strategy data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(18), // Strategy name
        ratatui::layout::Constraint::Length(12), // Symbol
        ratatui::layout::Constraint::Length(4),  // Status
        ratatui::layout::Constraint::Length(8),  // Orders
        ratatui::layout::Constraint::Min(0),     // Position
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Strategy"),
            Cell::from("Symbol"),
            Cell::from("Status"),
            Cell::from("Orders"),
            Cell::from("Position"),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Strategies"));

    frame.render_widget(table, area);
    Ok(())
}

/// Render the strategy details panel (for selected strategy)
#[allow(dead_code)]
pub fn render_strategy_details(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let strategy = app.selected_strategy();
    let trading_status = app.trading_status();

    let rows = if let Some(strat) = strategy {
        vec![
            Row::new(vec![
                Cell::from("Strategy:"),
                Cell::from(strat.strategy_name.as_str()),
            ]),
            Row::new(vec![
                Cell::from("Status:"),
                Cell::from(if trading_status {
                    "● Running"
                } else {
                    "⏸ Paused"
                })
                .style(if trading_status {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                }),
            ]),
            Row::new(vec![
                Cell::from("Symbol:"),
                Cell::from(strat.symbol.as_str()),
            ]),
            Row::new(vec![
                Cell::from("Position:"),
                Cell::from(if strat.position_quantity != "0" {
                    format!("{} @ {}", strat.position_quantity, strat.position_avg_price)
                } else {
                    "None".to_string()
                }),
            ]),
            Row::new(vec![
                Cell::from("P&L:"),
                Cell::from(
                    if strat.unrealized_pnl != "N/A" && strat.unrealized_pnl != "0" {
                        format!(
                            "{} ({})",
                            strat.unrealized_pnl, strat.unrealized_pnl_percent
                        )
                    } else {
                        "N/A".to_string()
                    },
                )
                .style(if strat.unrealized_pnl.starts_with('-') {
                    Style::default().fg(Color::Red)
                } else if !strat.unrealized_pnl.is_empty()
                    && strat.unrealized_pnl != "0"
                    && strat.unrealized_pnl != "N/A"
                {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                }),
            ]),
            Row::new(vec![
                Cell::from("Orders:"),
                Cell::from(format!(
                    "{}/{}",
                    strat.active_orders, strat.max_active_orders
                )),
            ]),
            Row::new(vec![
                Cell::from("Last Signal:"),
                Cell::from(
                    if !strat.last_signal.is_empty() && strat.last_signal != "None" {
                        strat.last_signal.as_str()
                    } else {
                        "None"
                    },
                ),
            ]),
        ]
    } else {
        vec![Row::new(vec![Cell::from("No strategy data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Min(0),
    ];

    let table = Table::new(rows, widths).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Strategy Status"),
    );
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
