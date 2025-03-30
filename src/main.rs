use anyhow::{Context, Result};
use clap::Parser;
use crossterm::ExecutableCommand;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

mod age_filter;
mod cycle;
mod scanner;
mod size_filter;
mod sort_order;
mod ui;

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

    let scanned_ents = Arc::new(AtomicU64::new(0));
    let scanned_ents_clone = Arc::clone(&scanned_ents);

    // Shared state for the scanner and UI
    let found_dirs = Arc::new(Mutex::new(Vec::new()));
    let found_dirs_clone = Arc::clone(&found_dirs);

    // Create a flag to indicate when scanning is complete
    let scan_complete = Arc::new(AtomicBool::new(false));
    let scan_complete_clone = Arc::clone(&scan_complete);

    // Start the scanner in a separate thread
    std::thread::spawn(move || {
        let result =
            scanner::scan_directories(&start_dir, max_depth, found_dirs_clone, scanned_ents_clone);
        if let Err(e) = result {
            eprintln!("Error scanning directories: {}", e);
        }
        // Mark scan as complete
        scan_complete_clone.store(true, Ordering::Relaxed);
    });

    // Run the UI loop
    ui::run_ui(&mut terminal, &found_dirs, &scan_complete, &scanned_ents)?;

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
