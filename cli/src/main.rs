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
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Initialize logging to file
    // Set default log level to debug if RUST_LOG is not set
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("cli_debug.log")
    {
        Ok(log_file) => {
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_writer(log_file)
                .init();
        }
        Err(e) => {
            // Fallback to stderr if file can't be opened
            eprintln!(
                "Warning: Could not open cli_debug.log: {}. Logging to stderr instead.",
                e
            );
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
    }
    // Set up panic hook to restore terminal on panic
    std::panic::set_hook(Box::new(|_| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
    }));

    // Early help handling (avoid terminal setup when just showing help)
    let args_vec: Vec<String> = std::env::args().collect();
    if args_vec.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    // Parse command line arguments
    let args = parse_args(&args_vec);

    // Setup terminal with proper error handling
    let mut terminal = match (|| -> Result<ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>, CliError> {
        enable_raw_mode().map_err(|e| CliError::Terminal(format!("enable_raw_mode failed: {}", e)))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| CliError::Terminal(format!("execute failed: {}", e)))?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let term = ratatui::Terminal::new(backend)
            .map_err(|e| CliError::Terminal(format!("Terminal::new failed: {}", e)))?;
        Ok(term)
    })() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to setup terminal: {}", e);
            eprintln!("Make sure you're running this in a terminal (not redirected output)");
            std::process::exit(1);
        }
    };

    // Create and run app with proper cleanup
    let result = match App::new(args.endpoint.clone()) {
        Ok(mut app) => app.run(&mut terminal).await,
        Err(e) => {
            eprintln!("Failed to create app: {}", e);
            eprintln!(
                "Make sure the trading system is running and accessible at: {}",
                args.endpoint
            );
            Err(e)
        }
    };

    // Always restore terminal, even on error
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    match result {
        Ok(()) => std::process::exit(0),
        Err(_) => std::process::exit(1),
    }
}

/// Parse command line arguments
struct Args {
    endpoint: String,
}

fn parse_args(args: &[String]) -> Args {
    // Simple argument parsing for now
    // Skip the first arg (program name), get the second (endpoint) or use default
    let endpoint = args
        .get(1)
        .map(|s| s.clone())
        .unwrap_or_else(|| "http://127.0.0.1:50051".to_string());

    Args { endpoint }
}

fn print_help() {
    println!("Trading System CLI (TUI)");
    println!("Usage: trading-cli [endpoint]");
    println!("Defaults to http://127.0.0.1:50051");
    println!("Options:");
    println!("  -h, --help    Show this help and exit");
}
