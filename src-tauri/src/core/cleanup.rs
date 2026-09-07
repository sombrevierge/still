use crate::core::safety::canonical_safe_path;
use crate::core::types::{CleanupPreview, CleanupResult};
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("selected item")
        .to_string()
}

fn item_size(path: &Path) -> u64 {
    if path.is_file() {
        return fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
    }
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && !entry.path_is_symlink())
        .filter_map(|entry| entry.metadata().ok().map(|metadata| metadata.len()))
        .sum()
}

fn validate(paths: &[String]) -> (Vec<(PathBuf, u64)>, Vec<String>) {
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for raw in paths {
        let source = Path::new(raw);
        match canonical_safe_path(source) {
            Ok(path) => eligible.push((path.clone(), item_size(&path))),
            Err(error) => rejected.push(format!("{} — {error}", display_name(source))),
        }
    }
    (eligible, rejected)
}

pub fn dry_run(paths: Vec<String>) -> CleanupPreview {
    let requested = paths.len();
    let (eligible, rejected) = validate(&paths);
    CleanupPreview {
        requested,
        eligible: eligible.len(),
        total_size_bytes: eligible.iter().map(|(_, size)| *size).sum(),
        rejected,
    }
}

fn write_action_log(count: usize, size: u64) {
    let Some(base) = dirs::data_local_dir() else {
        return;
    };
    let directory = base.join("Still");
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("actions.log"))
    {
        let _ = writeln!(
            file,
            "{} action=recycle_bin count={} bytes={}",
            Utc::now().to_rfc3339(),
            count,
            size
        );
    }
}

pub fn move_to_recycle_bin(paths: Vec<String>) -> CleanupResult {
    let (eligible, rejected) = validate(&paths);
    let mut result = CleanupResult {
        moved_to_recycle_bin: 0,
        total_size_bytes: 0,
        failed: rejected,
    };
    for (path, size) in eligible {
        match trash::delete(&path) {
            Ok(()) => {
                result.moved_to_recycle_bin += 1;
                result.total_size_bytes = result.total_size_bytes.saturating_add(size);
            }
            Err(error) => result
                .failed
                .push(format!("{} — {error}", display_name(&path))),
        }
    }
    write_action_log(result.moved_to_recycle_bin, result.total_size_bytes);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn missing_paths_are_rejected_by_dry_run() {
        let preview = dry_run(vec!["Z:\\definitely-missing\\file.zip".into()]);
        assert_eq!(preview.eligible, 0);
        assert_eq!(preview.rejected.len(), 1);
    }

    #[test]
    fn explicit_test_file_moves_to_recycle_bin() {
        let root = std::env::temp_dir().join(format!("still-recycle-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("safe-cleanup-smoke-test.zip");
        fs::write(&file, b"Still cleanup integration test").unwrap();
        let paths = vec![file.to_string_lossy().to_string()];
        let preview = dry_run(paths.clone());
        assert_eq!(preview.eligible, 1);
        let result = move_to_recycle_bin(paths);
        assert_eq!(result.moved_to_recycle_bin, 1);
        assert!(!file.exists());
        fs::remove_dir_all(&root).unwrap();
    }
}
