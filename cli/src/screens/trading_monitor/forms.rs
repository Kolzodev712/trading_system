//! Form rendering functions for Trading Monitor
use crate::app::App;
use crate::error::CliError;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Render the edit form popup
pub fn render_edit_form(frame: &mut Frame, app: &App) -> Result<(), CliError> {
    let area = frame.size();

    // First, render a full-screen dark overlay
    let overlay_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(" ".repeat(area.width as usize)))
        .collect();
    let dark_overlay = Paragraph::new(overlay_lines).style(Style::default().bg(Color::Black));
    frame.render_widget(dark_overlay, area);

    // Create centered popup (75% width, 80% height)
    let popup_width = (area.width as f32 * 0.75) as u16;
    let popup_height = (area.height as f32 * 0.8) as u16;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_width = popup_width.min(area.width.saturating_sub(popup_x));
    let popup_height = popup_height.min(area.height.saturating_sub(popup_y));

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Fill popup area with solid background
    let popup_bg_fill = Paragraph::new(vec![
        Line::from(" ".repeat(popup_area.width as usize));
        popup_area.height as usize
    ])
    .style(Style::default().bg(Color::Rgb(30, 30, 30)));
    frame.render_widget(popup_bg_fill, popup_area);

    // Render bordered popup
    let popup_bg = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(30, 30, 30)).fg(Color::White));
    frame.render_widget(popup_bg, popup_area);

    // Inner content area
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
    let form = app.edit_form();
    if let Some(form) = form {
        let header_text = format!("Edit Level: {}", form.level_id);
        let header = Paragraph::new(header_text)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, chunks[0]);

        // Content - form fields
        let field_names = [
            "Enabled",
            "Price (Fixed only)",
            "Quantity (Fixed only)",
            "Risk Limit (Daily Loss)",
            "Max Position Size",
            "Repost on Fill",
            "Repost on Cancel",
        ];

        // field_values needs to be Vec because elements are conditionally computed
        let field_values = [
            if form.enabled { "✓ Yes" } else { "✗ No" }.to_string(),
            if form.editing && form.selected_field == 1 {
                format!("{}_", form.input_buffer)
            } else if form.price.is_empty() {
                "(unchanged)".to_string()
            } else {
                form.price.clone()
            },
            if form.editing && form.selected_field == 2 {
                format!("{}_", form.input_buffer)
            } else if form.quantity.is_empty() {
                "(unchanged)".to_string()
            } else {
                form.quantity.clone()
            },
            if form.editing && form.selected_field == 3 {
                format!("{}_", form.input_buffer)
            } else if form.risk_limit.is_empty() {
                "(unchanged)".to_string()
            } else {
                form.risk_limit.clone()
            },
            if form.editing && form.selected_field == 4 {
                format!("{}_", form.input_buffer)
            } else if form.max_position.is_empty() {
                "(unchanged)".to_string()
            } else {
                form.max_position.clone()
            },
            if form.repost_on_fill {
                "✓ Yes"
            } else {
                "✗ No"
            }
            .to_string(),
            if form.repost_on_cancel {
                "✓ Yes"
            } else {
                "✗ No"
            }
            .to_string(),
        ];

        let mut form_lines = Vec::new();
        for (i, (name, value)) in field_names.iter().zip(field_values.iter()).enumerate() {
            let is_selected = i == form.selected_field;
            let prefix = if is_selected { "> " } else { "  " };
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let value_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            form_lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(name.to_string(), name_style),
                Span::from(": "),
                Span::styled(value.clone(), value_style),
            ]));
        }

        let content = Paragraph::new(form_lines)
            .block(Block::default().title("Level Configuration"))
            .wrap(Wrap { trim: true });
        frame.render_widget(content, chunks[1]);
    } else {
        let error_msg = Paragraph::new("No edit form data available")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error_msg, chunks[1]);
    }

    // Footer
    let footer_text = vec![Line::from(Span::styled(
        "Enter: Save | Esc: Cancel",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))];
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);

    Ok(())
}

