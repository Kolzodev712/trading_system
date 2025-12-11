//! Popup rendering functions for Trading Monitor
use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Render the level details popup
pub fn render_level_details_popup(frame: &mut Frame, app: &App) -> Result<(), CliError> {
    let area = frame.size();

    // First, render a full-screen dark overlay to completely cover everything
    // Fill entire screen line by line with spaces to ensure complete coverage
    let overlay_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(" ".repeat(area.width as usize)))
        .collect();
    let dark_overlay = Paragraph::new(overlay_lines).style(Style::default().bg(Color::Black));
    frame.render_widget(dark_overlay, area);

    // Create centered popup (80% width, 70% height)
    let popup_width = (area.width as f32 * 0.8) as u16;
    let popup_height = (area.height as f32 * 0.7) as u16;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    // Ensure popup doesn't exceed terminal bounds
    let popup_width = popup_width.min(area.width.saturating_sub(popup_x));
    let popup_height = popup_height.min(area.height.saturating_sub(popup_y));

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Fill the entire popup area with solid background using spaces (line by line)
    let popup_fill_lines: Vec<Line> = (0..popup_area.height)
        .map(|_| Line::from(" ".repeat(popup_area.width as usize)))
        .collect();
    let popup_bg_fill =
        Paragraph::new(popup_fill_lines).style(Style::default().bg(Color::Rgb(20, 20, 20)));
    frame.render_widget(popup_bg_fill, popup_area);

    // Then render the bordered popup on top
    let popup_bg = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(20, 20, 20)).fg(Color::White));
    frame.render_widget(popup_bg, popup_area);

    // Inner content area (with padding)
    let inner_area = Rect::new(
        popup_area.x + 1,
        popup_area.y + 1,
        popup_area.width.saturating_sub(2),
        popup_area.height.saturating_sub(2),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(2), // Footer
        ])
        .split(inner_area);

    // Header
    let level = app.level_details();
    if let Some(level) = level {
        let header_text = format!("Level Details: {}", level.id);
        let header = Paragraph::new(header_text)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, chunks[0]);

        // Content - use single column with wrapped text to avoid horizontal overflow
        let content_info = format!(
            "{}\n\n{}",
            format_level_basic_info(level),
            format_level_config_info(level)
        );
        let content_paragraph = Paragraph::new(content_info)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Level Information"));
        frame.render_widget(content_paragraph, chunks[1]);
    } else {
        let error_msg = Paragraph::new("No level details available")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error_msg, chunks[1]);
    }

    // Footer
    let footer_text = vec![Line::from(vec![
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::from(" or "),
        Span::styled(
            "L",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::from(" Close"),
    ])];
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);

    Ok(())
}

fn format_level_basic_info(level: &crate::client::trading_cli::Level) -> String {
    let mut info = Vec::new();

    info.push(format!("ID: {}", level.id));
    info.push(format!("Status: {}", level.status));
    info.push(format!(
        "Enabled: {}",
        if level.enabled { "Yes" } else { "No" }
    ));

    if let Some(config) = &level.config {
        if let Some(symbol) = &config.symbol {
            info.push(format!("Pair: {}/{}", symbol.base, symbol.quote));
        }
        info.push(format!("Side: {}", config.side));
        info.push(format!("Order Type: {}", config.order_type));
    }

    info.push(format!("Active Orders: {}", level.active_orders.len()));
    if !level.active_orders.is_empty() {
        info.push(String::new());
        info.push("Order IDs:".to_string());
        for order_id in &level.active_orders {
            info.push(format!("  - {}", order_id));
        }
    }

    info.push(format!("Filled Count: {}", level.filled_count));

    info.join("\n")
}

fn format_level_config_info(level: &crate::client::trading_cli::Level) -> String {
    let mut info = Vec::new();

    if let Some(config) = &level.config {
        info.push("Price Config:".to_string());
        info.push(format!("  {}", config.price));
        info.push(String::new());

        info.push("Quantity Config:".to_string());
        info.push(format!("  {}", config.quantity));
        info.push(String::new());

        if !config.risk_limit.is_empty() {
            info.push(format!("Daily Loss Limit: {}", config.risk_limit));
        }
        if !config.max_position_size.is_empty() {
            info.push(format!("Max Position: {}", config.max_position_size));
        }
        info.push(String::new());

        info.push(format!("Repost on Fill: {}", config.repost_on_fill));
        info.push(format!("Repost on Cancel: {}", config.repost_on_cancel));
    }

    info.join("\n")
}
