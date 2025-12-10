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
                Constraint::Length(5),  // Trading status
                Constraint::Length(12), // Navigation menu
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
            Line::from("Press [T] to toggle trading on/off"),
        ];

        let status_block = Block::default().borders(Borders::ALL).title("Status");
        let status_paragraph = Paragraph::new(status_text)
            .block(status_block)
            .wrap(Wrap { trim: true });
        frame.render_widget(status_paragraph, main_chunks[0]);

        // Navigation Menu
        let menu_items = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("[1] ", Style::default().fg(Color::Yellow)),
                Span::from("Trading Monitor"),
            ]),
            Line::from("     View orderbooks, indicators, orders, balances, trades,"),
            Line::from("     and manage levels"),
            Line::from(""),
            Line::from(vec![
                Span::styled("[2] ", Style::default().fg(Color::Yellow)),
                Span::from("System Monitor"),
            ]),
            Line::from("     View connectivity status, channel health, and system info"),
            Line::from(""),
            Line::from(vec![
                Span::styled("[3] ", Style::default().fg(Color::Yellow)),
                Span::from("Live Config"),
            ]),
            Line::from("     Replace entire config (levels.json or strategies.json)"),
            Line::from("     without restart"),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Q] ", Style::default().fg(Color::Yellow)),
                Span::from("Quit Application"),
            ]),
        ];

        let menu_block = Block::default().borders(Borders::ALL).title("Navigation");
        let menu_paragraph = Paragraph::new(menu_items)
            .block(menu_block)
            .wrap(Wrap { trim: true });
        frame.render_widget(menu_paragraph, main_chunks[1]);

        // Footer
        let footer_text = vec![Line::from(vec![
            Span::styled("Mode: ", Style::default()),
            Span::from("[Levels]"),
            Span::from("  "),
            Span::styled("Exchange: ", Style::default()),
            Span::from("[Binance]"),
            Span::from("  "),
            Span::styled("Uptime: ", Style::default()),
            Span::from("[0s]"), // TODO: Get actual uptime
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
                // TODO: Navigate to Trading Monitor
                // app.set_screen(Screen::TradingMonitor);
            }
            KeyCode::Char('2') => {
                // TODO: Navigate to System Monitor
                // app.set_screen(Screen::SystemMonitor);
            }
            KeyCode::Char('3') => {
                // TODO: Navigate to Live Config
                // app.set_screen(Screen::LiveConfig);
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                // Toggle trading
                let current_status = app.trading_status();
                let cancelled = app.set_trading_enabled(!current_status).await?;
                if !cancelled.is_empty() {
                    // TODO: Show notification about cancelled orders
                    eprintln!("Cancelled {} orders", cancelled.len());
                }
                // TODO: Show notification about cancelled orders
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
