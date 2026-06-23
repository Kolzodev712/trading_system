//! Main Menu Screen
use crate::app::App;
use crate::error::CliError;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub struct MainMenuScreen;

impl MainMenuScreen {
    pub fn render(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        // Create vertical layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        // Header
        let header = Paragraph::new("Trading System Control")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Main content
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // Trading status
                Constraint::Length(6), // Navigation menu (reduced since fewer items)
            ])
            .split(chunks[1]);

        // Trading Status
        let trading_status = if app.trading_status() {
            "ENABLED"
        } else {
            "DISABLED"
        };
        let status_color = if app.trading_status() {
            Color::Green
        } else {
            Color::Red
        };

        let status_text = vec![
            Line::from(vec![
                Span::styled("Trading Status: ", Style::default()),
                Span::styled(
                    format!("[● {}]", trading_status),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "T",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from("  Toggle trading"),
            ]),
        ];

        let status_block = Block::default().borders(Borders::ALL).title("Status");
        let status_paragraph = Paragraph::new(status_text)
            .block(status_block)
            .wrap(Wrap { trim: true });
        frame.render_widget(status_paragraph, main_chunks[0]);

        // Navigation Menu
        let monitor_hint = if app.has_dex() {
            "On-chain venue: orders, balances, pending txs, orderbooks"
        } else {
            "Orders, balances, trades, indicators, level management"
        };
        let menu_items = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "1",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from("  Trading Monitor"),
            ]),
            Line::from(format!("   {monitor_hint}")),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Q",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from("  Quit"),
            ]),
        ];

        let menu_block = Block::default().borders(Borders::ALL).title("Navigation");
        let menu_paragraph = Paragraph::new(menu_items)
            .block(menu_block)
            .wrap(Wrap { trim: true });
        frame.render_widget(menu_paragraph, main_chunks[1]);

        // Footer
        let mode_label = if app.has_dex() {
            "DEX"
        } else if app.has_strategy() {
            "Strategies"
        } else {
            "Levels"
        };
        let footer_text = vec![Line::from(vec![
            Span::styled("Mode: ", Style::default().fg(Color::DarkGray)),
            Span::styled(mode_label, Style::default().fg(Color::Cyan)),
            Span::from("  │  "),
            Span::styled("Exchange: ", Style::default().fg(Color::DarkGray)),
            Span::styled(app.exchange_name(), Style::default().fg(Color::Cyan)),
        ])];
        let footer_paragraph = Paragraph::new(footer_text)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer_paragraph, chunks[2]);

        Ok(())
    }

    pub async fn handle_input_async(app: &mut App, key: KeyCode) -> Result<(), CliError> {
        match key {
            KeyCode::Char('1') => {
                // Navigate to Trading Monitor and refresh data
                app.go_to_trading_monitor().await?;
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                // Toggle trading
                let current_status = app.trading_status();
                if app.set_trading_enabled(!current_status).await.is_ok() {
                    let _ = app.refresh_trading_status().await;
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.quit();
            }
            KeyCode::Esc => {
                app.quit();
            }
            _ => {}
        }
        Ok(())
    }
}
