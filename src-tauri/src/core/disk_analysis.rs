use crate::core::safety::{canonical_safe_path, path_is_within};
use crate::core::types::{CleanupGroup, FileCandidate, ScanStatus};
use chrono::Utc;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

static TOKEN_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-z0-9а-яё]+").unwrap());
static VERSION_TOKEN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:v|r)?\d+(?:\d|[._-])*$|^20\d{6,}$|^\d{4}-\d{2}-\d{2}$").unwrap());

#[derive(Default)]
pub struct ScanManager {
    current: Mutex<Option<Arc<ScanRuntime>>>,
}

struct ScanRuntime {
    status: Mutex<ScanStatus>,
    cancel: AtomicBool,
}

#[derive(Default)]
struct BuildAggregate {
    size: u64,
    newest_ms: i64,
}

impl ScanManager {
    pub fn start(
        &self,
        roots: Vec<String>,
        exclusions: Vec<String>,
        minimum_size_mb: u64,
        old_days: u64,
    ) -> Result<String, String> {
        if roots.is_empty() {
            return Err("Choose at least one folder".into());
        }
        if let Some(existing) = self.current.lock().unwrap().as_ref() {
            if existing.status.lock().unwrap().running {
                return Err("A storage scan is already running".into());
            }
        }

        let roots = roots
            .iter()
            .map(|path| canonical_safe_path(Path::new(path)))
            .collect::<Result<Vec<_>, _>>()?;
        for root in &roots {
            if !root.is_dir() {
                return Err(format!("{} is not a folder", root.display()));
            }
        }
        let exclusions = exclusions
            .iter()
            .filter_map(|path| canonical_safe_path(Path::new(path)).ok())
            .collect::<Vec<_>>();

        let id = Uuid::new_v4().to_string();
        let runtime = Arc::new(ScanRuntime {
            status: Mutex::new(ScanStatus {
                id: id.clone(),
                running: true,
                complete: false,
                cancelled: false,
                scanned_files: 0,
                scanned_bytes: 0,
                current_path: roots[0].to_string_lossy().to_string(),
                started_at_ms: Utc::now().timestamp_millis(),
                finished_at_ms: None,
                error: None,
                groups: Vec::new(),
            }),
            cancel: AtomicBool::new(false),
        });
        *self.current.lock().unwrap() = Some(runtime.clone());

        std::thread::spawn(move || {
            let outcome = run_scan(
                runtime.clone(),
                roots,
                exclusions,
                minimum_size_mb.max(25) * 1_048_576,
                old_days.max(7),
            );
            let mut status = runtime.status.lock().unwrap();
            status.running = false;
            status.complete = outcome.is_ok() && !runtime.cancel.load(Ordering::Relaxed);
            status.cancelled = runtime.cancel.load(Ordering::Relaxed);
            status.finished_at_ms = Some(Utc::now().timestamp_millis());
            if let Err(error) = outcome {
                status.error = Some(error);
            }
        });
        Ok(id)
    }

    pub fn status(&self) -> Option<ScanStatus> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .map(|runtime| runtime.status.lock().unwrap().clone())
    }

    pub fn cancel(&self) -> bool {
        if let Some(runtime) = self.current.lock().unwrap().as_ref() {
            runtime.cancel.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

fn is_reparse_or_symlink(entry: &DirEntry) -> bool {
    entry.path_is_symlink()
        || entry
            .metadata()
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
}

fn should_enter(entry: &DirEntry, exclusions: &[PathBuf]) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    !is_reparse_or_symlink(entry)
        && !exclusions
            .iter()
            .any(|excluded| path_is_within(entry.path(), excluded))
}

fn timestamp_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn build_root(path: &Path, root: &Path) -> Option<PathBuf> {
    let names = [
        "node_modules",
        ".next",
        "dist",
        "build",
        "__pycache__",
        ".pytest_cache",
    ];
    let mut current = path.parent();
    let mut found = None;
    while let Some(parent) = current {
        if parent == root {
            break;
        }
        if parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                names
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(name))
            })
        {
            found = Some(parent.to_path_buf());
        }
        current = parent.parent();
    }
    found
}

