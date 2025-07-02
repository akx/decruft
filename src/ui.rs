use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::age_filter::AgeFilter;
use crate::cycle::Cycle;
use crate::scanner::CruftDirectory;
use crate::size_filter::SizeFilter;
use crate::sort_order::SortOrder;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;

pub struct AppState {
    pub list_state: ListState,
    pub selected_path: Option<String>,
    pub confirm_delete: Option<String>, // Path of directory to delete, if confirmation is pending
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
            confirm_delete: None,
            size_filter: SizeFilter::SkipSmall,
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
        self.spinner_frame = (self.spinner_frame + 1) % 8;
    }

    pub fn mark_scan_complete(&mut self) {
        self.scan_complete = true;
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

    pub fn select_next_or_previous(&mut self, filtered_dirs: &[CruftDirectory], forward: bool) {
        if filtered_dirs.is_empty() {
            return;
        }

        let current_pos = if let Some(ref selected_path) = self.selected_path {
            filtered_dirs
                .iter()
                .position(|dir| dir.id() == *selected_path)
        } else {
            None
        };

        let new_pos = match current_pos {
            Some(current_pos) => {
                let list_len = (filtered_dirs.len() - 1) as i64;
                ((current_pos as i64) + if forward { 1 } else { -1 })
                    .max(0)
                    .min(list_len) as usize
            }
            None => 0,
        };
        self.list_state.select(Some(new_pos));
        self.selected_path = Some(filtered_dirs[new_pos].id());
    }

    // Update selection position based on filtered directories
    pub fn update_selection(&mut self, filtered_dirs: &[CruftDirectory]) {
        if filtered_dirs.is_empty() {
            self.selected_path = None;
            self.list_state.select(None);
            return;
        }

        if let Some(ref selected_path) = self.selected_path {
            let position = filtered_dirs
                .iter()
                .position(|dir| dir.id() == *selected_path);
            self.list_state.select(position);
        } else {
            self.selected_path = None;
            self.list_state.select(None);
        }
    }
}

/// Filters the directory list based on size, type, and age criteria
fn filter_dirs(dirs: &[CruftDirectory], app_state: &AppState) -> Vec<CruftDirectory> {
    let min_size_bytes = app_state.size_filter.as_bytes();
    let max_age_days = app_state.age_filter.as_days();
    let sort_order = app_state.sort_order;

    let mut filtered = dirs
        .iter()
        .filter(|dir| {
            if dir.size < min_size_bytes {
                return false;
            }
            if let Some(days) = max_age_days {
                if dir.newest_file_age_days.unwrap_or(0.0) < days as f64 {
                    return false;
                }
            }
            true
        })
        .cloned() // Clone the CruftDirectory objects
        .collect::<Vec<_>>();

    sort_order.sort_entries(&mut filtered);

    filtered
}

