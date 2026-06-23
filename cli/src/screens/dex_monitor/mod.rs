//! DEX Monitor — venue snapshot and quote controls (Phase 2).

mod input;
mod panels;

use crate::app::App;
use crate::error::CliError;
use crate::screens::shared::panels as shared_panels;
use input::handle_input_async;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct DexMonitorScreen;

impl DexMonitorScreen {
    pub async fn handle_input_async(
        app: &mut App,
        key: crossterm::event::KeyCode,
    ) -> Result<(), CliError> {
        handle_input_async(app, key).await
    }

    pub fn render(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
        let dex = app.monitoring_data().and_then(|md| md.dex.as_ref());
        let pool_filter = app.selected_orderbook_pair_label();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Percentage(22),
                Constraint::Length(3),
            ])
            .split(area);

        let venue_label = dex.map(|d| d.venue.as_str()).unwrap_or("dex");
        let exchange = app.exchange_name();
        let header = Paragraph::new(format!("DEX Monitor — {venue_label} · {exchange}"))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(chunks[1]);

        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Length(9),
                Constraint::Min(0),
            ])
            .split(main[0]);

        panels::render_venue(frame, left[0], app)?;
        panels::render_dex_indicators(frame, left[1], dex, pool_filter.as_deref())?;
        panels::render_dex_open_orders(frame, left[2], dex, pool_filter.as_deref())?;

        shared_panels::render_orderbook(frame, main[1], app)?;

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main[2]);

        panels::render_dex_balances(frame, right[0], dex)?;
        panels::render_pending_transactions(frame, right[1], dex)?;

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(chunks[2]);

        panels::render_dex_quotes(
            frame,
            bottom[0],
            dex,
            pool_filter.as_deref(),
            app.selected_dex_quote_index(),
        )?;
        panels::render_dex_strategies(
            frame,
            bottom[1],
            dex,
            pool_filter.as_deref(),
            app.selected_dex_strategy_index(),
        )?;
        shared_panels::render_trades(frame, bottom[2], app)?;

        let footer = Paragraph::new(vec![Line::from(vec![
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
                "←/→",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Pool tabs  │  "),
            Span::styled(
                "↑/↓",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Quote  │  "),
            Span::styled(
                "T",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Toggle  │  "),
            Span::styled(
                "[/]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Strategy  │  "),
            Span::styled(
                "S",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Strat toggle  │  "),
            Span::styled(
                "C",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(" Cancel pool orders"),
        ])])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[3]);

        Ok(())
    }
}
