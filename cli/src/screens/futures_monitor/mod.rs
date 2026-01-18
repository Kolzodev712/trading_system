//! Futures Monitor Screen - Displays real-time futures trading data
pub mod panels;

use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct FuturesMonitorScreen;

impl FuturesMonitorScreen {
    pub fn render(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        // Create main layout
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
        let header = Paragraph::new("Futures Monitor")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Main content - split into 3 columns: (positions/orders), (orderbook), (margin/indicators)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33), // Left column: positions/orders
                Constraint::Percentage(34), // Middle column: orderbook
                Constraint::Percentage(33), // Right column: margin/indicators
            ])
            .split(chunks[1]);

        // Left column: split vertically
        // In strategies mode: (strategies, orders)
        // In levels mode: (positions, orders)
        let left_col = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(main_chunks[0]);

        // Top-left: Show strategies if in strategies mode, otherwise show positions
        if app.has_strategy() {
            // Strategies (top-left) - for futures strategies mode
            crate::screens::shared::panels::render_strategy(frame, left_col[0], app)?;
        } else {
            // Positions (top-left) - most important for futures levels mode
            panels::render_positions(frame, left_col[0], app)?;
        }

        // Orders (bottom-left)
        panels::render_orders(frame, left_col[1], app)?;

        // Middle column: Orderbook (full height)
        panels::render_orderbook(frame, main_chunks[1], app)?;

        // Right column: split vertically (margin info, indicators)
        let right_col = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[2]);

        // Margin info (top-right)
        panels::render_margin_info(frame, right_col[0], app)?;

        // Indicators (bottom-right)
        panels::render_indicators(frame, right_col[1], app)?;

        // Trades full-width
        panels::render_trades(frame, chunks[2], app)?;

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
                "↑/↓",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Select  │  "),
            Span::styled(
                "T",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(if app.has_strategy() {
                " Toggle Trading  │  "
            } else {
                " Toggle  │  "
            }),
            Span::styled(
                "L",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Details  │  "),
            Span::styled(
                "←/→",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Orderbook Tabs"),
        ])];
        let footer = Paragraph::new(footer_text)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[3]);

        // Strategy details popup (if shown) - render LAST so it overlays everything
        if app.show_strategy_details() {
            crate::screens::shared::popups::render_strategy_details_popup(frame, app)?;
        }

        Ok(())
    }
}
