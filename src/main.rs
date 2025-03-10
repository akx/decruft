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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Maximum depth to search
    #[arg(short, long, default_value_t = 3)]
    max_depth: usize,

    /// Starting directory
    #[arg(short, long)]
    dir: Option<PathBuf>,
    
    /// Show only directories larger than this size in MB
    #[arg(short = 's', long, default_value_t = 0.0)]
    min_size: f64,
    
    /// Skip small cruft (less than 1 MB)
    #[arg(short = 'S', long)]
    skip_small: bool,
    
    /// Show all cruft types (by default, only shows the most common types)
    #[arg(short, long)]
    all: bool,
    
    /// Show only old directories (not modified in 90 days)
    #[arg(short = 'o', long)]
    old: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    let start_dir = args.dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    let max_depth = args.max_depth;
    
    // Convert min_size from MB to bytes
    let min_size_bytes = (args.min_size * 1_048_576.0) as u64;
    
    // If skip_small is set, ensure min_size is at least 1 MB
    let min_size_bytes = if args.skip_small && min_size_bytes < 1_048_576 {
        1_048_576 // 1 MB in bytes
    } else {
        min_size_bytes
    };
    
    let show_all = args.all;
    let show_old = args.old;
    
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
    ui::run_ui(&mut terminal, &found_dirs, &scan_complete, min_size_bytes, show_all, show_old)?;
    
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
