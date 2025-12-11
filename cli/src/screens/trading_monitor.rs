//! Trading Monitor Screen - Displays real-time trading data
use crate::app::App;
use crate::error::CliError;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
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
                Span::from(" Toggle  │  "),
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
            Self::render_level_details_popup(frame, app)?;
        }

        // Edit form popup (if shown) - render LAST so it overlays everything
        if app.show_edit_form() {
            Self::render_edit_form(frame, app)?;
        }

        // Create form popup (if shown) - render LAST so it overlays everything
        if app.show_create_form() {
            Self::render_create_form(frame, app)?;
        }

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
        let selected_idx = app.selected_level_index();
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
                            Cell::from(l.id.clone()).style(row_style),
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
        // If create form is open, handle create form input
        if app.show_create_form() {
            // Handle actions that need to drop the form reference first
            match key {
                KeyCode::Esc => {
                    app.close_create_form();
                    return Ok(());
                }
                KeyCode::Enter => {
                    app.save_create_form().await?;
                    return Ok(());
                }
                _ => {}
            }

            // Handle other input with form reference
            if let Some(form) = app.create_form_mut() {
                match key {
                    KeyCode::Tab | KeyCode::Down => {
                        if form.editing {
                            form.editing = false;
                            form.input_buffer.clear();
                        } else {
                            form.selected_field = (form.selected_field + 1) % 12;
                        }
                    }
                    KeyCode::Up => {
                        if form.editing {
                            form.editing = false;
                            form.input_buffer.clear();
                        } else {
                            form.selected_field = if form.selected_field == 0 {
                                11
                            } else {
                                form.selected_field - 1
                            };
                        }
                    }
                    KeyCode::Char(' ') => {
                        // Toggle boolean/enum fields
                        match form.selected_field {
                            3 => {
                                // Toggle side
                                form.side = if form.side == "Buy" {
                                    "Sell".to_string()
                                } else {
                                    "Buy".to_string()
                                };
                            }
                            4 => {
                                // Cycle order type
                                form.order_type = match form.order_type.as_str() {
                                    "Limit" => "Market".to_string(),
                                    "Market" => "PostOnly".to_string(),
                                    "PostOnly" => "IoC".to_string(),
                                    "IoC" => "FoK".to_string(),
                                    _ => "Limit".to_string(),
                                };
                            }
                            5 => form.enabled = !form.enabled,
                            10 => form.repost_on_fill = !form.repost_on_fill,
                            11 => form.repost_on_cancel = !form.repost_on_cancel,
                            _ => {
                                // For text fields, start editing
                                form.editing = true;
                                match form.selected_field {
                                    0 => form.input_buffer = form.level_id.clone(),
                                    1 => form.input_buffer = form.symbol_base.clone(),
                                    2 => form.input_buffer = form.symbol_quote.clone(),
                                    6 => form.input_buffer = form.price.clone(),
                                    7 => form.input_buffer = form.quantity.clone(),
                                    8 => form.input_buffer = form.risk_limit.clone(),
                                    9 => form.input_buffer = form.max_position.clone(),
                                    _ => {}
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        // Handle text input
                        match form.selected_field {
                            0 | 1 | 2 | 6 | 7 | 8 | 9 => {
                                // Text fields
                                if !form.editing {
                                    form.editing = true;
                                    form.input_buffer.clear();
                                }
                                if form.selected_field == 0
                                    || form.selected_field == 1
                                    || form.selected_field == 2
                                {
                                    // Alphanumeric for ID and symbols
                                    if c.is_alphanumeric() || c == '_' || c == '-' {
                                        form.input_buffer.push(c);
                                    }
                                } else {
                                    // Numeric for price, quantity, risk
                                    if c.is_ascii_digit() || c == '.' || c == '-' {
                                        form.input_buffer.push(c);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Backspace => {
                        if form.editing {
                            form.input_buffer.pop();
                        }
                    }
                    _ => {}
                }

                // Update field values from input buffer when not editing
                if !form.editing && !form.input_buffer.is_empty() {
                    match form.selected_field {
                        0 => {
                            form.level_id = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        1 => {
                            form.symbol_base = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        2 => {
                            form.symbol_quote = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        6 => {
                            form.price = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        7 => {
                            form.quantity = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        8 => {
                            form.risk_limit = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        9 => {
                            form.max_position = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        _ => {}
                    }
                }
            }
            return Ok(());
        }

        // If edit form is open, handle edit form input
        if app.show_edit_form() {
            // Handle actions that need to drop the form reference first
            match key {
                KeyCode::Esc => {
                    app.close_edit_form();
                    return Ok(());
                }
                KeyCode::Enter => {
                    app.save_edit_form().await?;
                    return Ok(());
                }
                _ => {}
            }

            // Handle other input with form reference
            if let Some(form) = app.edit_form_mut() {
                match key {
                    KeyCode::Tab | KeyCode::Down => {
                        // Move to next field
                        if form.editing {
                            form.editing = false;
                            form.input_buffer.clear();
                        } else {
                            form.selected_field = (form.selected_field + 1) % 7;
                        }
                    }
                    KeyCode::Up => {
                        // Move to previous field
                        if form.editing {
                            form.editing = false;
                            form.input_buffer.clear();
                        } else {
                            form.selected_field = if form.selected_field == 0 {
                                6
                            } else {
                                form.selected_field - 1
                            };
                        }
                    }
                    KeyCode::Char(' ') => {
                        // Toggle boolean fields
                        match form.selected_field {
                            0 => form.enabled = !form.enabled,
                            5 => form.repost_on_fill = !form.repost_on_fill,
                            6 => form.repost_on_cancel = !form.repost_on_cancel,
                            _ => {
                                // For text fields, start editing
                                form.editing = true;
                                match form.selected_field {
                                    1 => form.input_buffer = form.price.clone(),
                                    2 => form.input_buffer = form.quantity.clone(),
                                    3 => form.input_buffer = form.risk_limit.clone(),
                                    4 => form.input_buffer = form.max_position.clone(),
                                    _ => {}
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        // Handle text input
                        match form.selected_field {
                            1 | 2 | 3 | 4 => {
                                // Text fields
                                if !form.editing {
                                    form.editing = true;
                                    form.input_buffer.clear();
                                }
                                if c.is_ascii_digit() || c == '.' || c == '-' {
                                    form.input_buffer.push(c);
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Backspace => {
                        // Delete character
                        if form.editing {
                            form.input_buffer.pop();
                        }
                    }
                    _ => {}
                }

                // Update field values from input buffer when not editing
                if !form.editing && !form.input_buffer.is_empty() {
                    match form.selected_field {
                        1 => {
                            form.price = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        2 => {
                            form.quantity = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        3 => {
                            form.risk_limit = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        4 => {
                            form.max_position = form.input_buffer.clone();
                            form.input_buffer.clear();
                        }
                        _ => {}
                    }
                }
            }
            return Ok(());
        }

        // Normal input handling (when edit form is not open)
        match key {
            KeyCode::Esc => {
                // If details popup is open, close it first
                if app.show_level_details() {
                    app.toggle_level_details().await?;
                } else {
                    // Otherwise navigate back to main menu
                    app.go_to_main_menu();
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Refresh data
                app.refresh_monitoring_data().await?;
            }
            KeyCode::Up => {
                // Select level up
                app.select_level_up();
            }
            KeyCode::Down => {
                // Select level down
                app.select_level_down();
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                // Toggle selected level
                app.toggle_selected_level().await?;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Toggle level details popup
                app.toggle_level_details().await?;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Open edit form
                app.open_edit_form().await?;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // Open create form
                app.open_create_form().await?;
            }
            _ => {}
        }
        Ok(())
    }

    fn render_level_details_popup(frame: &mut Frame, app: &App) -> Result<(), CliError> {
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
                Self::format_level_basic_info(level),
                Self::format_level_config_info(level)
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
            info.push("".to_string());
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
            info.push("".to_string());

            info.push("Quantity Config:".to_string());
            info.push(format!("  {}", config.quantity));
            info.push("".to_string());

            if !config.risk_limit.is_empty() {
                info.push(format!("Daily Loss Limit: {}", config.risk_limit));
            }
            if !config.max_position_size.is_empty() {
                info.push(format!("Max Position: {}", config.max_position_size));
            }
            info.push("".to_string());

            info.push(format!("Repost on Fill: {}", config.repost_on_fill));
            info.push(format!("Repost on Cancel: {}", config.repost_on_cancel));
        }

        info.join("\n")
    }

    fn render_edit_form(frame: &mut Frame, app: &App) -> Result<(), CliError> {
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
            let field_names = vec![
                "Enabled",
                "Price (Fixed only)",
                "Quantity (Fixed only)",
                "Risk Limit (Daily Loss)",
                "Max Position Size",
                "Repost on Fill",
                "Repost on Cancel",
            ];

            let field_values = vec![
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

    fn render_create_form(frame: &mut Frame, app: &App) -> Result<(), CliError> {
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
            let field_names = vec![
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

            let field_values = vec![
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
            if form.selected_field == 1 || form.selected_field == 2 {
                if !form.available_symbols.is_empty() {
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
}
