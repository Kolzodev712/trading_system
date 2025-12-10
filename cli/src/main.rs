//! Trading System CLI - Terminal User Interface
mod app;
mod client;
mod error;
mod screens;

use app::App;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use error::CliError;
use std::io;

#[tokio::main]
async fn main() -> Result<(), CliError> {
    // Parse command line arguments
    let args = parse_args();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Create terminal
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Create and run app
    let mut app = App::new(args.endpoint)?;
    let result = app.run(&mut terminal).await;

    // Restore terminal
    disable_raw_mode().map_err(|e| CliError::Terminal(format!("{}", e)))?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(|e| CliError::Terminal(format!("{}", e)))?;
    terminal
        .show_cursor()
        .map_err(|e| CliError::Terminal(format!("{}", e)))?;

    result
}

/// Parse command line arguments
struct Args {
    endpoint: String,
}

fn parse_args() -> Args {
    // Simple argument parsing for now
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://[::1]:50051".to_string());

    Args { endpoint }
}
