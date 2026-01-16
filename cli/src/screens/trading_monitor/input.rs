//! Input handling for Trading Monitor
use crate::app::App;
use crate::error::CliError;
use crossterm::event::KeyCode;

/// Handle input for the trading monitor screen
pub async fn handle_input_async(app: &mut App, key: KeyCode) -> Result<(), CliError> {
    // If create form is open, handle create form input
    if app.show_create_form() {
        return handle_create_form_input(app, key).await;
    }

    // If edit form is open, handle edit form input
    if app.show_edit_form() {
        return handle_edit_form_input(app, key).await;
    }

    // Normal input handling (when forms are not open)
    handle_normal_input(app, key).await
}

async fn handle_create_form_input(app: &mut App, key: KeyCode) -> Result<(), CliError> {
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
    Ok(())
}

async fn handle_edit_form_input(app: &mut App, key: KeyCode) -> Result<(), CliError> {
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
                if (1..=4).contains(&form.selected_field) {
                    // Text fields
                    if !form.editing {
                        form.editing = true;
                        form.input_buffer.clear();
                    }
                    if c.is_ascii_digit() || c == '.' || c == '-' {
                        form.input_buffer.push(c);
                    }
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
    Ok(())
}

async fn handle_normal_input(app: &mut App, key: KeyCode) -> Result<(), CliError> {
    match key {
        KeyCode::Esc => {
            // If details popup is open, close it first
            if app.show_level_details() {
                app.toggle_level_details().await?;
            } else if app.show_strategy_details() {
                app.toggle_strategy_details().await?;
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
            // Select up (level or strategy depending on mode)
            if app.has_strategy() {
                app.select_strategy_up();
            } else {
                app.select_level_up();
            }
        }
        KeyCode::Down => {
            // Select down (level or strategy depending on mode)
            if app.has_strategy() {
                app.select_strategy_down();
            } else {
                app.select_level_down();
            }
        }
        KeyCode::Left => {
            // Navigate orderbook pairs left
            app.select_orderbook_pair_left();
        }
        KeyCode::Right => {
            // Navigate orderbook pairs right
            app.select_orderbook_pair_right();
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            // In strategies mode: toggle selected strategy
            // In levels mode: toggle selected level
            if app.has_strategy() {
                // Toggle selected strategy enabled/disabled
                app.toggle_selected_strategy().await?;
            } else {
                // Toggle selected level
                app.toggle_selected_level().await?;
            }
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            // Toggle level/strategy details popup
            if app.has_strategy() {
                app.toggle_strategy_details().await?;
            } else {
                app.toggle_level_details().await?;
            }
        }
        KeyCode::Char('e') | KeyCode::Char('E') => {
            // Open edit form (only in levels mode)
            if !app.has_strategy() {
                app.open_edit_form().await?;
            }
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            // Open create form (only in levels mode)
            if !app.has_strategy() {
                app.open_create_form().await?;
            }
        }
        _ => {}
    }
    Ok(())
}
