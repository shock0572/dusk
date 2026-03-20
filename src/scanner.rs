use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rayon::prelude::*;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Entry>,
    pub error: bool,
}

impl Entry {
    pub fn child_count(&self) -> usize {
        if self.is_dir {
            self.children.len()
        } else {
            0
        }
    }

}

pub struct ScanProgress {
    pub files_scanned: Arc<AtomicU64>,
    pub bytes_scanned: Arc<AtomicU64>,
    pub cancelled: Arc<AtomicBool>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            files_scanned: Arc::new(AtomicU64::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn scan_directory(path: &Path, progress: &ScanProgress) -> Entry {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    if progress.cancelled.load(Ordering::Relaxed) {
        return Entry {
            name,
            path: path.to_path_buf(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            error: false,
        };
    }

    let entries: Vec<fs::DirEntry> = match fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => {
            return Entry {
                name,
                path: path.to_path_buf(),
                size: 0,
                is_dir: true,
                children: Vec::new(),
                error: true,
            };
        }
    };

    let children: Vec<Entry> = entries
        .par_iter()
        .filter_map(|dir_entry| {
            if progress.cancelled.load(Ordering::Relaxed) {
                return None;
            }

            let entry_path = dir_entry.path();
            let meta = match dir_entry.metadata() {
                Ok(m) => m,
                Err(_) => return None,
            };

            if meta.is_symlink() {
                return None;
            }

            let entry_name = dir_entry.file_name().to_string_lossy().to_string();

            if meta.is_dir() {
                Some(scan_directory(&entry_path, progress))
            } else {
                let size = meta.len();
                progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                progress.bytes_scanned.fetch_add(size, Ordering::Relaxed);

                Some(Entry {
                    name: entry_name,
                    path: entry_path,
                    size,
                    is_dir: false,
                    children: Vec::new(),
                    error: false,
                })
            }
        })
        .collect();

    let total_size: u64 = children.iter().map(|c| c.size).sum();
    progress.files_scanned.fetch_add(1, Ordering::Relaxed);

    Entry {
        name,
        path: path.to_path_buf(),
        size: total_size,
        is_dir: true,
        children,
        error: false,
    }
}