/// Render the create form popup
pub fn render_create_form(frame: &mut Frame, app: &App) -> Result<(), CliError> {
    let area = frame.size();

    // First, render a full-screen dark overlay
    let overlay_lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(" ".repeat(area.width as usize)))
        .collect();
    let dark_overlay = Paragraph::new(overlay_lines).style(Style::default().bg(Color::Black));
    frame.render_widget(dark_overlay, area);

    // Create centered popup (80% width, 85% height - larger for more fields)
    let popup_width = (area.width as f32 * 0.8) as u16;
    let popup_height = (area.height as f32 * 0.85) as u16;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_width = popup_width.min(area.width.saturating_sub(popup_x));
    let popup_height = popup_height.min(area.height.saturating_sub(popup_y));

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Fill popup area with solid background
    let popup_bg_fill = Paragraph::new(vec![
        Line::from(" ".repeat(popup_area.width as usize));
        popup_area.height as usize
    ])
    .style(Style::default().bg(Color::Rgb(30, 30, 30)));
    frame.render_widget(popup_bg_fill, popup_area);

    // Render bordered popup
    let popup_bg = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(30, 30, 30)).fg(Color::White));
    frame.render_widget(popup_bg, popup_area);

    // Inner content area
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
    let form = app.create_form();
    if let Some(form) = form {
        let header_text = "Create New Level";
        let header = Paragraph::new(header_text)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, chunks[0]);

        // Content - form fields
        let field_names = [
            "Level ID",
            "Symbol Base",
            "Symbol Quote",
            "Side",
            "Order Type",
            "Enabled",
            "Price (Fixed)",
            "Quantity (Fixed)",
            "Risk Limit (Daily Loss)",
            "Max Position Size",
            "Repost on Fill",
            "Repost on Cancel",
        ];

        // field_values needs to be Vec because elements are conditionally computed
        let field_values = [
            if form.editing && form.selected_field == 0 {
                format!("{}_", form.input_buffer)
            } else {
                form.level_id.clone()
            },
            if form.editing && form.selected_field == 1 {
                format!("{}_", form.input_buffer)
            } else {
                form.symbol_base.clone()
            },
            if form.editing && form.selected_field == 2 {
                format!("{}_", form.input_buffer)
            } else {
                form.symbol_quote.clone()
            },
            form.side.clone(),
            form.order_type.clone(),
            if form.enabled { "✓ Yes" } else { "✗ No" }.to_string(),
            if form.editing && form.selected_field == 6 {
                format!("{}_", form.input_buffer)
            } else {
                form.price.clone()
            },
            if form.editing && form.selected_field == 7 {
                format!("{}_", form.input_buffer)
            } else {
                form.quantity.clone()
            },
            if form.editing && form.selected_field == 8 {
                format!("{}_", form.input_buffer)
            } else {
                form.risk_limit.clone()
            },
            if form.editing && form.selected_field == 9 {
                format!("{}_", form.input_buffer)
            } else {
                form.max_position.clone()
            },
            if form.repost_on_fill {
                "✓ Yes"
            } else {
                "✗ No"
            }
            .to_string(),
            if form.repost_on_cancel {
                "✓ Yes"
            } else {
                "✗ No"
            }
            .to_string(),
        ];

        let mut form_lines = Vec::new();
        for (i, (name, value)) in field_names.iter().zip(field_values.iter()).enumerate() {
            let is_selected = i == form.selected_field;
            let prefix = if is_selected { "> " } else { "  " };
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let value_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            form_lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(name.to_string(), name_style),
                Span::from(": "),
                Span::styled(value.clone(), value_style),
            ]));
        }

        // Show available symbols hint if on symbol fields
        if (1..=2).contains(&form.selected_field) && !form.available_symbols.is_empty() {
            form_lines.push(Line::from(""));
            form_lines.push(Line::from(Span::styled(
                "Available symbols:",
                Style::default().fg(Color::DarkGray),
            )));
            for (base, quote) in &form.available_symbols {
                form_lines.push(Line::from(Span::styled(
                    format!("  {}/{}", base, quote),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        let content = Paragraph::new(form_lines)
            .block(Block::default().title("New Level Configuration"))
            .wrap(Wrap { trim: true });
        frame.render_widget(content, chunks[1]);
    } else {
        let error_msg = Paragraph::new("No create form data available")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error_msg, chunks[1]);
    }

    // Footer
    let footer_text = vec![Line::from(Span::styled(
        "Enter: Create | Esc: Cancel",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))];
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);

    Ok(())
}