fn category(
    path: &Path,
    size: u64,
    modified: SystemTime,
    minimum_size: u64,
) -> Option<(&'static str, String)> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    let looks_project = [
        "session", "bundle", "model", "report", "result", "quant", "research",
    ]
    .iter()
    .any(|token| name.contains(token));
    let archive =
        ["zip", "7z", "rar", "gz", "tgz", "tar", "bz2", "xz"].contains(&extension.as_str());
    let installer = ["exe", "msi", "msix", "appx", "appxbundle"].contains(&extension.as_str());

    if archive && looks_project {
        Some((
            "project-archives",
            "A project/session archive; review retention rather than deleting by size alone".into(),
        ))
    } else if installer && size >= 10 * 1_048_576 {
        Some((
            "installers",
            "Downloaded installer; verify the application is already installed".into(),
        ))
    } else if archive && size >= 10 * 1_048_576 {
        Some((
            "archives",
            "Archive that may duplicate an extracted folder".into(),
        ))
    } else if age >= Duration::from_secs(30 * 24 * 60 * 60) && size >= 50 * 1_048_576 {
        Some((
            "old-downloads",
            "Large file not modified for at least 30 days".into(),
        ))
    } else if size >= minimum_size {
        Some(("large-files", "Large file; no deletion is implied".into()))
    } else {
        None
    }
}

