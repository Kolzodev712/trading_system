//! Shared panel rendering functions for Spot and Futures monitor screens
use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

fn level_enabled_indicator(
    level_enabled: bool,
    global_trading_enabled: bool,
) -> (&'static str, Color) {
    if !level_enabled {
        ("✗", Color::Red)
    } else if global_trading_enabled {
        ("✓", Color::Green)
    } else {
        // Level is enabled, but globally paused/disabled.
        ("⏸", Color::Yellow)
    }
}

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
    let global_trading_enabled = app.trading_status();
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

                    let (enabled_text, enabled_color) =
                        level_enabled_indicator(l.enabled, global_trading_enabled);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_indicator_respects_global_trading_disabled() {
        assert_eq!(level_enabled_indicator(true, false), ("⏸", Color::Yellow));
    }

    #[test]
    fn level_indicator_shows_enabled_only_when_global_enabled() {
        assert_eq!(level_enabled_indicator(true, true), ("✓", Color::Green));
    }

    #[test]
    fn level_indicator_shows_disabled_when_level_disabled() {
        assert_eq!(level_enabled_indicator(false, true), ("✗", Color::Red));
        assert_eq!(level_enabled_indicator(false, false), ("✗", Color::Red));
    }
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
                            strat.position_quantity.to_string()
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

/// Render the strategy details panel (for selected strategy). Used by the strategy details popup.
pub fn render_strategy_details(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let strategy = app.selected_strategy();
    let trading_status = app.trading_status();

    let rows: Vec<_> = if let Some(strat) = strategy {
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

/// Render the orderbook panel with multi-pair support
pub fn render_orderbook(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let orderbooks = data.map(|md| md.orderbooks.as_slice()).unwrap_or(&[]);

    if orderbooks.is_empty() {
        let empty_text = vec![ratatui::text::Line::from("No orderbook data available")];
        let paragraph = ratatui::widgets::Paragraph::new(empty_text).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title("Orderbooks"),
        );
        frame.render_widget(paragraph, area);
        return Ok(());
    }

    let selected_idx = app.selected_orderbook_pair_index().unwrap_or(0);
    let selected_idx = selected_idx.min(orderbooks.len().saturating_sub(1));

    // Split area: top for pair tabs, bottom for orderbook content
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(3), // Pair tabs
            ratatui::layout::Constraint::Min(0),    // Orderbook content
        ])
        .split(area);

    let mut tab_spans = Vec::new();
    for (idx, ob) in orderbooks.iter().enumerate() {
        let is_selected = idx == selected_idx;
        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        tab_spans.push(ratatui::text::Span::styled(format!(" {} ", ob.pair), style));
    }

    let tabs_line = ratatui::text::Line::from(tab_spans);
    let tabs_paragraph = ratatui::widgets::Paragraph::new(tabs_line).block(
        ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .title("Pairs"),
    );
    frame.render_widget(tabs_paragraph, chunks[0]);

    // Render orderbook for selected pair
    if let Some(orderbook) = orderbooks.get(selected_idx) {
        // Split orderbook area: left for bids, right for asks
        let ob_chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Percentage(50), // Bids
                ratatui::layout::Constraint::Percentage(50), // Asks
            ])
            .split(chunks[1]);

        // Render bids (left side)
        let bid_rows: Vec<Row> = orderbook
            .bids
            .iter()
            .map(|level| {
                Row::new(vec![
                    Cell::from(level.price.as_str()),
                    Cell::from(level.quantity.as_str()),
                ])
            })
            .collect();

        let bid_table = Table::new(
            bid_rows,
            [
                ratatui::layout::Constraint::Percentage(50),
                ratatui::layout::Constraint::Percentage(50),
            ],
        )
        .header(Row::new(vec![
            Cell::from("Price").style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Qty").style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Bids - {} (Best: {})",
            orderbook.pair, orderbook.best_bid
        )));
        frame.render_widget(bid_table, ob_chunks[0]);

        // Render asks (right side)
        let ask_rows: Vec<Row> = orderbook
            .asks
            .iter()
            .map(|level| {
                Row::new(vec![
                    Cell::from(level.price.as_str()),
                    Cell::from(level.quantity.as_str()),
                ])
            })
            .collect();

        let ask_table = Table::new(
            ask_rows,
            [
                ratatui::layout::Constraint::Percentage(50),
                ratatui::layout::Constraint::Percentage(50),
            ],
        )
        .header(Row::new(vec![
            Cell::from("Price").style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Cell::from("Qty").style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]))
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Asks - {} (Best: {})",
            orderbook.pair, orderbook.best_ask
        )));
        frame.render_widget(ask_table, ob_chunks[1]);

        // Render summary info at bottom if space allows
        if chunks[1].height > 15 {
            let summary_text = vec![ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Mid: ", Style::default()),
                ratatui::text::Span::styled(
                    &orderbook.mid_price,
                    Style::default().fg(Color::Yellow),
                ),
                ratatui::text::Span::from("  │  "),
                ratatui::text::Span::styled("Spread: ", Style::default()),
                ratatui::text::Span::styled(&orderbook.spread, Style::default().fg(Color::Magenta)),
            ])];
            let summary_paragraph = ratatui::widgets::Paragraph::new(summary_text)
                .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL));
            let summary_area = Rect {
                x: area.x,
                y: area.y + area.height - 3,
                width: area.width,
                height: 3,
            };
            frame.render_widget(summary_paragraph, summary_area);
        }
    }

    Ok(())
}
