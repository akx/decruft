use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use anyhow::Result;
use walkdir::WalkDir;

#[derive(Clone)]
pub struct CruftDirectory {
    pub path: PathBuf,
    pub size: u64,
    pub crufty_reason: CruftyReason,
    pub newest_file_age_days: u64,
}

impl CruftDirectory {
    pub fn id(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CruftyReason {
    NodeModules,
    CacheDir,
    CacheTagFound,
    BuildDir,
    TempDir,
    VenvDir,
    DistDir,
    ToxDir,
}

impl std::fmt::Display for CruftyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CruftyReason::NodeModules => write!(f, "node_modules"),
            CruftyReason::CacheDir => write!(f, "cache dir"),
            CruftyReason::CacheTagFound => write!(f, "CACHEDIR.TAG"),
            CruftyReason::BuildDir => write!(f, "build dir"),
            CruftyReason::TempDir => write!(f, "temp dir"),
            CruftyReason::VenvDir => write!(f, "venv"),
            CruftyReason::DistDir => write!(f, "dist dir"),
            CruftyReason::ToxDir => write!(f, "tox dir"),
        }
    }
}

/// Returns true if this type of cruft should be shown by default in normal mode
/// (when --all is not specified)
pub fn is_common_cruft(reason: &CruftyReason) -> bool {
    matches!(
        reason,
        CruftyReason::NodeModules | 
        CruftyReason::CacheDir | 
        CruftyReason::CacheTagFound |
        CruftyReason::BuildDir |
        CruftyReason::VenvDir |
        CruftyReason::ToxDir
    )
}

pub fn scan_directories(
    start_dir: &Path,
    max_depth: usize,
    found_dirs: Arc<Mutex<Vec<CruftDirectory>>>,
) -> Result<()> {
    let walker = WalkDir::new(start_dir)
        .max_depth(max_depth)
        .into_iter()
        .filter_entry(|e| {
            if !e.file_type().is_dir() {
                return true; // Always process files
            }
            
            let path = e.path();
            
            // Skip this directory and its children if it's cruft
            if let Some(reason) = check_crufty(path) {
                // We found cruft, so add it to our list before skipping recursion
                let size = calculate_dir_size(path).unwrap_or(0);
                
                // Calculate the age of the newest file in the directory
                let newest_file_age_days = get_newest_file_age_days(path).unwrap_or(0);
                
                let cruft_dir = CruftDirectory {
                    path: path.to_path_buf(),
                    size,
                    crufty_reason: reason,
                    newest_file_age_days,
                };
                
                // Add to the shared vector
                if let Ok(mut dirs) = found_dirs.lock() {
                    dirs.push(cruft_dir);
                }
                
                false // Don't recurse into this directory
            } else {
                true // Not cruft, so continue recursion
            }
        });

    for _ in walker.filter_map(Result::ok).filter(|e| e.file_type().is_dir()) {
        // Do nothing - the work is done in filter_entry
    }
    
    Ok(())
}

const PROTECTED_DIRS: &[&str] = &[
    ".git",         // Git configuration
    ".github",       // GitHub configuration
    ".vscode",       // VS Code configuration
    ".idea",         // IntelliJ configuration
    "node_modules/.bin", // Executable scripts in node_modules
];

/// Checks if a directory is protected and should not be considered as cruft
fn is_protected_directory(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    for protected in PROTECTED_DIRS {
        if path_str.contains(&format!("/{}/", protected)) {
            return true;
        }
    }

    if let Some(last_component) = path_str.rsplit('/').next() {
        if PROTECTED_DIRS.contains(&last_component) {
            return true;
        }
    }

    false
}

fn check_crufty(path: &Path) -> Option<CruftyReason> {
    let path_str = path.to_string_lossy();
    
    // Skip protected directories
    if is_protected_directory(path) {
        return None;
    }
    
    // Get the filename as lowercase for comparisons
    let file_name = match path.file_name() {
        Some(name) => name.to_string_lossy().to_lowercase(),
        None => return None, // No filename, so it's not crufty
    };
    
    // Check for node_modules
    if file_name == "node_modules" {
        return Some(CruftyReason::NodeModules);
    }
    
    // Check for cache directories
    if path_str.contains(".cache") || file_name.contains("cache") {
        return Some(CruftyReason::CacheDir);
    }
    
    // Check for build directories
    if file_name == "build" || file_name == "target" || file_name.contains("build") {
        return Some(CruftyReason::BuildDir);
    }
    
    // Check for temp directories - avoid matching "templates"
    if file_name == "tmp" || file_name == "temp" || file_name == ".tmp" || file_name == ".temp" ||
       file_name.starts_with("temp-") || file_name.starts_with("tmp-") || 
       file_name.ends_with("-temp") || file_name.ends_with("-tmp") {
        return Some(CruftyReason::TempDir);
    }
    
    // Check for virtual environments
    if file_name == "venv" || file_name == "env" || file_name == ".venv" || file_name == ".env" || 
       file_name.starts_with("virtualenv") {
        return Some(CruftyReason::VenvDir);
    }
    
    // Check for distribution directories
    if file_name == "dist" || file_name == "out" || file_name.contains("dist") {
        return Some(CruftyReason::DistDir);
    }
    
    // Check for tox directories
    if file_name == ".tox" {
        return Some(CruftyReason::ToxDir);
    }
    
    // Check for CACHEDIR.TAG
    let cachedir_tag_path = path.join("CACHEDIR.TAG");
    if cachedir_tag_path.exists() {
        return Some(CruftyReason::CacheTagFound);
    }
    
        
    None
}

/// Calculates the age of the newest file in a directory in days
fn get_newest_file_age_days(path: &Path) -> Result<u64> {
    let mut newest_time = SystemTime::UNIX_EPOCH; // Start with the oldest possible time
    let now = SystemTime::now();
    let mut found_file = false;
    
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(metadata) = fs::metadata(entry.path()) {
            if let Ok(modified_time) = metadata.modified() {
                if newest_time == SystemTime::UNIX_EPOCH || modified_time > newest_time {
                    newest_time = modified_time;
                    found_file = true;
                }
            }
        }
    }
    
    if !found_file {
        // If no files found, use the directory's own modification time
        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(modified_time) = metadata.modified() {
                newest_time = modified_time;
                found_file = true;
            }
        }
    }
    
    if found_file {
        if let Ok(duration) = now.duration_since(newest_time) {
            // Convert seconds to days (86400 seconds in a day)
            return Ok(duration.as_secs() / 86400);
        }
    }
    
    // Default to 0 days if we couldn't determine the age
    Ok(0)
}

fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut total_size = 0;
    
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(metadata) = fs::metadata(entry.path()) {
            total_size += metadata.len();
        }
    }
    
    Ok(total_size)
}