use once_cell::sync::Lazy;
use std::env;
use std::path::{Path, PathBuf};

static PROTECTED_PROCESS_NAMES: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "system",
        "registry",
        "memory compression",
        "secure system",
        "idle",
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "lsaiso.exe",
        "logonui.exe",
        "winlogon.exe",
        "userinit.exe",
        "fontdrvhost.exe",
        "dwm.exe",
        "svchost.exe",
        "audiodg.exe",
        "sihost.exe",
        "taskhostw.exe",
        "ctfmon.exe",
        "textinputhost.exe",
        "shellexperiencehost.exe",
        "startmenuexperiencehost.exe",
        "lockapp.exe",
        "msmpeng.exe",
        "securityhealthservice.exe",
        "explorer.exe",
    ]
});

pub fn process_protection(pid: u32, name: &str, executable: &str) -> Option<String> {
    if pid <= 4 {
        return Some("Windows kernel process".into());
    }
    if pid == std::process::id() {
        return Some("Still protects its own process".into());
    }

    let lowered = name.to_ascii_lowercase();
    if PROTECTED_PROCESS_NAMES.iter().any(|item| *item == lowered) {
        return Some("Protected Windows session process".into());
    }

    let path = executable.to_ascii_lowercase().replace('/', "\\");
    if path.ends_with("\\windows\\explorer.exe") {
        return Some("Windows shell is protected".into());
    }
    None
}

pub fn protected_roots() -> Vec<PathBuf> {
    ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
        .iter()
        .filter_map(|key| env::var_os(key).map(PathBuf::from))
        .collect()
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_ascii_lowercase()
}

pub fn is_protected_path(path: &Path) -> bool {
    let candidate = normalized(path);
    protected_roots().iter().any(|root| {
        let root = normalized(root);
        candidate == root || candidate.starts_with(&(root + "\\"))
    })
}

pub fn canonical_safe_path(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Path is unavailable: {error}"))?;
    if canonical.parent().is_none() || canonical.components().count() <= 2 {
        return Err("Drive roots cannot be selected".into());
    }
    if is_protected_path(&canonical) {
        return Err("This path is protected by policy".into());
    }
    let metadata = std::fs::symlink_metadata(&canonical)
        .map_err(|error| format!("Cannot inspect path: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Symbolic links and junctions are not accepted".into());
    }
    Ok(canonical)
}

pub fn path_is_within(path: &Path, parent: &Path) -> bool {
    let path = normalized(path);
    let parent = normalized(parent);
    path == parent || path.starts_with(&(parent + "\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_process_names_are_protected() {
        assert!(process_protection(888, "lsass.exe", "").is_some());
        assert!(process_protection(888, "notepad.exe", "C:\\Tools\\notepad.exe").is_none());
        assert!(
            process_protection(888, "taskmgr.exe", "C:\\Windows\\System32\\Taskmgr.exe").is_none()
        );
    }

    #[test]
    fn kernel_pids_are_protected() {
        assert!(process_protection(4, "System", "").is_some());
    }
}
