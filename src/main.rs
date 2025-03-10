use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod scanner;
mod ui;
mod cycle;
mod sort_order;
mod size_filter;
mod age_filter;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Maximum depth to search
    #[arg(short, long, default_value_t = 3)]
    max_depth: usize,

    /// Starting directory
    #[arg(short, long)]
    dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    let start_dir = args.dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    let max_depth = args.max_depth;

    // Set up the terminal
    setup_terminal()?;
    
    // Initialize the TUI
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    
    // Shared state for the scanner and UI
    let found_dirs = Arc::new(Mutex::new(Vec::new()));
    let found_dirs_clone = Arc::clone(&found_dirs);
    
    // Create a flag to indicate when scanning is complete
    let scan_complete = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let scan_complete_clone = Arc::clone(&scan_complete);
    
    // Start the scanner in a separate thread
    std::thread::spawn(move || {
        let result = scanner::scan_directories(&start_dir, max_depth, found_dirs_clone);
        if let Err(e) = result {
            eprintln!("Error scanning directories: {}", e);
        }
        // Mark scan as complete
        scan_complete_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    
    // Run the UI loop
    ui::run_ui(&mut terminal, &found_dirs, &scan_complete)?;

    // Clean up
    restore_terminal()?;
    
    Ok(())
}

fn setup_terminal() -> Result<()> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    std::io::stdout()
        .execute(EnterAlternateScreen)
        .context("Failed to enter alternate screen")?;
    Ok(())
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode().context("Failed to disable raw mode")?;
    std::io::stdout()
        .execute(LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    Ok(())
}
