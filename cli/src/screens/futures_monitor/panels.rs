//! Panel rendering functions for Futures Monitor
use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

/// Render the positions panel (futures-specific)
pub fn render_positions(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let rows = if let Some(md) = data {
        md.positions
            .iter()
            .take(10)
            .map(|pos| {
                let side_color = if pos.side == "Long" {
                    Color::Green
                } else {
                    Color::Red
                };
                let pnl_color = if pos.unrealized_pnl.starts_with('-') {
                    Color::Red
                } else if !pos.unrealized_pnl.is_empty() && pos.unrealized_pnl != "0" {
                    Color::Green
                } else {
                    Color::White
                };
                Row::new(vec![
                    Cell::from(pos.symbol.as_str()),
                    Cell::from(pos.side.as_str()).style(Style::default().fg(side_color)),
                    Cell::from(pos.quantity.as_str()),
                    Cell::from(pos.avg_price.as_str()),
                    Cell::from(pos.mark_price.as_str()),
                    Cell::from(pos.unrealized_pnl.as_str()).style(Style::default().fg(pnl_color)),
                    Cell::from(pos.leverage.as_str()),
                    Cell::from(if pos.liquidation_price.is_empty() {
                        "N/A"
                    } else {
                        pos.liquidation_price.as_str()
                    }),
                ])
            })
            .collect::<Vec<_>>()
    } else {
        vec![Row::new(vec![Cell::from("No positions data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(10), // Symbol
        ratatui::layout::Constraint::Length(6),  // Side
        ratatui::layout::Constraint::Length(10), // Quantity
        ratatui::layout::Constraint::Length(10), // Entry Price
        ratatui::layout::Constraint::Length(10), // Mark Price
        ratatui::layout::Constraint::Length(12), // PnL
        ratatui::layout::Constraint::Length(8),  // Leverage
        ratatui::layout::Constraint::Length(12), // Liquidation
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            Cell::from("Symbol"),
            Cell::from("Side"),
            Cell::from("Quantity"),
            Cell::from("Entry"),
            Cell::from("Mark"),
            Cell::from("Unreal PnL"),
            Cell::from("Lev"),
            Cell::from("Liquidation"),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Positions"));
    frame.render_widget(table, area);
    Ok(())
}

/// Render margin information panel
pub fn render_margin_info(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let data = app.monitoring_data();
    let rows = if let Some(md) = data {
        let mut total_margin = 0.0;
        let mut total_pnl = 0.0;
        let mut position_count = 0;

        for pos in &md.positions {
            if let Ok(margin) = pos.margin_used.parse::<f64>() {
                total_margin += margin;
            }
            if let Ok(pnl) = pos.unrealized_pnl.parse::<f64>() {
                total_pnl += pnl;
            }
            position_count += 1;
        }

        // Get collateral balance (usually USDT)
        let collateral = md
            .balances
            .iter()
            .find(|b| b.asset == "USDT" || b.asset == "USD")
            .map(|b| b.total.clone())
            .unwrap_or_else(|| "0".to_string());

        let margin_str = format!("{:.2}", total_margin);
        let pnl_str = format!("{:.2}", total_pnl);
        let count_str = position_count.to_string();

        vec![
            Row::new(vec![
                Cell::from("Collateral:"),
                Cell::from(collateral.clone()),
            ]),
            Row::new(vec![
                Cell::from("Margin Used:"),
                Cell::from(margin_str.clone()),
            ]),
            Row::new(vec![
                Cell::from("Total PnL:"),
                Cell::from(pnl_str.clone()).style(if total_pnl > 0.0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                }),
            ]),
            Row::new(vec![
                Cell::from("Positions:"),
                Cell::from(count_str.clone()),
            ]),
        ]
    } else {
        vec![Row::new(vec![Cell::from("No margin data available")])]
    };

    let widths = [
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Min(0),
    ];

    let table =
        Table::new(rows, widths).block(Block::default().borders(Borders::ALL).title("Margin Info"));
    frame.render_widget(table, area);
    Ok(())
}

// Re-export shared panels from shared module
pub fn render_orders(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    crate::screens::shared::panels::render_orders(frame, area, app)
}

pub fn render_orderbook(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    crate::screens::shared::panels::render_orderbook(frame, area, app)
}

pub fn render_indicators(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    crate::screens::shared::panels::render_indicators(frame, area, app)
}

pub fn render_trades(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    crate::screens::shared::panels::render_trades(frame, area, app)
}
