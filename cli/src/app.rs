//! Main application state and event loop
use crate::client::TradingCliClient;
use crate::error::CliError;
use crate::screens::main_menu::MainMenuScreen;
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::time::Duration;

pub struct App {
    client: Option<TradingCliClient>,
    endpoint: String,
    current_screen: Screen,
    should_quit: bool,
    trading_status: bool,
}

enum Screen {
    MainMenu,
}

impl App {
    pub fn new(endpoint: String) -> Result<Self, CliError> {
        Ok(Self {
            client: None, // Will connect when needed
            endpoint,
            current_screen: Screen::MainMenu,
            should_quit: false,
            trading_status: true, // Default to enabled
        })
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), CliError> {
        // Connect to gRPC server
        self.client = Some(TradingCliClient::connect(&self.endpoint).await?);

        // Initial status fetch
        self.refresh_trading_status().await?;

        loop {
            // Draw current screen
            terminal.draw(|f| {
                match self.current_screen {
                    Screen::MainMenu => {
                        if let Err(e) = MainMenuScreen::render(f, f.size(), self) {
                            // Log error but don't crash
                            eprintln!("Render error: {}", e);
                        }
                    }
                }
            })?;

            // Handle events (keyboard input)
            if crossterm::event::poll(Duration::from_millis(250))
                .map_err(|e| CliError::Terminal(format!("{}", e)))?
            {
                if let crossterm::event::Event::Key(key) =
                    crossterm::event::read().map_err(|e| CliError::Terminal(format!("{}", e)))?
                {
                    match self.current_screen {
                        Screen::MainMenu => {
                            if let Err(e) = MainMenuScreen::handle_input_async(self, key.code).await
                            {
                                eprintln!("Input handling error: {}", e);
                            }
                        }
                    }
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
            let cancelled = client.set_trading_enabled(enabled).await?;
            self.trading_status = enabled; // Update cached status
            Ok(cancelled)
        } else {
            Ok(vec![])
        }
    }
}
