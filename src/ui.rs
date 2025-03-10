use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;

use crate::scanner::{CruftDirectory, is_common_cruft};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    SizeDescending,
    AgeDescending,
    Alphabetical,
}

impl SortOrder {
    // The ordered rotation of sort modes
    const ALL_ORDERS: [SortOrder; 3] = [
        SortOrder::SizeDescending, 
        SortOrder::AgeDescending, 
        SortOrder::Alphabetical
    ];
    
    pub fn next(&self) -> Self {
        let current_idx = Self::ALL_ORDERS.iter()
            .position(|&order| order == *self)
            .unwrap_or(0);
        
        // Get the next index, wrapping around to 0 if needed
        let next_idx = (current_idx + 1) % Self::ALL_ORDERS.len();
        Self::ALL_ORDERS[next_idx]
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            SortOrder::SizeDescending => "size",
            SortOrder::AgeDescending => "age",
            SortOrder::Alphabetical => "name",
        }
    }
}

pub struct AppState {
    pub list_state: ListState,
    pub selected_index: Option<usize>,
    pub show_help: bool,
    pub confirm_delete: Option<String>, // Path of directory to delete, if confirmation is pending
    pub min_size_bytes: u64,
    pub show_all_types: bool,
    pub max_age_days: Option<u64>, // None means no age filter, Some(days) means show only dirs older than days
    pub sort_order: SortOrder,
    pub scan_complete: bool,
    pub spinner_frame: usize, // For animation
}

