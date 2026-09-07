use crate::core::cleanup;
use crate::core::types::{CleanupResult, MaintenanceReport, MaintenanceTarget};
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

const MANUAL_MAX_ITEMS: usize = 2_000;
const AUTOMATIC_MAX_ITEMS: usize = 500;
const MANUAL_MAX_BYTES: u64 = 6 * 1_073_741_824;
const AUTOMATIC_MAX_BYTES: u64 = 2 * 1_073_741_824;

#[derive(Clone)]
struct MaintenanceLocation {
    id: String,
    title: String,
    detail: String,
    category: String,
    root: PathBuf,
    minimum_age_days: u64,
    automatic_eligible: bool,
}

pub fn analyze(include_browser_caches: bool) -> MaintenanceReport {
    let mut targets = maintenance_locations(include_browser_caches)
        .into_iter()
        .filter_map(|location| analyze_location(&location))
        .collect::<Vec<_>>();
    targets.sort_by_key(|target| std::cmp::Reverse(target.reclaimable_bytes));
    let total_reclaimable_bytes = targets.iter().map(|target| target.reclaimable_bytes).sum();
    let automatic_reclaimable_bytes = targets
        .iter()
        .filter(|target| target.automatic_eligible)
        .map(|target| target.reclaimable_bytes)
        .sum();
    MaintenanceReport {
        captured_at_ms: Utc::now().timestamp_millis(),
        total_reclaimable_bytes,
        automatic_reclaimable_bytes,
        targets,
    }
}

pub fn clean_selected(target_ids: Vec<String>, include_browser_caches: bool) -> CleanupResult {
    let requested = target_ids.into_iter().collect::<HashSet<_>>();
    let locations = maintenance_locations(include_browser_caches)
        .into_iter()
        .filter(|location| requested.contains(&location.id));
    clean_locations(locations, false, None)
}

pub fn clean_automatic(minimum_age_days: u64) -> CleanupResult {
    let locations = maintenance_locations(false)
        .into_iter()
        .filter(|location| location.automatic_eligible);
    clean_locations(locations, true, Some(minimum_age_days.clamp(3, 30)))
}

fn clean_locations(
    locations: impl Iterator<Item = MaintenanceLocation>,
    automatic: bool,
    age_override: Option<u64>,
) -> CleanupResult {
    let max_items = if automatic {
        AUTOMATIC_MAX_ITEMS
    } else {
        MANUAL_MAX_ITEMS
    };
    let max_bytes = if automatic {
        AUTOMATIC_MAX_BYTES
    } else {
        MANUAL_MAX_BYTES
    };
    let mut paths = Vec::new();
    let mut selected_bytes = 0u64;

    for location in locations {
        let age_days = age_override.unwrap_or(location.minimum_age_days);
        for (path, size, _) in old_files(&location.root, age_days) {
            if paths.len() >= max_items || selected_bytes.saturating_add(size) > max_bytes {
                break;
            }
            selected_bytes = selected_bytes.saturating_add(size);
            paths.push(path.to_string_lossy().to_string());
        }
        if paths.len() >= max_items || selected_bytes >= max_bytes {
            break;
        }
    }

    cleanup::move_to_recycle_bin(paths)
}

fn analyze_location(location: &MaintenanceLocation) -> Option<MaintenanceTarget> {
    if !location.root.is_dir() {
        return None;
    }
    let mut reclaimable_bytes = 0u64;
    let mut item_count = 0usize;
    for (_, size, _) in old_files(&location.root, location.minimum_age_days) {
        reclaimable_bytes = reclaimable_bytes.saturating_add(size);
        item_count += 1;
    }
    if item_count == 0 {
        return None;
    }
    Some(MaintenanceTarget {
        id: location.id.clone(),
        title: location.title.clone(),
        detail: location.detail.clone(),
        category: location.category.clone(),
        path: location.root.to_string_lossy().to_string(),
        reclaimable_bytes,
        item_count,
        minimum_age_days: location.minimum_age_days,
        automatic_eligible: location.automatic_eligible,
    })
}

fn old_files(root: &Path, minimum_age_days: u64) -> Vec<(PathBuf, u64, SystemTime)> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(minimum_age_days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !entry.path_is_symlink())
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && !entry.path_is_symlink())
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            (modified <= cutoff).then(|| (entry.into_path(), metadata.len(), modified))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(_, _, modified)| *modified);
    files
}

