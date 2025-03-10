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
    pub newest_file_age_days: Option<f64>,
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
    RustTargetDir,
    TempDir,
    VenvDir,
    DistDir,
    ToxDir,
}

impl std::fmt::Display for CruftyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CruftyReason::BuildDir => write!(f, "build dir"),
            CruftyReason::CacheDir => write!(f, "cache dir"),
            CruftyReason::CacheTagFound => write!(f, "CACHEDIR.TAG"),
            CruftyReason::DistDir => write!(f, "dist dir"),
            CruftyReason::NodeModules => write!(f, "node_modules"),
            CruftyReason::RustTargetDir => write!(f, "rust target dir"),
            CruftyReason::TempDir => write!(f, "temp dir"),
            CruftyReason::ToxDir => write!(f, "tox dir"),
            CruftyReason::VenvDir => write!(f, "venv"),
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
                let cruft_dir = CruftDirectory {
                    path: path.to_path_buf(),
                    size: calculate_dir_size(path).unwrap_or(0),
                    crufty_reason: reason,
                    newest_file_age_days: get_newest_file_age_days(path).unwrap_or(None),
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
    ".git",
    ".github",
    ".idea",
    ".vscode",
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
    // Skip protected directories
    if is_protected_directory(path) {
        return None;
    }
    let path_str = path.to_string_lossy();

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
    if file_name == "build" || file_name.contains("build") {
        return Some(CruftyReason::BuildDir);
    }

    if file_name == "target" && path.join(".rustc_info.json").is_file() {
        return Some(CruftyReason::RustTargetDir);
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
fn get_newest_file_age_days(path: &Path) -> Result<Option<f64>> {
    let now = SystemTime::now();

    let newest_child_mtime = WalkDir::new(path)
        .max_depth(3)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|entry| {
            if let Ok(metadata) = fs::metadata(entry.path()) {
                metadata.modified().ok()
            } else {
                None
            }
        })
        .max();

    if let Some(newest_child_mtime) = newest_child_mtime {
        let since = now.duration_since(newest_child_mtime)?.as_secs();
        return Ok(Some(since as f64 / 86400.0));
    }

    // If no files found, use the directory's own modification time
    if let Ok(metadata) = fs::metadata(path) {
        if let Ok(modified_time) = metadata.modified() {
            let since = now.duration_since(modified_time)?.as_secs();
            return Ok(Some(since as f64 / 86400.0));
        }
    }
    Ok(None)
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