pub fn run_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    found_dirs: &Arc<Mutex<Vec<CruftDirectory>>>,
    scan_complete: &Arc<std::sync::atomic::AtomicBool>,
    n_scanned_ents: &Arc<AtomicU64>,
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
        let (n_total_dirs, filtered_dirs) = {
            let dirs = found_dirs.lock().unwrap();
            (dirs.len(), filter_dirs(&dirs, &app_state))
        };

        // Update selection based on newly filtered directories
        app_state.update_selection(&filtered_dirs);

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Status bar with border
                    Constraint::Min(10),   // List content
                    Constraint::Length(1), // Help line
                ])
                .split(f.area());

            // Status bar at the top (no title bar)

            let total_size: u64 = filtered_dirs.iter().map(|d| d.size).sum();

            let items: Vec<ListItem> = filtered_dirs
                .iter()
                .map(|dir| {
                    let size_mb = dir.size as f64 / 1_048_576.0;

                    // Format size with fixed width (15 chars)
                    let size_str = format!("{:.2} MB", size_mb);
                    let size_formatted = format!("{:>15} ", size_str);

                    // Format age with fixed width (10 chars)
                    let age_str = format!("{} days", dir.newest_file_age_days.unwrap_or(0.0).round());
                    let age_formatted = format!("{:>10} ", age_str);

                    // Format type with fixed width (15 chars)
                    let type_str = format!("{}", dir.crufty_reason);
                    let type_formatted = format!("{:<15} ", type_str);

                    let line = Line::from(vec![
                        Span::styled(
                            size_formatted,
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(
                            age_formatted,
                            Style::default().fg(Color::Magenta),
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
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_stateful_widget(list, chunks[1], &mut app_state.list_state);

            // Status/help text comes first now (at the top)
            if let Some(ref path_to_delete) = app_state.confirm_delete {
                let confirm_text = format!(
                    "Delete {}? Press y to confirm, n to cancel.",
                    path_to_delete
                );
                let confirm = Paragraph::new(confirm_text)
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::BOTTOM));
                f.render_widget(confirm, chunks[0]);
            } else {
                // Build status text showing current filtering state
                let mut filter_parts = Vec::new();
                filter_parts.push(app_state.size_filter.as_str().to_string());
                if app_state.age_filter != AgeFilter::None {
                    filter_parts.push(app_state.age_filter.as_str().to_string());
                }

                // Show sort order
                filter_parts.push(format!("sort: {}", app_state.sort_order.as_str()));

                let header = if app_state.scan_complete {
                    format!("Decruft: Found {} dirs in {} entities", n_total_dirs, n_scanned_ents.load(Ordering::Relaxed))
                } else {
                    let spinner = SPINNER_CHARS[app_state.spinner_frame];
                    format!("{} Decruft: Scanning {} entities, found {} dirs so far", spinner, n_scanned_ents.load(Ordering::Relaxed), n_total_dirs)
                };

                let status_text = format!(
                    "{} (showing {}, {}). Total: {:.2} MB",
                    header,
                    filtered_dirs.len(),
                    filter_parts.join(", "),
                    total_size as f64 / 1_048_576.0
                );

                let status = Paragraph::new(status_text)
                    .style(Style::default().fg(Color::White))
                    .block(Block::default().borders(Borders::BOTTOM));
                f.render_widget(status, chunks[0]);
            }

            // Always show help line at the bottom
            let help_text = "j/k: Navigate | a: Toggle all types | s: Toggle small files | o: Toggle age filter | r: Toggle sort | d: Delete | D: Delete (no confirm) | q: Quit";
            let help_line = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(help_line, chunks[2]);
        })?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match &app_state.confirm_delete {
                    Some(_) => match key.code {
                        KeyCode::Char('y') => {
                            if let Some(path_str) = app_state.confirm_delete.take() {
                                terminal.draw(|f| {
                                    let confirm = Paragraph::new("Deleting...")
                                        .style(Style::default().fg(Color::Red))
                                        .block(Block::default().borders(Borders::BOTTOM));
                                    f.render_widget(confirm, f.area());
                                })?;
                                do_delete_now(found_dirs, &path_str);
                            }
                        }
                        KeyCode::Char('n') => {
                            app_state.cancel_delete_confirmation();
                        }
                        _ => {}
                    },
                    None => {
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('j') | KeyCode::Down => {
                                app_state.select_next_or_previous(&filtered_dirs, true)
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app_state.select_next_or_previous(&filtered_dirs, false)
                            }
                            KeyCode::Char('s') => app_state.toggle_skip_small(),
                            KeyCode::Char('o') => app_state.toggle_old_dirs(),
                            KeyCode::Char('r') => app_state.toggle_sort_order(),
                            KeyCode::Char('d') => {
                                if let Some(ref selected_path) = app_state.selected_path {
                                    app_state.request_delete_confirmation(selected_path.clone());
                                }
                            }
                            KeyCode::Char('D') => {
                                // Immediately delete without confirmation (Shift+D)
                                if let Some(ref selected_path) = app_state.selected_path {
                                    do_delete_now(found_dirs, selected_path);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn do_delete_now(found_dirs: &Arc<Mutex<Vec<CruftDirectory>>>, selected_path: &String) {
    let mut dirs = found_dirs.lock().unwrap();
    if let Some(cd) = dirs.iter().find(|dir| dir.id() == *selected_path) {
        let path = cd.path.clone();
        std::fs::remove_dir_all(&path).unwrap();
        dirs.retain(|dir| dir.path != path);
    }
}
