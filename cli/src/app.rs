//! Main application state and event loop
use crate::client::trading_cli::MonitoringData;
use crate::client::TradingCliClient;
use crate::error::CliError;
use crate::screens::main_menu::MainMenuScreen;
use crate::screens::trading_monitor::{handle_input_async, TradingMonitorScreen};
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::time::Duration;
use tracing::error;

pub struct App {
    client: Option<TradingCliClient>,
    endpoint: String,
    pub current_screen: Screen,
    should_quit: bool,
    trading_status: bool,
    monitoring_data: Option<MonitoringData>,
    levels: Option<Vec<crate::client::trading_cli::Level>>,
    strategies: Option<Vec<crate::client::trading_cli::StrategyStatus>>, // Strategies list
    streaming_active: bool,
    selected_level_index: Option<usize>, // Selected level index in levels list
    selected_strategy_index: Option<usize>, // Selected strategy index in strategies list
    selected_orderbook_pair_index: Option<usize>, // Selected orderbook pair index
    level_details: Option<crate::client::trading_cli::Level>, // Currently viewed level details
    show_level_details: bool,            // Whether to show level details popup
    show_strategy_details: bool,         // Whether to show strategy details popup
    edit_form: Option<EditFormState>,    // Edit form state
    create_form: Option<CreateFormState>, // Create form state
}

#[derive(Debug, Clone)]
pub struct CreateFormState {
    pub level_id: String,
    pub symbol_base: String,
    pub symbol_quote: String,
    pub side: String,       // "Buy" or "Sell"
    pub order_type: String, // "Limit", "Market", etc.
    pub enabled: bool,
    pub price: String,        // For Fixed price only
    pub quantity: String,     // For Fixed quantity only
    pub risk_limit: String,   // Daily loss limit
    pub max_position: String, // Max position size
    pub repost_on_fill: bool,
    pub repost_on_cancel: bool,
    pub selected_field: usize, // 0=id, 1=base, 2=quote, 3=side, 4=order_type, 5=enabled, 6=price, 7=quantity, 8=risk_limit, 9=max_position, 10=repost_on_fill, 11=repost_on_cancel
    pub input_buffer: String,  // Current input buffer for text fields
    pub editing: bool,         // Whether currently editing a text field
    pub available_symbols: Vec<(String, String)>, // (base, quote) pairs from existing levels
}

#[derive(Debug, Clone)]
pub struct EditFormState {
    pub level_id: String,
    pub enabled: bool,
    pub price: String,        // For Fixed price only
    pub quantity: String,     // For Fixed quantity only
    pub risk_limit: String,   // Daily loss limit
    pub max_position: String, // Max position size
    pub repost_on_fill: bool,
    pub repost_on_cancel: bool,
    pub selected_field: usize, // 0=enabled, 1=price, 2=quantity, 3=risk_limit, 4=max_position, 5=repost_on_fill, 6=repost_on_cancel
    pub input_buffer: String,  // Current input buffer for text fields
    pub editing: bool,         // Whether currently editing a text field
}

#[derive(Debug)]
pub enum Screen {
    MainMenu,
    TradingMonitor,
}

