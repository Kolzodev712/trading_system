//! Trading Monitor Screen - Displays real-time trading data
mod forms;
mod input;
mod panels;
mod popups;

use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub use input::handle_input_async;

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
        panels::render_orders(frame, top_row[0], app)?;

        // Balances (top-right)
        panels::render_balances(frame, top_row[1], app)?;

        // Strategy or Levels (bottom-left) - show strategy if available, else levels
        if app.has_strategy() {
            panels::render_strategy(frame, bottom_row[0], app)?;
        } else {
            panels::render_levels(frame, bottom_row[0], app)?;
        }

        // Indicators (bottom-right)
        panels::render_indicators(frame, bottom_row[1], app)?;

        // Trades full-width
        panels::render_trades(frame, chunks[2], app)?;

        // Footer - show different text if edit/create form is open
        let footer_text = if app.show_create_form() {
            vec![Line::from(vec![
                Span::styled(
                    "↑/↓",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Navigate  │  "),
                Span::styled(
                    "Space",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Toggle/Edit  │  "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Create  │  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Cancel"),
            ])]
        } else if app.show_edit_form() {
            vec![Line::from(vec![
                Span::styled(
                    "↑/↓",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Navigate  │  "),
                Span::styled(
                    "Space",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Toggle/Edit  │  "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Save  │  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Cancel"),
            ])]
        } else {
            vec![Line::from(vec![
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
                    "E",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Edit  │  "),
                Span::styled(
                    "C",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" Create"),
            ])]
        };
        let footer = Paragraph::new(footer_text)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[3]);

        // Level details popup (if shown) - render LAST so it overlays everything
        if app.show_level_details() {
            popups::render_level_details_popup(frame, app)?;
        }

        // Edit form popup (if shown) - render LAST so it overlays everything
        if app.show_edit_form() {
            forms::render_edit_form(frame, app)?;
        }

        // Create form popup (if shown) - render LAST so it overlays everything
        if app.show_create_form() {
            forms::render_create_form(frame, app)?;
        }

        Ok(())
    }
}
