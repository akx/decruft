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
use crate::age_filter::AgeFilter;
use crate::cycle::Cycle;
use crate::scanner::{is_common_cruft, CruftDirectory};
use crate::size_filter::SizeFilter;
use crate::sort_order::SortOrder;

pub struct AppState {
    pub list_state: ListState,
    pub selected_path: Option<String>,
    pub show_help: bool,
    pub confirm_delete: Option<String>, // Path of directory to delete, if confirmation is pending
    pub show_all_types: bool,
    pub age_filter: AgeFilter,
    pub sort_order: SortOrder,
    pub size_filter: SizeFilter,
    pub scan_complete: bool,
    pub spinner_frame: usize, // For animation
}

impl AppState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            selected_path: None,
            show_help: false,
            confirm_delete: None,
            size_filter: SizeFilter::SkipSmall,
            show_all_types: false,
            age_filter: AgeFilter::None,
            sort_order: SortOrder::SizeDescending,
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
        self.size_filter = self.size_filter.next();
    }

    pub fn toggle_old_dirs(&mut self) {
        self.age_filter = self.age_filter.next();
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

    pub fn select_next_or_previous(&mut self, filtered_dirs: &[CruftDirectory], forward: bool) {
        if filtered_dirs.is_empty() {
            return;
        }
        
        let current_pos = if let Some(ref selected_path) = self.selected_path {
            filtered_dirs.iter().position(|dir| dir.id() == *selected_path)
        } else {
            None
        };

        match current_pos {
            Some(current_pos) => {
                let list_len = (filtered_dirs.len() - 1) as i64;
                let new_pos = ((current_pos as i64) + if forward { 1 } else { -1 }).max(0).min(list_len) as usize;
                self.list_state.select(Some(new_pos));
                self.selected_path = Some(filtered_dirs[new_pos].id());
            }
            None => {
                // Selection lost...
                self.selected_path = None;
                self.list_state.select(None);
            }
        }
    }

    // Update selection position based on filtered directories
    pub fn update_selection(&mut self, filtered_dirs: &[CruftDirectory]) {
        if filtered_dirs.is_empty() {
            self.selected_path = None;
            self.list_state.select(None);
            return;
        }
        
        if let Some(ref selected_path) = self.selected_path {
            let position = filtered_dirs.iter()
                .position(|dir| dir.id() == *selected_path);
                
            if let Some(idx) = position {
                // Selected path exists in filtered list, update the visual selection
                self.list_state.select(Some(idx));
            } else {
                // Selected path not found, select the first item
                let first_path = filtered_dirs[0].id();
                self.selected_path = Some(first_path);
                self.list_state.select(Some(0));
            }
        } else {
            // No current selection, select the first item
            let first_path = filtered_dirs[0].id();
            self.selected_path = Some(first_path);
            self.list_state.select(Some(0));
        }
    }
}

/// Filters the directory list based on size, type, and age criteria
fn filter_dirs(
    dirs: &[CruftDirectory],
    app_state: &AppState,
) -> Vec<CruftDirectory> {
    let min_size_bytes = app_state.size_filter.as_bytes();
    let show_all_types = app_state.show_all_types;
    let max_age_days = app_state.age_filter.as_days();
    let sort_order = app_state.sort_order;

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
        .cloned() // Clone the CruftDirectory objects
        .collect::<Vec<_>>();

    sort_order.sort_entries(&mut filtered);

    filtered
}

// No longer needed - we now use paths as identifiers