impl App {
    pub fn new(endpoint: String) -> Result<Self, CliError> {
        Ok(Self {
            client: None, // Will connect when needed
            endpoint,
            current_screen: Screen::MainMenu,
            should_quit: false,
            trading_status: false, // Default to disabled until server says otherwise
            monitoring_data: None,
            levels: None,
            strategies: None,
            streaming_active: false,
            selected_level_index: None,
            selected_strategy_index: None,
            selected_orderbook_pair_index: None,
            level_details: None,
            show_level_details: false,
            show_strategy_details: false,
            edit_form: None,
            create_form: None,
        })
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), CliError> {
        // Connect to gRPC server
        match TradingCliClient::connect(&self.endpoint).await {
            Ok(mut client) => {
                // Fetch initial trading status
                if let Ok(status) = client.get_trading_status().await {
                    self.trading_status = status;
                }
                self.client = Some(client);

                // Preload monitoring data so mode (levels/strategies) is accurate on first render
                // This is critical for showing the correct mode in the footer
                if let Err(e) = self.refresh_monitoring_data().await {
                    tracing::warn!("[CLI] Failed to fetch initial monitoring data: {:?}", e);
                    // Continue anyway - streaming will update it
                }
            }
            Err(e) => {
                // Draw error message before returning
                terminal.draw(|f| {
                    use ratatui::{
                        layout::Alignment,
                        style::{Color, Style},
                        widgets::{Block, Borders, Paragraph},
                    };
                    let error_msg = format!(
                        "Failed to connect to trading system at: {}\n\nError: {}\n\nPress 'q' to quit",
                        self.endpoint, e
                    );
                    let paragraph = Paragraph::new(error_msg)
                        .style(Style::default().fg(Color::Red))
                        .alignment(Alignment::Center)
                        .block(Block::default().borders(Borders::ALL).title("Connection Error"));
                    f.render_widget(paragraph, f.size());
                })?;

                // Wait for user to press 'q'
                loop {
                    if crossterm::event::poll(Duration::from_millis(100))? {
                        if let crossterm::event::Event::Key(key_event) = crossterm::event::read()? {
                            if key_event.code == crossterm::event::KeyCode::Char('q') {
                                break;
                            }
                        }
                    }
                }

                return Err(e);
            }
        }

        // Skip initial status fetch to avoid blocking startup
        // Status will default to enabled, and will be fetched when user interacts

        // Channel for streaming updates
        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel::<MonitoringData>(10);

        // Initial draw to show the screen immediately
        terminal.draw(|f| {
            match self.current_screen {
                Screen::MainMenu => {
                    if let Err(e) = MainMenuScreen::render(f, f.size(), self) {
                        // Fallback: show error message
                        use ratatui::{
                            layout::Alignment,
                            style::{Color, Style},
                            widgets::{Block, Borders, Paragraph},
                        };
                        let error_msg = format!("Render error: {}", e);
                        let paragraph = Paragraph::new(error_msg)
                            .style(Style::default().fg(Color::Red))
                            .alignment(Alignment::Center)
                            .block(Block::default().borders(Borders::ALL).title("Error"));
                        f.render_widget(paragraph, f.size());
                    }
                }
                Screen::TradingMonitor => {
                    if let Err(e) = TradingMonitorScreen::render(f, f.size(), self) {
                        // Fallback: show error message
                        use ratatui::{
                            layout::Alignment,
                            style::{Color, Style},
                            widgets::{Block, Borders, Paragraph},
                        };
                        let error_msg = format!("Render error: {}", e);
                        let paragraph = Paragraph::new(error_msg)
                            .style(Style::default().fg(Color::Red))
                            .alignment(Alignment::Center)
                            .block(Block::default().borders(Borders::ALL).title("Error"));
                        f.render_widget(paragraph, f.size());
                    }
                }
            }
        })?;

        let mut needs_redraw = true; // Initial redraw
        let mut level_refresh_interval = tokio::time::interval(Duration::from_secs(5)); // Refresh levels every 5 seconds
        level_refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            // Start streaming if we're in Trading Monitor and not already streaming
            if matches!(self.current_screen, Screen::TradingMonitor) && !self.streaming_active {
                // Refresh levels immediately when entering
                if self.refresh_levels().await.is_err() {
                    // Ignore errors, will retry next cycle
                }
                if let Err(e) = self.start_streaming(stream_tx.clone()).await {
                    error!("[CLI] Event loop: failed to start streaming: {:?}", e);
                }
                needs_redraw = true;
            }

            // Stop streaming if we left Trading Monitor
            if !matches!(self.current_screen, Screen::TradingMonitor) && self.streaming_active {
                self.streaming_active = false;
                needs_redraw = true;
            }

            // Only redraw when needed
            if needs_redraw {
                terminal.draw(|f| match self.current_screen {
                    Screen::MainMenu => {
                        if let Err(e) = MainMenuScreen::render(f, f.size(), self) {
                            use ratatui::{
                                layout::Alignment,
                                style::{Color, Style},
                                widgets::{Block, Borders, Paragraph},
                            };
                            let error_msg = format!("Render error: {}", e);
                            let paragraph = Paragraph::new(error_msg)
                                .style(Style::default().fg(Color::Red))
                                .alignment(Alignment::Center)
                                .block(Block::default().borders(Borders::ALL).title("Error"));
                            f.render_widget(paragraph, f.size());
                        }
                    }
                    Screen::TradingMonitor => {
                        if let Err(e) = TradingMonitorScreen::render(f, f.size(), self) {
                            use ratatui::{
                                layout::Alignment,
                                style::{Color, Style},
                                widgets::{Block, Borders, Paragraph},
                            };
                            let error_msg = format!("Render error: {}", e);
                            let paragraph = Paragraph::new(error_msg)
                                .style(Style::default().fg(Color::Red))
                                .alignment(Alignment::Center)
                                .block(Block::default().borders(Borders::ALL).title("Error"));
                            f.render_widget(paragraph, f.size());
                        }
                    }
                })?;
                needs_redraw = false;
            }

            // Use tokio::select! to handle keyboard input, streaming updates, and periodic level refresh
            tokio::select! {
                // Handle keyboard input - use a shorter poll timeout
                result = tokio::task::spawn_blocking(|| -> Option<crossterm::event::KeyCode> {
                    // Poll with very short timeout to avoid blocking
                    if let Ok(true) = crossterm::event::poll(Duration::from_millis(10)) {
                        if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                            return Some(key.code);
                        }
                    }
                    None
                }) => {
                    match result {
                        Ok(Some(key_code)) => {
                            match self.current_screen {
                                Screen::MainMenu => {
                                    if MainMenuScreen::handle_input_async(self, key_code).await.is_ok() {
                                        needs_redraw = true;
                                    }
                                }
                                Screen::TradingMonitor => {
                                    if handle_input_async(self, key_code).await.is_ok() {
                                        needs_redraw = true;
                                    }
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(_) => {}
                    }
                }

                // Handle streaming updates
                update = stream_rx.recv() => {
                    match update {
                        Some(data) => {
                            self.monitoring_data = Some(data.clone());
                            // Extract strategies from monitoring data
                            self.strategies = if !data.strategies.is_empty() {
                                Some(data.strategies.clone())
                            } else {
                                None
                            };
                            needs_redraw = true; // Data updated, need redraw
                        }
                        None => {
                            // Channel closed - this means the stream handler stopped
                            // Try to restart streaming if we're still in Trading Monitor
                            if matches!(self.current_screen, Screen::TradingMonitor) {
                                self.streaming_active = false;
                            }
                        }
                    }
                }

                // Periodic level refresh (only when in Trading Monitor)
                _ = level_refresh_interval.tick(), if matches!(self.current_screen, Screen::TradingMonitor) => {
                    if self.refresh_levels().await.is_err() {
                        // Ignore errors, will retry next cycle
                    } else {
                        needs_redraw = true; // Levels updated, need redraw
                    }
                }

                // Small delay to prevent busy-waiting
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    // Continue loop
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn trading_status(&self) -> bool {
        self.trading_status
    }

    pub async fn refresh_trading_status(&mut self) -> Result<(), CliError> {
        if let Some(ref mut client) = self.client {
            self.trading_status = client.get_trading_status().await?;
        }
        Ok(())
    }

    pub async fn set_trading_enabled(&mut self, enabled: bool) -> Result<Vec<String>, CliError> {
        if let Some(ref mut client) = self.client {
            // Use timeout to avoid blocking
            match tokio::time::timeout(Duration::from_secs(2), client.set_trading_enabled(enabled))
                .await
            {
                Ok(Ok(cancelled)) => {
                    self.trading_status = enabled;
                    Ok(cancelled)
                }
                Ok(Err(_)) => Ok(vec![]),
                Err(_) => {
                    self.trading_status = enabled;
                    Ok(vec![])
                }
            }
        } else {
            Ok(vec![])
        }
    }

    pub fn monitoring_data(&self) -> Option<&MonitoringData> {
        self.monitoring_data.as_ref()
    }

    /// Check if we're in strategies mode (has strategy data)
    pub fn has_strategy(&self) -> bool {
        self.monitoring_data
            .as_ref()
            .map(|md| !md.strategies.is_empty())
            .unwrap_or(false)
    }

    pub fn strategies(&self) -> Option<&Vec<crate::client::trading_cli::StrategyStatus>> {
        self.strategies.as_ref()
    }

    pub fn selected_strategy_index(&self) -> Option<usize> {
        self.selected_strategy_index
    }

    pub fn selected_orderbook_pair_index(&self) -> Option<usize> {
        self.selected_orderbook_pair_index
    }

    pub fn select_orderbook_pair_left(&mut self) {
        if let Some(md) = &self.monitoring_data {
            let orderbooks = &md.orderbooks;
            if orderbooks.is_empty() {
                self.selected_orderbook_pair_index = None;
                return;
            }
            self.selected_orderbook_pair_index = Some(
                self.selected_orderbook_pair_index
                    .map(|idx| {
                        if idx == 0 {
                            orderbooks.len() - 1
                        } else {
                            idx - 1
                        }
                    })
                    .unwrap_or(0),
            );
        }
    }

    pub fn select_orderbook_pair_right(&mut self) {
        if let Some(md) = &self.monitoring_data {
            let orderbooks = &md.orderbooks;
            if orderbooks.is_empty() {
                self.selected_orderbook_pair_index = None;
                return;
            }
            let max_idx = orderbooks.len() - 1;
            self.selected_orderbook_pair_index = Some(
                self.selected_orderbook_pair_index
                    .map(|idx| if idx >= max_idx { 0 } else { idx + 1 })
                    .unwrap_or(0),
            );
        }
    }

    pub fn selected_strategy(&self) -> Option<&crate::client::trading_cli::StrategyStatus> {
        if let (Some(strategies), Some(idx)) =
            (self.strategies.as_ref(), self.selected_strategy_index)
        {
            strategies.get(idx)
        } else {
            None
        }
    }

    pub fn show_strategy_details(&self) -> bool {
        self.show_strategy_details
    }

    pub async fn toggle_strategy_details(&mut self) -> Result<(), CliError> {
        if self.show_strategy_details {
            // Close details
            self.show_strategy_details = false;
        } else {
            // Show details for selected strategy (data is already in monitoring_data)
            if self.selected_strategy().is_some() {
                self.show_strategy_details = true;
            }
        }
        Ok(())
    }

    /// Strategy selection methods (similar to level selection)
    pub fn select_strategy_up(&mut self) {
        if let Some(strategies) = &self.strategies {
            if strategies.is_empty() {
                self.selected_strategy_index = None;
                return;
            }
            let max_idx = strategies.len() - 1;
            self.selected_strategy_index = Some(
                self.selected_strategy_index
                    .map(|idx| if idx == 0 { max_idx } else { idx - 1 })
                    .unwrap_or(0),
            );
        }
    }

    pub fn select_strategy_down(&mut self) {
        if let Some(strategies) = &self.strategies {
            if strategies.is_empty() {
                self.selected_strategy_index = None;
                return;
            }
            let max_idx = strategies.len() - 1;
            self.selected_strategy_index = Some(
                self.selected_strategy_index
                    .map(|idx| if idx >= max_idx { 0 } else { idx + 1 })
                    .unwrap_or(0),
            );
        }
    }

    pub fn levels(&self) -> Option<&Vec<crate::client::trading_cli::Level>> {
        self.levels.as_ref()
    }

    pub async fn refresh_monitoring_data(&mut self) -> Result<(), CliError> {
        if let Some(ref mut client) = self.client {
            // Use timeout to avoid blocking, but increase timeout for initial fetch
            match tokio::time::timeout(Duration::from_secs(5), client.get_monitoring_data()).await {
                Ok(Ok(data)) => {
                    self.monitoring_data = Some(data.clone());
                    // Extract strategies from monitoring data
                    self.strategies = if !data.strategies.is_empty() {
                        Some(data.strategies.clone())
                    } else {
                        None
                    };
                    tracing::debug!(
                        "[CLI] Monitoring data refreshed: {} strategies, {} orders, {} balances",
                        data.strategies.len(),
                        data.orders.len(),
                        data.balances.len()
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!("[CLI] Failed to get monitoring data: {:?}", e);
                    return Err(e);
                }
                Err(_) => {
                    tracing::warn!("[CLI] Timeout waiting for monitoring data");
                    return Err(CliError::Custom(
                        "Timeout waiting for monitoring data".to_string(),
                    ));
                }
            }
        } else {
            return Err(CliError::Custom("Client not initialized".to_string()));
        }
        Ok(())
    }

    pub async fn go_to_trading_monitor(&mut self) -> Result<(), CliError> {
        self.current_screen = Screen::TradingMonitor;
        // Reset selection when entering
        self.selected_level_index = None;
        // Fetch levels when entering trading monitor
        self.refresh_levels().await?;
        // Don't block - just switch screen, data will come from streaming
        // The streaming will start automatically in the event loop
        Ok(())
    }

    pub async fn refresh_levels(&mut self) -> Result<(), CliError> {
        if let Some(ref mut client) = self.client {
            match tokio::time::timeout(Duration::from_secs(2), client.get_levels()).await {
                Ok(Ok(levels)) => {
                    let levels_len = levels.len();
                    self.levels = Some(levels);
                    // Reset selection if it's out of bounds
                    if let Some(idx) = self.selected_level_index {
                        if idx >= levels_len {
                            self.selected_level_index =
                                if levels_len == 0 { None } else { Some(0) };
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(_) => {}
            }
        }
        Ok(())
    }

    pub fn selected_level_index(&self) -> Option<usize> {
        self.selected_level_index
    }

    pub fn select_level_up(&mut self) {
        if let Some(levels) = &self.levels {
            if levels.is_empty() {
                self.selected_level_index = None;
                return;
            }
            self.selected_level_index = Some(
                self.selected_level_index
                    .map(|idx| if idx == 0 { levels.len() - 1 } else { idx - 1 })
                    .unwrap_or(0),
            );
        }
    }

    pub fn select_level_down(&mut self) {
        if let Some(levels) = &self.levels {
            if levels.is_empty() {
                self.selected_level_index = None;
                return;
            }
            let max_idx = levels.len() - 1;
            self.selected_level_index = Some(
                self.selected_level_index
                    .map(|idx| if idx >= max_idx { 0 } else { idx + 1 })
                    .unwrap_or(0),
            );
        }
    }

    pub async fn toggle_selected_level(&mut self) -> Result<(), CliError> {
        if let Some(idx) = self.selected_level_index {
            // Get level_id before making async call
            let level_id = self
                .levels
                .as_ref()
                .and_then(|levels| levels.get(idx))
                .map(|level| (level.id.clone(), !level.enabled));

            if let Some((level_id, new_enabled)) = level_id {
                if let Some(ref mut client) = self.client {
                    match tokio::time::timeout(
                        Duration::from_secs(2),
                        client.toggle_level(&level_id, new_enabled),
                    )
                    .await
                    {
                        Ok(Ok(success)) => {
                            if success {
                                // Refresh levels to get updated state
                                self.refresh_levels().await?;
                            }
                        }
                        Ok(Err(_)) => {}
                        Err(_) => {}
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn toggle_selected_strategy(&mut self) -> Result<(), CliError> {
        // Extract strategy info before mutable borrow
        let (strategy_name, symbol, new_enabled) = if let Some(strategy) = self.selected_strategy()
        {
            (
                strategy.strategy_name.clone(),
                strategy.symbol.clone(),
                !strategy.enabled,
            )
        } else {
            return Ok(()); // No strategy selected
        };

        if let Some(ref mut client) = self.client {
            match tokio::time::timeout(
                Duration::from_secs(2),
                client.toggle_strategy(&strategy_name, &symbol, new_enabled),
            )
            .await
            {
                Ok(Ok(_cancelled_orders)) => {
                    // Refresh monitoring data to get updated state
                    self.refresh_monitoring_data().await?;
                }
                Ok(Err(_)) => {}
                Err(_) => {}
            }
        }
        Ok(())
    }

    pub fn go_to_main_menu(&mut self) {
        self.current_screen = Screen::MainMenu;
        self.streaming_active = false; // Stop streaming when leaving Trading Monitor
        self.show_level_details = false; // Close details popup
        self.level_details = None;
        self.show_strategy_details = false; // Close strategy details popup
        self.edit_form = None; // Close edit form
        self.create_form = None; // Close create form
    }

    pub fn show_level_details(&self) -> bool {
        self.show_level_details
    }

    pub fn level_details(&self) -> Option<&crate::client::trading_cli::Level> {
        self.level_details.as_ref()
    }

    pub async fn toggle_level_details(&mut self) -> Result<(), CliError> {
        if self.show_level_details {
            // Close details
            self.show_level_details = false;
            self.level_details = None;
        } else {
            // Show details for selected level
            if let Some(idx) = self.selected_level_index {
                if let Some(levels) = &self.levels {
                    if let Some(level) = levels.get(idx) {
                        if let Some(ref mut client) = self.client {
                            match tokio::time::timeout(
                                Duration::from_secs(2),
                                client.get_level(&level.id),
                            )
                            .await
                            {
                                Ok(Ok(details)) => {
                                    self.level_details = Some(details);
                                    self.show_level_details = true;
                                }
                                Ok(Err(e)) => {
                                    tracing::warn!("Failed to fetch level details: {:?}", e);
                                }
                                Err(_) => {
                                    tracing::warn!("Timeout fetching level details");
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn show_edit_form(&self) -> bool {
        self.edit_form.is_some()
    }

    pub fn edit_form(&self) -> Option<&EditFormState> {
        self.edit_form.as_ref()
    }

    pub fn edit_form_mut(&mut self) -> Option<&mut EditFormState> {
        self.edit_form.as_mut()
    }

    pub async fn open_edit_form(&mut self) -> Result<(), CliError> {
        if let Some(idx) = self.selected_level_index {
            if let Some(levels) = &self.levels {
                if let Some(level) = levels.get(idx) {
                    // Parse price and quantity from strings
                    let price = if level
                        .config
                        .as_ref()
                        .map(|c| c.price.starts_with("Fixed("))
                        .unwrap_or(false)
                    {
                        level
                            .config
                            .as_ref()
                            .and_then(|c| c.price.strip_prefix("Fixed("))
                            .and_then(|s| s.strip_suffix(')'))
                            .unwrap_or("")
                            .to_string()
                    } else {
                        String::new()
                    };

                    let quantity = if level
                        .config
                        .as_ref()
                        .map(|c| c.quantity.starts_with("Fixed("))
                        .unwrap_or(false)
                    {
                        level
                            .config
                            .as_ref()
                            .and_then(|c| c.quantity.strip_prefix("Fixed("))
                            .and_then(|s| s.strip_suffix(')'))
                            .unwrap_or("")
                            .to_string()
                    } else {
                        String::new()
                    };

                    let config = level.config.as_ref();
                    self.edit_form = Some(EditFormState {
                        level_id: level.id.clone(),
                        enabled: level.enabled,
                        price,
                        quantity,
                        risk_limit: config
                            .and_then(|c| {
                                if c.risk_limit.is_empty() {
                                    None
                                } else {
                                    Some(c.risk_limit.clone())
                                }
                            })
                            .unwrap_or_default(),
                        max_position: config
                            .and_then(|c| {
                                if c.max_position_size.is_empty() {
                                    None
                                } else {
                                    Some(c.max_position_size.clone())
                                }
                            })
                            .unwrap_or_default(),
                        repost_on_fill: config.map(|c| c.repost_on_fill).unwrap_or(true),
                        repost_on_cancel: config.map(|c| c.repost_on_cancel).unwrap_or(true),
                        selected_field: 0,
                        input_buffer: String::new(),
                        editing: false,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn close_edit_form(&mut self) {
        self.edit_form = None;
    }

    pub async fn save_edit_form(&mut self) -> Result<(), CliError> {
        if let Some(form) = &self.edit_form {
            let config = crate::client::trading_cli::LevelConfig {
                id: form.level_id.clone(),
                symbol: None,              // Keep existing
                side: String::new(),       // Keep existing
                order_type: String::new(), // Keep existing
                price: if form.price.is_empty() {
                    String::new() // Keep existing
                } else {
                    format!("Fixed({})", form.price)
                },
                quantity: if form.quantity.is_empty() {
                    String::new() // Keep existing
                } else {
                    format!("Fixed({})", form.quantity)
                },
                enabled: form.enabled,
                risk_limit: form.risk_limit.clone(),
                max_position_size: form.max_position.clone(),
                repost_on_fill: form.repost_on_fill,
                repost_on_cancel: form.repost_on_cancel,
            };

            if let Some(ref mut client) = self.client {
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    client.update_level(&form.level_id, config),
                )
                .await
                {
                    Ok(Ok(success)) => {
                        if success {
                            // Refresh levels to get updated state
                            self.refresh_levels().await?;
                            self.close_edit_form();
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Failed to update level: {:?}", e);
                    }
                    Err(_) => {
                        tracing::warn!("Timeout updating level");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn show_create_form(&self) -> bool {
        self.create_form.is_some()
    }

    pub fn create_form(&self) -> Option<&CreateFormState> {
        self.create_form.as_ref()
    }

    pub fn create_form_mut(&mut self) -> Option<&mut CreateFormState> {
        self.create_form.as_mut()
    }

    pub async fn open_create_form(&mut self) -> Result<(), CliError> {
        // Collect available symbols from existing levels using HashSet for O(1) deduplication
        use std::collections::HashSet;
        let mut available_symbols_set = HashSet::new();
        if let Some(levels) = &self.levels {
            for level in levels {
                if let Some(config) = &level.config {
                    if let Some(symbol) = &config.symbol {
                        available_symbols_set.insert((symbol.base.clone(), symbol.quote.clone()));
                    }
                }
            }
        }
        let available_symbols: Vec<_> = available_symbols_set.into_iter().collect();

        self.create_form = Some(CreateFormState {
            level_id: String::new(),
            symbol_base: String::new(),
            symbol_quote: String::new(),
            side: "Buy".to_string(),
            order_type: "Limit".to_string(),
            enabled: true,
            price: String::new(),
            quantity: String::new(),
            risk_limit: String::new(),
            max_position: String::new(),
            repost_on_fill: true,
            repost_on_cancel: true,
            selected_field: 0,
            input_buffer: String::new(),
            editing: false,
            available_symbols,
        });
        Ok(())
    }

    pub fn close_create_form(&mut self) {
        self.create_form = None;
    }

    pub async fn save_create_form(&mut self) -> Result<(), CliError> {
        if let Some(form) = &self.create_form {
            // Validate required fields
            if form.level_id.is_empty() {
                tracing::warn!("Level ID is required");
                return Ok(()); // Don't create, just return
            }
            if form.symbol_base.is_empty() || form.symbol_quote.is_empty() {
                tracing::warn!("Symbol base and quote are required");
                return Ok(()); // Don't create, just return
            }
            if form.price.is_empty() || form.quantity.is_empty() {
                tracing::warn!("Price and quantity are required");
                return Ok(()); // Don't create, just return
            }

            let symbol = crate::client::trading_cli::Symbol {
                base: form.symbol_base.clone(),
                quote: form.symbol_quote.clone(),
            };

            let config = crate::client::trading_cli::LevelConfig {
                id: form.level_id.clone(),
                symbol: Some(symbol),
                side: form.side.clone(),
                order_type: form.order_type.clone(),
                price: format!("Fixed({})", form.price),
                quantity: format!("Fixed({})", form.quantity),
                enabled: form.enabled,
                risk_limit: form.risk_limit.clone(),
                max_position_size: form.max_position.clone(),
                repost_on_fill: form.repost_on_fill,
                repost_on_cancel: form.repost_on_cancel,
            };

            if let Some(ref mut client) = self.client {
                match tokio::time::timeout(Duration::from_secs(5), client.create_level(config))
                    .await
                {
                    Ok(Ok(level_id)) => {
                        tracing::info!("Created level: {}", level_id);
                        // Refresh levels to get updated state
                        self.refresh_levels().await?;
                        self.close_create_form();
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Failed to create level: {:?}", e);
                    }
                    Err(_) => {
                        tracing::warn!("Timeout creating level");
                    }
                }
            }
        }
        Ok(())
    }

    /// Start streaming monitoring data updates
    async fn start_streaming(
        &mut self,
        tx: tokio::sync::mpsc::Sender<MonitoringData>,
    ) -> Result<(), CliError> {
        if self.streaming_active {
            return Ok(()); // Already streaming
        }

        // Get a mutable reference to the client
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| CliError::Custom("Client not initialized".to_string()))?;

        match client.stream_monitoring_data().await {
            Ok(mut stream) => {
                self.streaming_active = true;

                // Spawn a task to handle the stream
                tokio::spawn(async move {
                    tracing::debug!("[CLI] Stream handler started");
                    loop {
                        match stream.message().await {
                            Ok(Some(update)) => {
                                if let Some(data) = update.data {
                                    tracing::debug!(
                                        "[CLI] Received stream update: {} strategies, {} orders",
                                        data.strategies.len(),
                                        data.orders.len()
                                    );
                                    if tx.send(data).await.is_err() {
                                        // Receiver dropped, stop streaming
                                        tracing::debug!("[CLI] Stream receiver dropped, stopping");
                                        break;
                                    }
                                } else {
                                    tracing::warn!("[CLI] Stream update has no data field");
                                }
                            }
                            Ok(None) => {
                                // Stream ended
                                tracing::debug!("[CLI] Stream ended (None received)");
                                break;
                            }
                            Err(e) => {
                                // Wait a bit before breaking to avoid rapid reconnection attempts
                                error!("[CLI] stream_handler: Stream error: {e:?}");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                break;
                            }
                        }
                    }
                    tracing::debug!("[CLI] Stream handler exiting");
                });
                tracing::debug!("[CLI] Streaming started successfully");
                Ok(())
            }
            Err(e) => {
                error!("[CLI] start_streaming: Error starting streaming: {e:?}");
                // Fall back to polling
                self.streaming_active = false;
                Ok(())
            }
        }
    }
}
