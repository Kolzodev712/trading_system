//! Main application state and event loop
use crate::client::trading_cli::MonitoringData;
use crate::client::TradingCliClient;
use crate::error::CliError;
use crate::screens::main_menu::MainMenuScreen;
use crate::screens::trading_monitor::TradingMonitorScreen;
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
    streaming_active: bool,
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
            trading_status: true, // Default to enabled
            monitoring_data: None,
            levels: None,
            streaming_active: false,
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
                tracing::debug!(
                    "[CLI] Event loop: detected TradingMonitor screen, starting streaming..."
                );
                // Refresh levels immediately when entering
                if let Err(_) = self.refresh_levels().await {
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
                                    if TradingMonitorScreen::handle_input_async(self, key_code).await.is_ok() {
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
                    if let Some(data) = update {
                        self.monitoring_data = Some(data);
                        needs_redraw = true; // Data updated, need redraw
                    }
                }

                // Periodic level refresh (only when in Trading Monitor)
                _ = level_refresh_interval.tick(), if matches!(self.current_screen, Screen::TradingMonitor) => {
                    if let Err(_) = self.refresh_levels().await {
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

    pub fn levels(&self) -> Option<&Vec<crate::client::trading_cli::Level>> {
        self.levels.as_ref()
    }

    pub async fn refresh_monitoring_data(&mut self) -> Result<(), CliError> {
        if let Some(ref mut client) = self.client {
            // Use timeout to avoid blocking
            match tokio::time::timeout(Duration::from_secs(2), client.get_monitoring_data()).await {
                Ok(Ok(data)) => {
                    self.monitoring_data = Some(data);
                }
                Ok(Err(_)) => {}
                Err(_) => {}
            }
        }
        Ok(())
    }

    pub async fn go_to_trading_monitor(&mut self) -> Result<(), CliError> {
        self.current_screen = Screen::TradingMonitor;
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
                    self.levels = Some(levels);
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
    }

    /// Start streaming monitoring data updates
    async fn start_streaming(
        &mut self,
        tx: tokio::sync::mpsc::Sender<MonitoringData>,
    ) -> Result<(), CliError> {
        if self.streaming_active {
            tracing::debug!("[CLI] start_streaming: already streaming, skipping");
            return Ok(()); // Already streaming
        }

        tracing::debug!("[CLI] start_streaming: attempting to start streaming...");

        // Get a mutable reference to the client
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| CliError::Custom("Client not initialized".to_string()))?;

        tracing::debug!("[CLI] start_streaming: calling client.stream_monitoring_data()...");
        match client.stream_monitoring_data().await {
            Ok(mut stream) => {
                tracing::info!("[CLI] start_streaming: stream received, spawning handler task");
                self.streaming_active = true;

                // Spawn a task to handle the stream
                tokio::spawn(async move {
                    tracing::debug!("[CLI] stream_handler: started, waiting for messages...");
                    loop {
                        match stream.message().await {
                            Ok(Some(update)) => {
                                let indicator_count = update
                                    .data
                                    .as_ref()
                                    .map(|d| d.indicators.len())
                                    .unwrap_or(0);
                                tracing::debug!(
                                    "[CLI] stream_handler: received update with {} indicators",
                                    indicator_count
                                );
                                if let Some(data) = update.data {
                                    if tx.send(data).await.is_err() {
                                        tracing::debug!(
                                            "[CLI] stream_handler: receiver dropped, stopping"
                                        );
                                        // Receiver dropped, stop streaming
                                        break;
                                    }
                                }
                                // Refresh levels periodically (every 5 seconds)
                                // This is a simple approach - in production, you might want a separate task
                            }
                            Ok(None) => {
                                tracing::info!("[CLI] stream_handler: stream ended (Ok(None))");
                                // Stream ended
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
                    tracing::debug!("[CLI] stream_handler: task ended");
                });

                tracing::info!("[CLI] start_streaming: streaming started successfully");
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
