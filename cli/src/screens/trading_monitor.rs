//! Trading Monitor Screen - Displays real-time trading data
use crate::app::App;
use crate::error::CliError;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

pub struct TradingMonitorScreen;

impl TradingMonitorScreen {
    pub fn render(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        // Create main layout - use Percentage to avoid overflow issues
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),      // Header
                Constraint::Min(0),         // Main content (flexible)
                Constraint::Percentage(25), // Trades (25% of remaining)
                Constraint::Length(3),      // Footer
            ])
            .split(area);

        // Header
        let header = Paragraph::new("Trading Monitor")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Main content - split into three rows: (orders/balances), (levels/indicators)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // Top row split horizontally (orders/balances)
        let top_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[0]);

        // Bottom row split horizontally (levels/indicators)
        let bottom_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[1]);

        // Orders (top-left)
        Self::render_orders(frame, top_row[0], app)?;

        // Balances (top-right)
        Self::render_balances(frame, top_row[1], app)?;

        // Levels (bottom-left)
        Self::render_levels(frame, bottom_row[0], app)?;

        // Indicators (bottom-right)
        Self::render_indicators(frame, bottom_row[1], app)?;

        // Trades full-width
        Self::render_trades(frame, chunks[2], app)?;

        // Footer
        let footer_text = vec![Line::from(vec![
            Span::styled(
                "Esc",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Back  │  "),
            Span::styled(
                "R",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Refresh  │  "),
            Span::styled(
                "L",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Levels"),
        ])];
        let footer = Paragraph::new(footer_text)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[3]);

        Ok(())
    }

    fn render_orders(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        let data = app.monitoring_data();
        let rows = if let Some(md) = data {
            md.orders
                .iter()
                .take(15)
                .map(|o| {
                    Row::new(vec![
                        Cell::from(o.order_id.clone()),
                        Cell::from(o.symbol.clone()),
                        Cell::from(o.side.clone()),
                        Cell::from(o.price.clone()),
                        Cell::from(o.quantity.clone()),
                        Cell::from(o.status.clone()),
                    ])
                })
                .collect::<Vec<_>>()
        } else {
            vec![Row::new(vec![Cell::from("No orders data available")])]
        };

        let widths = [
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
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

    fn render_levels(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        let levels = app.levels();
        let rows = if let Some(levels_vec) = levels {
            if levels_vec.is_empty() {
                vec![Row::new(vec![Cell::from("No active levels")])]
            } else {
                levels_vec
                    .iter()
                    .take(10)
                    .map(|l| {
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
                        let pair_str = if let Some(config) = &l.config {
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

                        Row::new(vec![
                            Cell::from(l.id.clone()),
                            Cell::from(pair_str),
                            Cell::from(enabled_text).style(Style::default().fg(enabled_color)),
                            Cell::from(status).style(Style::default().fg(status_color)),
                            Cell::from(l.active_orders.len().to_string()),
                        ])
                    })
                    .collect::<Vec<_>>()
            }
        } else {
            vec![Row::new(vec![Cell::from("Loading levels...")])]
        };

        let widths = [
            Constraint::Length(12), // Level ID
            Constraint::Length(10), // Pair
            Constraint::Length(8),  // Enabled (was 5, too narrow)
            Constraint::Length(6),  // Status (was 3, too narrow)
            Constraint::Length(8),  // Orders
        ];

        let table = Table::new(rows, widths)
            .header(Row::new(vec![
                Cell::from("Level ID"),
                Cell::from("Pair"),
                Cell::from("Enabled"), // Level's individual enabled state (✓ = enabled, ✗ = disabled)
                Cell::from("Status"), // Level state (● = Created/Posted, ✓ = Filled, ✗ = Cancelled, ⏸ = Paused)
                Cell::from("Orders"), // Number of active orders
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Active Levels"),
            );
        frame.render_widget(table, area);
        Ok(())
    }

    fn render_balances(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        let data = app.monitoring_data();
        let rows = if let Some(md) = data {
            md.balances
                .iter()
                .take(10)
                .map(|b| {
                    Row::new(vec![
                        Cell::from(b.asset.clone()),
                        Cell::from(b.free.clone()),
                        Cell::from(b.used.clone()),
                        Cell::from(b.total.clone()),
                    ])
                })
                .collect::<Vec<_>>()
        } else {
            vec![Row::new(vec![Cell::from("No balances data available")])]
        };

        let widths = [
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
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

    fn render_indicators(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        let data = app.monitoring_data();
        let rows = if let Some(md) = data {
            md.indicators
                .iter()
                .take(10)
                .map(|i| {
                    Row::new(vec![
                        Cell::from(i.indicator_name.clone()),
                        Cell::from(i.pair.clone()),
                        Cell::from(i.value.clone()),
                        Cell::from(i.period.to_string()),
                    ])
                })
                .collect::<Vec<_>>()
        } else {
            vec![Row::new(vec![Cell::from("No indicators data available")])]
        };

        let widths = [
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(8),
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

    fn render_trades(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        let data = app.monitoring_data();
        let rows = if let Some(md) = data {
            md.trades
                .iter()
                .rev() // show latest first
                .take(15)
                .map(|t| {
                    Row::new(vec![
                        Cell::from(t.trade_id.clone()),
                        Cell::from(t.order_id.clone()),
                        Cell::from(t.symbol.clone()),
                        Cell::from(t.side.clone()),
                        Cell::from(t.price.clone()),
                        Cell::from(t.quantity.clone()),
                    ])
                })
                .collect::<Vec<_>>()
        } else {
            vec![Row::new(vec![Cell::from("No trades data available")])]
        };

        let widths = [
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(10),
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

    pub async fn handle_input_async(app: &mut App, key: KeyCode) -> Result<(), CliError> {
        match key {
            KeyCode::Esc => {
                // Navigate back to main menu
                app.go_to_main_menu();
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Refresh data
                app.refresh_monitoring_data().await?;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Navigate to level management (Phase 3)
                // Note: Level management screen not yet implemented
                // For now, levels are view-only in the Trading Monitor
            }
            _ => {}
        }
        Ok(())
    }
}