pub fn run_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    found_dirs: &Arc<Mutex<Vec<CruftDirectory>>>,
    scan_complete: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let mut app_state = AppState::new();

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
        
        // Refresh the filtered directories
        let filtered_dirs_cache = {
            let dirs = found_dirs.lock().unwrap();
            filter_dirs(&dirs, &app_state)
        };
        
        // Update selection based on newly filtered directories
        app_state.update_selection(&filtered_dirs_cache);
        
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
            
            let total_size: u64 = filtered_dirs_cache.iter().map(|d| d.size).sum();
            
            let items: Vec<ListItem> = filtered_dirs_cache
                .iter()
                .map(|dir| {
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
                let help_text = ["j/Down: Move selection down",
                    "k/Up: Move selection up",
                    "a: Toggle between showing all cruft types or just common ones",
                    "s: Toggle display of small entries (less than 1 MB)",
                    "o: Toggle age filter (none -> 90 days -> 180 days -> 365 days)",
                    "r: Toggle sort order (size -> age -> name)",
                    "d: Request deletion of selected directory (with confirmation)",
                    "D: Delete selected directory immediately (Shift+D, no confirmation)",
                    "h: Toggle help screen",
                    "q: Quit application"]
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
                filter_parts.push(app_state.size_filter.as_str().to_string());
                if app_state.age_filter != AgeFilter::None {
                    filter_parts.push(app_state.age_filter.as_str().to_string());
                }

                // Show sort order
                filter_parts.push(format!("sort: {}", app_state.sort_order.as_str()));
                
                let status_text = if app_state.scan_complete {
                    format!(
                        "Decruft: Found {} dirs ({}). Total: {:.2} MB",
                        filtered_dirs_cache.len(),
                        filter_parts.join(", "),
                        total_size as f64 / 1_048_576.0
                    )
                } else {
                    let spinner = SPINNER_CHARS[app_state.spinner_frame];
                    format!(
                        "{} Scanning... Found {} directories so far",
                        spinner,
                        filtered_dirs_cache.len()
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
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                        // Use the cached filtered list for navigation
                        match key.code {
                            KeyCode::Down | KeyCode::Char('j') => app_state.select_next_or_previous(&filtered_dirs_cache, true),
                            KeyCode::Up | KeyCode::Char('k') => app_state.select_next_or_previous(&filtered_dirs_cache, false),
                            _ => unreachable!(),
                        }
                    }
                    KeyCode::Char('h') => {
                        app_state.toggle_help();
                    }
                    KeyCode::Char('a') | KeyCode::Char('s') | KeyCode::Char('o') | KeyCode::Char('r') => {
                        // Handle filter/sort changes
                        match key.code {
                            KeyCode::Char('a') => app_state.toggle_show_all(),
                            KeyCode::Char('s') => app_state.toggle_skip_small(),
                            KeyCode::Char('o') => app_state.toggle_old_dirs(),
                            KeyCode::Char('r') => app_state.toggle_sort_order(),
                            _ => unreachable!(),
                        }
                        
                        // Mark cache as dirty - will be recalculated at the next render
                        // We don't need to do anything here since the main loop updates the cache
                    }
                    KeyCode::Char('d') => {
                        // Request confirmation before deleting
                        if let Some(ref selected_path) = app_state.selected_path {
                            // We already have the path, so we can directly request confirmation
                            app_state.request_delete_confirmation(selected_path.clone());
                        }
                    }
                    KeyCode::Char('D') | KeyCode::Char('y') => {
                        // Handle deletion (with or without confirmation)
                        match key.code {
                            KeyCode::Char('D') => {
                                // Immediately delete without confirmation (Shift+D)
                                if let Some(ref selected_path) = app_state.selected_path {
                                    let mut dirs = found_dirs.lock().unwrap();
                                    
                                    // Find the directory with the selected path
                                    if let Some(pos) = dirs.iter().position(|dir| dir.id() == *selected_path) {
                                        // Get the path to delete
                                        let path_to_delete = dirs[pos].path.clone();
                                        
                                        // Actually delete the directory
                                        match std::fs::remove_dir_all(&path_to_delete) {
                                            Ok(_) => {
                                                // Remove from our list
                                                dirs.retain(|dir| dir.path != path_to_delete);
                                                
                                                // Cache will be refreshed on the next render
                                            }
                                            Err(e) => {
                                                eprintln!("Error deleting directory {}: {}", path_to_delete.display(), e);
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('y') => {
                                // Confirm deletion
                                if let Some(path_str) = app_state.confirm_delete.take() {
                                    let path = std::path::PathBuf::from(&path_str);
                                    
                                    // Delete from our internal list
                                    {
                                        let mut dirs = found_dirs.lock().unwrap();
                                        
                                        // Remove from our list - use path string comparison
                                        dirs.retain(|dir| dir.path.to_string_lossy() != path_str);
                                        
                                        // Cache will be refreshed on the next render
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
                            _ => unreachable!(),
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