fn maintenance_locations(include_browser_caches: bool) -> Vec<MaintenanceLocation> {
    let mut locations = Vec::new();
    let Some(local) = dirs::data_local_dir() else {
        return locations;
    };
    push_location(
        &mut locations,
        "user-temp",
        "Aged temporary files",
        "Files unused for at least seven days in the current Windows profile.",
        "temporary",
        local.join("Temp"),
        7,
        true,
    );
    push_location(
        &mut locations,
        "crash-dumps",
        "Application crash dumps",
        "Local diagnostic dumps left behind by crashed applications.",
        "diagnostics",
        local.join("CrashDumps"),
        3,
        true,
    );
    push_location(
        &mut locations,
        "d3d-cache",
        "DirectX shader cache",
        "Regenerable graphics cache; applications rebuild entries when needed.",
        "cache",
        local.join("D3DSCache"),
        7,
        true,
    );
    push_location(
        &mut locations,
        "npm-cache",
        "npm download cache",
        "Old package downloads. Cleaning may make the next npm install slower.",
        "developer-cache",
        local.join("npm-cache"),
        30,
        false,
    );
    push_location(
        &mut locations,
        "pip-cache",
        "Python package cache",
        "Old Python wheels and downloads; environments and source files are excluded.",
        "developer-cache",
        local.join("pip").join("Cache"),
        30,
        false,
    );

    if let Some(home) = dirs::home_dir() {
        push_location(
            &mut locations,
            "gradle-cache",
            "Gradle artifact cache",
            "Old downloaded build artifacts; Android projects themselves are never scanned here.",
            "developer-cache",
            home.join(".gradle").join("caches"),
            60,
            false,
        );
        push_location(
            &mut locations,
            "cargo-download-cache",
            "Cargo crate archives",
            "Old downloaded crate archives; registry sources and build targets are preserved.",
            "developer-cache",
            home.join(".cargo").join("registry").join("cache"),
            60,
            false,
        );
    }

    if include_browser_caches {
        add_chromium_profiles(
            &mut locations,
            "edge",
            "Microsoft Edge",
            local.join("Microsoft").join("Edge").join("User Data"),
        );
        add_chromium_profiles(
            &mut locations,
            "chrome",
            "Google Chrome",
            local.join("Google").join("Chrome").join("User Data"),
        );
        add_firefox_profiles(
            &mut locations,
            local.join("Mozilla").join("Firefox").join("Profiles"),
        );
    }
    locations
}

fn add_chromium_profiles(
    locations: &mut Vec<MaintenanceLocation>,
    id_prefix: &str,
    browser: &str,
    user_data: PathBuf,
) {
    let Ok(entries) = fs::read_dir(user_data) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name != "Default" && !name.starts_with("Profile ") && name != "Guest Profile" {
            continue;
        }
        let profile_id = name.to_ascii_lowercase().replace([' ', '.'], "-");
        for (suffix, folder, label) in [
            ("cache", "Cache", "web cache"),
            ("code-cache", "Code Cache", "compiled code cache"),
            ("gpu-cache", "GPUCache", "graphics cache"),
        ] {
            push_location(
                locations,
                &format!("{id_prefix}-{profile_id}-{suffix}"),
                &format!("{browser} {label}"),
                "Regenerable browser data. Open or locked files are skipped.",
                "browser-cache",
                entry.path().join(folder),
                14,
                false,
            );
        }
    }
}

fn add_firefox_profiles(locations: &mut Vec<MaintenanceLocation>, profiles: PathBuf) {
    let Ok(entries) = fs::read_dir(profiles) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        push_location(
            locations,
            &format!(
                "firefox-{}",
                name.to_ascii_lowercase().replace([' ', '.'], "-")
            ),
            "Firefox web cache",
            "Regenerable browser data. Open or locked files are skipped.",
            "browser-cache",
            entry.path().join("cache2"),
            14,
            false,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_location(
    locations: &mut Vec<MaintenanceLocation>,
    id: &str,
    title: &str,
    detail: &str,
    category: &str,
    root: PathBuf,
    minimum_age_days: u64,
    automatic_eligible: bool,
) {
    if root.is_dir() {
        locations.push(MaintenanceLocation {
            id: id.into(),
            title: title.into(),
            detail: detail.into(),
            category: category.into(),
            root,
            minimum_age_days,
            automatic_eligible,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn old_files_exclude_recent_entries() {
        let root = std::env::temp_dir().join(format!("still-maintenance-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("recent.tmp"), b"recent").unwrap();
        assert!(old_files(&root, 7).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_locations_are_allow_listed() {
        let automatic = maintenance_locations(false)
            .into_iter()
            .filter(|location| location.automatic_eligible)
            .map(|location| location.id)
            .collect::<Vec<_>>();
        assert!(automatic
            .iter()
            .all(|id| ["user-temp", "crash-dumps", "d3d-cache"].contains(&id.as_str())));
    }

    #[test]
    fn selected_allow_list_location_is_measured_and_recycled() {
        let root = std::env::temp_dir().join(format!("still-maintenance-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("old-cache.bin");
        fs::write(&file, vec![7u8; 4_096]).unwrap();
        let location = MaintenanceLocation {
            id: "test-cache".into(),
            title: "Test cache".into(),
            detail: "Test-only allow list".into(),
            category: "test".into(),
            root: root.clone(),
            minimum_age_days: 0,
            automatic_eligible: false,
        };

        let target = analyze_location(&location).expect("test cache should be measured");
        assert_eq!(target.item_count, 1);
        assert_eq!(target.reclaimable_bytes, 4_096);
        let result = clean_locations([location].into_iter(), false, None);
        assert_eq!(result.moved_to_recycle_bin, 1);
        assert!(!file.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