fn series_key(name: &str, category: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let without_extensions = lower
        .trim_end_matches(".tar.gz")
        .trim_end_matches(".zip")
        .trim_end_matches(".rar")
        .trim_end_matches(".7z")
        .trim_end_matches(".exe")
        .trim_end_matches(".msi");
    let generic = [
        "full",
        "source",
        "windows",
        "migration",
        "latest",
        "current",
        "compact",
        "old",
        "backup",
        "bundle",
        "kit",
        "results",
        "report",
        "installer",
        "setup",
        "rebuild",
    ];
    let mut tokens = Vec::new();
    for token in TOKEN_SPLIT.split(without_extensions) {
        if token.len() < 2 || VERSION_TOKEN.is_match(token) || generic.contains(&token) {
            continue;
        }
        tokens.push(token);
        if tokens.len() == 3 {
            break;
        }
    }
    if tokens.is_empty() {
        category.to_string()
    } else if category == "project-archives" {
        tokens.into_iter().take(2).collect::<Vec<_>>().join(" ")
    } else {
        tokens.join(" ")
    }
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn group_candidates(candidates: Vec<FileCandidate>) -> Vec<CleanupGroup> {
    let mut map: HashMap<(String, String), Vec<FileCandidate>> = HashMap::new();
    for candidate in candidates {
        map.entry((candidate.category.clone(), candidate.series.clone()))
            .or_default()
            .push(candidate);
    }
    let mut groups = map
        .into_iter()
        .map(|((category, series), mut files)| {
            files.sort_by_key(|file| std::cmp::Reverse(file.modified_at_ms));
            let total_size_bytes = files.iter().map(|file| file.size_bytes).sum();
            let oldest_at_ms = files
                .iter()
                .map(|file| file.modified_at_ms)
                .min()
                .unwrap_or(0);
            let newest_at_ms = files
                .iter()
                .map(|file| file.modified_at_ms)
                .max()
                .unwrap_or(0);
            let label = match category.as_str() {
                "project-archives" => "archives",
                "old-builds" => "build artifacts",
                "old-downloads" => "old downloads",
                "large-files" => "large files",
                "installers" => "installers",
                _ => "archives",
            };
            let title = if series == category {
                title_case(label)
            } else {
                format!("{} {}", title_case(&series), label)
            };
            CleanupGroup {
                id: Uuid::new_v4().to_string(),
                title,
                category,
                summary: files
                    .first()
                    .map(|file| file.reason.clone())
                    .unwrap_or_default(),
                file_count: files.len(),
                total_size_bytes,
                oldest_at_ms,
                newest_at_ms,
                files,
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by_key(|group| std::cmp::Reverse(group.total_size_bytes));
    groups
}

fn run_scan(
    runtime: Arc<ScanRuntime>,
    roots: Vec<PathBuf>,
    exclusions: Vec<PathBuf>,
    minimum_size: u64,
    old_days: u64,
) -> Result<(), String> {
    let mut candidates = Vec::new();
    let mut builds: HashMap<PathBuf, BuildAggregate> = HashMap::new();
    let old_cutoff = Utc::now().timestamp_millis() - old_days as i64 * 86_400_000;

    for root in &roots {
        let iterator = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_enter(entry, &exclusions));
        for entry in iterator {
            if runtime.cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let size = metadata.len();
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            let modified_ms = timestamp_ms(modified);
            {
                let mut status = runtime.status.lock().unwrap();
                status.scanned_files += 1;
                status.scanned_bytes = status.scanned_bytes.saturating_add(size);
                if status.scanned_files.is_multiple_of(200) {
                    status.current_path = entry.path().to_string_lossy().to_string();
                }
            }

            if let Some(build) = build_root(entry.path(), root) {
                let item = builds.entry(build).or_default();
                item.size = item.size.saturating_add(size);
                item.newest_ms = item.newest_ms.max(modified_ms);
                continue;
            }
            if let Some((category, reason)) = category(entry.path(), size, modified, minimum_size) {
                let name = entry.file_name().to_string_lossy().to_string();
                candidates.push(FileCandidate {
                    id: Uuid::new_v4().to_string(),
                    path: entry.path().to_string_lossy().to_string(),
                    name: name.clone(),
                    size_bytes: size,
                    modified_at_ms: modified_ms,
                    category: category.into(),
                    series: series_key(&name, category),
                    reason,
                });
            }
        }
    }

    for (path, aggregate) in builds {
        if aggregate.size < 50 * 1_048_576 || aggregate.newest_ms >= old_cutoff {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("build")
            .to_string();
        candidates.push(FileCandidate {
            id: Uuid::new_v4().to_string(),
            path: path.to_string_lossy().to_string(),
            name: name.clone(),
            size_bytes: aggregate.size,
            modified_at_ms: aggregate.newest_ms,
            category: "old-builds".into(),
            series: name,
            reason: "Rebuildable dependency or build output not modified recently".into(),
        });
    }
    let groups = group_candidates(candidates);
    runtime.status.lock().unwrap().groups = groups;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::thread;

    #[test]
    fn project_versions_share_a_series() {
        assert_eq!(
            series_key("pepe-quant-v0195-session-bundle.zip", "project-archives"),
            "pepe quant"
        );
        assert_eq!(
            series_key("pepe-quant-v0194-results.zip", "project-archives"),
            "pepe quant"
        );
    }

    fn wait_until_finished(manager: &ScanManager) -> ScanStatus {
        for _ in 0..100 {
            let status = manager.status().expect("scan status");
            if !status.running {
                return status;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("scan did not finish in time");
    }

    #[test]
    fn background_scan_groups_a_large_project_archive() {
        let root = std::env::temp_dir().join(format!("still-scan-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("pepe-quant-v0195-session-bundle.zip");
        let file = File::create(&archive).unwrap();
        file.set_len(30 * 1_048_576).unwrap();

        let manager = ScanManager::default();
        manager
            .start(vec![root.to_string_lossy().to_string()], Vec::new(), 25, 30)
            .unwrap();
        let status = wait_until_finished(&manager);
        assert!(status.complete);
        assert_eq!(status.scanned_files, 1);
        assert!(status
            .groups
            .iter()
            .any(|group| group.category == "project-archives"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn running_scan_can_be_cancelled() {
        let root = std::env::temp_dir().join(format!("still-cancel-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        for index in 0..500 {
            fs::write(root.join(format!("file-{index}.txt")), b"test").unwrap();
        }
        let manager = ScanManager::default();
        manager
            .start(vec![root.to_string_lossy().to_string()], Vec::new(), 25, 30)
            .unwrap();
        assert!(manager.cancel());
        let status = wait_until_finished(&manager);
        assert!(status.cancelled);
        fs::remove_dir_all(&root).unwrap();
    }
}
