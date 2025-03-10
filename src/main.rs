use anyhow::{Context, Result};
use clap::Parser;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
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

    /// Just scan directories, do not show TUI
    #[arg(long)]
    scan_only: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let start_dir = args.dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    let max_depth = args.max_depth;

    if args.scan_only {
        // If scan_only is true, just run the scanner and exit
        let scanned_ents = Arc::new(AtomicU64::new(0));
        let found_dirs = Arc::new(Mutex::new(Vec::new()));
        scanner::scan_directories(
            &start_dir,
            max_depth,
            found_dirs.clone(),
            scanned_ents,
            Some(Box::new(|progress| {
                eprintln!("Scanned: {}, Found: {}", progress.scanned, progress.found);
            })),
        )?;
        for dir in found_dirs.lock().unwrap().iter() {
            println!(
                "Found directory: {} (size: {} bytes)",
                dir.path.display(),
                dir.size
            );
        }
        return Ok(());
    }
    run_with_tui(start_dir, max_depth)
}

fn run_with_tui(start_dir: PathBuf, max_depth: usize) -> Result<()> {
    setup_terminal()?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let scanned_ents = Arc::new(AtomicU64::new(0));
    let scanned_ents_clone = Arc::clone(&scanned_ents);

    let found_dirs = Arc::new(Mutex::new(Vec::new()));
    let found_dirs_clone = Arc::clone(&found_dirs);

    let scan_complete = Arc::new(AtomicBool::new(false));
    let scan_complete_clone = Arc::clone(&scan_complete);

    std::thread::spawn(move || {
        let result = scanner::scan_directories(
            &start_dir,
            max_depth,
            found_dirs_clone,
            scanned_ents_clone,
            None,
        );
        if let Err(e) = result {
            eprintln!("Error scanning directories: {}", e);
        }
        scan_complete_clone.store(true, Ordering::Relaxed);
    });

    ui::run_ui(&mut terminal, &found_dirs, &scan_complete, &scanned_ents)?;

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