impl AppState {
    pub fn new(min_size_bytes: u64, show_all_types: bool, show_old_dirs: bool) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            list_state,
            selected_index: Some(0),
            show_help: false,
            confirm_delete: None,
            min_size_bytes,
            show_all_types,
            max_age_days: if show_old_dirs { Some(90) } else { None }, // Default to 90 days if old filter enabled
            sort_order: SortOrder::SizeDescending, // Default sort by size
            scan_complete: false,
            spinner_frame: 0,
        }
    }
    
    pub fn toggle_sort_order(&mut self) {
        self.sort_order = self.sort_order.next();
    }
    
    pub fn update_spinner(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % 8; // 8 frames in our spinner
    }
    
    pub fn mark_scan_complete(&mut self) {
        self.scan_complete = true;
    }
    
    pub fn toggle_show_all(&mut self) {
        self.show_all_types = !self.show_all_types;
    }
    
    pub fn toggle_skip_small(&mut self) {
        // Toggle between 0 (show all) and 1 MB (skip small)
        if self.min_size_bytes < 1_048_576 {
            self.min_size_bytes = 1_048_576; // 1 MB in bytes
        } else {
            self.min_size_bytes = 0;
        }
    }
    
    pub fn toggle_old_dirs(&mut self) {
        // Cycle through age thresholds
        static AGE_OPTIONS: [Option<u64>; 4] = [None, Some(90), Some(180), Some(365)];
        
        let current_idx = AGE_OPTIONS.iter()
            .position(|&opt| opt == self.max_age_days)
            .unwrap_or(0);
            
        // Get the next index, wrapping around to 0 if needed
        let next_idx = (current_idx + 1) % AGE_OPTIONS.len();
        self.max_age_days = AGE_OPTIONS[next_idx];
    }
    
    pub fn request_delete_confirmation(&mut self, path: String) {
        self.confirm_delete = Some(path);
    }
    
    pub fn cancel_delete_confirmation(&mut self) {
        self.confirm_delete = None;
    }
    
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn next(&mut self, items_len: usize) {
        if items_len == 0 {
            return;
        }
        let i = match self.selected_index {
            Some(i) => {
                if i >= items_len - 1 {
                    // Don't wrap around - stay at the last item
                    items_len - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.selected_index = Some(i);
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self, items_len: usize) {
        if items_len == 0 {
            return;
        }
        let i = match self.selected_index {
            Some(i) => {
                if i == 0 {
                    // Don't wrap around - stay at the first item
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.selected_index = Some(i);
        self.list_state.select(Some(i));
    }
}

/// Filters the directory list based on size, type, and age criteria
fn filter_dirs<'a>(
    dirs: &'a [CruftDirectory],
    min_size_bytes: u64,
    show_all_types: bool,
    max_age_days: Option<u64>,
    sort_order: SortOrder,
) -> Vec<&'a CruftDirectory> {
    let mut filtered = dirs
        .iter()
        .filter(|dir| {
            // Size filter
            let size_ok = dir.size >= min_size_bytes;
            
            // Type filter
            let type_ok = show_all_types || is_common_cruft(&dir.crufty_reason);
            
            // Age filter - only apply if max_age_days is Some
            let age_ok = if let Some(days) = max_age_days {
                dir.newest_file_age_days >= days // Show directories that are older than the threshold
            } else {
                true // If no age filter, accept all
            };
            
            size_ok && type_ok && age_ok
        })
        .collect::<Vec<_>>();
    
    // Sort according to the selected sort order
    match sort_order {
        SortOrder::SizeDescending => {
            // Sort by size, descending
            filtered.sort_by(|a, b| b.size.cmp(&a.size));
        },
        SortOrder::AgeDescending => {
            // Sort by age, descending (oldest first)
            filtered.sort_by(|a, b| b.newest_file_age_days.cmp(&a.newest_file_age_days));
        },
        SortOrder::Alphabetical => {
            // Sort alphabetically by path
            filtered.sort_by(|a, b| a.path.to_string_lossy().cmp(&b.path.to_string_lossy()));
        },
    }
    
    filtered
}

// No longer needed - we now use paths as identifiers

pub fn run_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    found_dirs: &Arc<Mutex<Vec<CruftDirectory>>>,
    scan_complete: &Arc<std::sync::atomic::AtomicBool>,
    min_size_bytes: u64,
    show_all_types: bool,
    show_old_dirs: bool,
) -> Result<()> {
    let mut app_state = AppState::new(min_size_bytes, show_all_types, show_old_dirs);
    
    // Spinner characters for the animation
    const SPINNER_CHARS: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    
    loop {
        // Check if scanning is complete
        let is_scan_complete = scan_complete.load(std::sync::atomic::Ordering::Relaxed);
        if is_scan_complete && !app_state.scan_complete {
            app_state.mark_scan_complete();
        }
        
        // Update spinner animation if still scanning
        if !app_state.scan_complete {
            app_state.update_spinner();
        }
        
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Status bar with border
                    Constraint::Min(10),   // List content
                    Constraint::Length(1), // Help line
                ])
                .split(f.size());
            
            // Status bar at the top (no title bar)
            
            // Directory list
            let dirs = found_dirs.lock().unwrap();
            
            // Filter and sort directories
            let filtered_dirs = filter_dirs(&dirs, app_state.min_size_bytes, app_state.show_all_types, app_state.max_age_days, app_state.sort_order);
                
            let total_size: u64 = filtered_dirs.iter().map(|d| d.size).sum();
            
            // Ensure the selected index is valid
            if let Some(selected) = app_state.selected_index {
                if selected >= filtered_dirs.len() {
                    if filtered_dirs.is_empty() {
                        app_state.selected_index = None;
                        app_state.list_state.select(None);
                    } else {
                        app_state.selected_index = Some(filtered_dirs.len() - 1);
                        app_state.list_state.select(Some(filtered_dirs.len() - 1));
                    }
                }
            }
            
            let items: Vec<ListItem> = filtered_dirs
                .iter()
                .map(|&dir| {
                    let size_mb = dir.size as f64 / 1_048_576.0;
                    
                    // Format size with fixed width (15 chars)
                    let size_str = format!("{:.2} MB", size_mb);
                    let size_formatted = format!("{:>15} ", size_str); // Added a space at the end
                    
                    // Format age with fixed width (10 chars)
                    let age_str = format!("{} days", dir.newest_file_age_days);
                    let age_formatted = format!("{:>10} ", age_str); // Added a space at the end
                    
                    // Format type with fixed width (15 chars)
                    let type_str = format!("{}", dir.crufty_reason);
                    let type_formatted = format!("{:<15} ", type_str); // Added a space at the end
                    
                    let line = Line::from(vec![
                        Span::styled(
                            size_formatted,
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(
                            age_formatted,
                            Style::default().fg(Color::Magenta), // Use a different color for age
                        ),
                        Span::styled(
                            type_formatted,
                            Style::default().fg(Color::Green),
                        ),
                        Span::raw(dir.path.to_string_lossy().to_string()),
                    ]);
                    ListItem::new(line)
                })
                .collect();
            
            let list = List::new(items)
                // No borders around the list
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
            
            f.render_stateful_widget(list, chunks[1], &mut app_state.list_state);
            
            // Status/help text comes first now (at the top)
            if let Some(ref path_to_delete) = app_state.confirm_delete {
                // We already have the path directly, no need to refilter
                let confirm_text = format!(
                    "Delete {}? Press y to confirm, n to cancel.",
                    path_to_delete
                );
                let confirm = Paragraph::new(confirm_text)
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::BOTTOM));
                f.render_widget(confirm, chunks[0]);
            } else if app_state.show_help {
                let help_text = vec![
                    "j/Down: Move selection down",
                    "k/Up: Move selection up",
                    "a: Toggle between showing all cruft types or just common ones",
                    "s: Toggle display of small entries (less than 1 MB)",
                    "o: Toggle age filter (none -> 90 days -> 180 days -> 365 days)",
                    "r: Toggle sort order (size -> age -> name)",
                    "d: Request deletion of selected directory (with confirmation)",
                    "D: Delete selected directory immediately (Shift+D, no confirmation)",
                    "h: Toggle help screen",
                    "q: Quit application",
                ]
                .join(" | ");
                
                let help = Paragraph::new(help_text)
                    .block(Block::default().borders(Borders::BOTTOM));
                f.render_widget(help, chunks[0]);
            } else {
                // Build status text showing current filtering state
                let mut filter_parts = Vec::new();
                if app_state.show_all_types {
                    filter_parts.push("all types".to_string());
                } else {
                    filter_parts.push("common types".to_string());
                }
                
                if app_state.min_size_bytes >= 1_048_576 {
                    filter_parts.push("size ≥ 1 MB".to_string());
                }
                
                // Show age filter if active
                if let Some(days) = app_state.max_age_days {
                    filter_parts.push(format!("age ≥ {} days", days));
                }
                
                // Show sort order
                filter_parts.push(format!("sort: {}", app_state.sort_order.as_str()));
                
                let status_text = if app_state.scan_complete {
                    format!(
                        "Decruft: Found {} dirs ({}). Total: {:.2} MB",
                        filtered_dirs.len(),
                        filter_parts.join(", "),
                        total_size as f64 / 1_048_576.0
                    )
                } else {
                    let spinner = SPINNER_CHARS[app_state.spinner_frame];
                    format!(
                        "{} Scanning... Found {} directories so far",
                        spinner,
                        filtered_dirs.len()
                    )
                };
                
                let status = Paragraph::new(status_text)
                    .style(Style::default().fg(Color::White))
                    .block(Block::default().borders(Borders::BOTTOM));
                f.render_widget(status, chunks[0]);
            }
            
            // Always show help line at the bottom
            let help_text = "j/k: Navigate | a: Toggle all types | s: Toggle small files | o: Toggle age filter | r: Toggle sort | d: Delete | D: Delete (no confirm) | h: Help | q: Quit";
            let help_line = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(help_line, chunks[2]);
        })?;
        
        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let dirs = found_dirs.lock().unwrap();
                        app_state.next(dirs.len());
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let dirs = found_dirs.lock().unwrap();
                        app_state.previous(dirs.len());
                    }
                    KeyCode::Char('h') => {
                        app_state.toggle_help();
                    }
                    KeyCode::Char('a') => {
                        // Toggle showing all cruft types
                        app_state.toggle_show_all();
                    }
                    KeyCode::Char('s') => {
                        // Toggle showing small entries
                        app_state.toggle_skip_small();
                    }
                    KeyCode::Char('o') => {
                        // Toggle showing old directories 
                        app_state.toggle_old_dirs();
                    }
                    KeyCode::Char('r') => {
                        // Toggle sort order
                        app_state.toggle_sort_order();
                    }
                    KeyCode::Char('d') => {
                        // Request confirmation before deleting
                        if let Some(selected) = app_state.selected_index {
                            let dirs = found_dirs.lock().unwrap();
                            
                            // Filter and sort directories
                            let filtered_dirs = filter_dirs(&dirs, app_state.min_size_bytes, app_state.show_all_types, app_state.max_age_days, app_state.sort_order);
                                
                            if selected < filtered_dirs.len() {
                                // Request confirmation using path as the identifier
                                let path = filtered_dirs[selected].path.to_string_lossy().to_string();
                                app_state.request_delete_confirmation(path);
                            }
                        }
                    }
                    KeyCode::Char('D') => {
                        // Immediately delete without confirmation (Shift+D)
                        if let Some(selected) = app_state.selected_index {
                            {
                                let mut dirs = found_dirs.lock().unwrap();
                                
                                // Filter and sort directories
                                let filtered_dirs = filter_dirs(&dirs, app_state.min_size_bytes, app_state.show_all_types, app_state.max_age_days, app_state.sort_order);
                                
                                if selected < filtered_dirs.len() {
                                    // Get the path to delete
                                    let path_to_delete = filtered_dirs[selected].path.clone();
                                    
                                    // Actually delete the directory
                                    match std::fs::remove_dir_all(&path_to_delete) {
                                        Ok(_) => {
                                            // Remove from our list
                                            dirs.retain(|dir| dir.path != path_to_delete);
                                            
                                            // Keep filtered_dirs for selection update
                                            let dirs_remaining = filter_dirs(&dirs, app_state.min_size_bytes, app_state.show_all_types, app_state.max_age_days, app_state.sort_order);
                                            
                                            // Update selection index
                                            if dirs_remaining.is_empty() {
                                                app_state.selected_index = None;
                                                app_state.list_state.select(None);
                                            } else if selected >= dirs_remaining.len() {
                                                app_state.selected_index = Some(dirs_remaining.len() - 1);
                                                app_state.list_state.select(Some(dirs_remaining.len() - 1));
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Error deleting directory {}: {}", path_to_delete.display(), e);
                                        }
                                    }
                                }
                            };
                        }
                    }
                    KeyCode::Char('y') => {
                        // Confirm deletion
                        if let Some(path_str) = app_state.confirm_delete.take() {
                            let path = std::path::PathBuf::from(&path_str);
                            let selected_index = app_state.selected_index;
                            
                            // Delete from our internal list and the filesystem
                            {
                                let mut dirs = found_dirs.lock().unwrap();
                                
                                // Remove from our list - use path string comparison
                                dirs.retain(|dir| dir.path.to_string_lossy() != path_str);
                                
                                // After deletion, refilter the list for selection updates
                                let new_filtered_dirs = filter_dirs(&dirs, app_state.min_size_bytes, app_state.show_all_types, app_state.max_age_days, app_state.sort_order);
                                
                                // Update selection index
                                if new_filtered_dirs.is_empty() {
                                    app_state.selected_index = None;
                                    app_state.list_state.select(None);
                                } else if let Some(idx) = selected_index {
                                    if idx >= new_filtered_dirs.len() {
                                        app_state.selected_index = Some(new_filtered_dirs.len() - 1);
                                        app_state.list_state.select(Some(new_filtered_dirs.len() - 1));
                                    }
                                }
                            }
                            
                            // Actually delete the directory from the filesystem
                            match std::fs::remove_dir_all(&path) {
                                Ok(_) => {}
                                Err(e) => {
                                    eprintln!("Error deleting directory {}: {}", path.display(), e);
                                }
                            }
                        }
                    }
                    KeyCode::Char('n') => {
                        // Cancel deletion
                        app_state.cancel_delete_confirmation();
                    }
                    _ => {}
                }
            }
        }
    }
    
    Ok(())
}