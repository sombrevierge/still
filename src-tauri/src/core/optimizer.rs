use crate::core::processes::process_inventory;
use crate::core::types::{
    OptimizationAction, OptimizationResult, OptimizedProcess, OptimizerStatus, ProcessInfo,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows_sys::Win32::System::Threading::{
    GetPriorityClass, OpenProcess, SetPriorityClass, BELOW_NORMAL_PRIORITY_CLASS,
    NORMAL_PRIORITY_CLASS, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA,
};

const PRESSURE_SAMPLES_BEFORE_TUNING: u32 = 2;
const MAX_TUNED_PROCESSES: usize = 8;
const TRIM_COOLDOWN: Duration = Duration::from_secs(10 * 60);

const BUILT_IN_EXCLUSIONS: &[&str] = &[
    "docker",
    "vmmem",
    "wsl",
    "studio64",
    "emulator",
    "qemu",
    "adb",
    "java",
    "gradle",
    "cargo",
    "rustc",
    "node",
    "python",
    "msbuild",
    "devenv",
    "code.exe",
    "codex",
    "pwsh",
    "cmd.exe",
    "conhost",
    "git.exe",
    "localtypeassist",
    "still-session-lens",
    "still-hardware-sensors",
];

#[derive(Clone, Debug)]
struct TunedProcess {
    name: String,
    original_priority: u32,
}

#[derive(Default)]
pub struct AdaptiveOptimizer {
    pressure_streak: u32,
    tuned: HashMap<u32, TunedProcess>,
    last_trimmed: HashMap<u32, Instant>,
    last_action: String,
}

impl AdaptiveOptimizer {
    pub fn tick(
        &mut self,
        enabled: bool,
        under_pressure: bool,
        candidate_pids: Vec<u32>,
        custom_exclusions: Vec<String>,
        trim_memory: bool,
        aggressive: bool,
    ) -> OptimizerStatus {
        let inventory = process_inventory();
        let live_pids: HashSet<u32> = inventory.iter().map(|process| process.pid).collect();
        let mut restored_this_tick = 0;
        let mut skipped_this_tick = 0;
        let mut trimmed_this_tick = 0;
        let mut trimmed_memory_before = HashMap::new();

        self.tuned.retain(|pid, tuned| {
            let should_restore = !enabled
                || !under_pressure
                || inventory
                    .iter()
                    .find(|process| process.pid == *pid)
                    .is_none_or(|process| {
                        process.foreground
                            || process.protected
                            || is_excluded(&process.name, &process.executable, &custom_exclusions)
                    });
            if should_restore
                && (!live_pids.contains(pid) || set_priority(*pid, tuned.original_priority).is_ok())
            {
                restored_this_tick += 1;
                return false;
            }
            true
        });

        if !enabled {
            self.pressure_streak = 0;
            self.last_action = if restored_this_tick > 0 {
                format!("Restored {restored_this_tick} background process priorities")
            } else {
                "Autopilot is off; no process priorities are modified".into()
            };
            return self.status(
                "off",
                restored_this_tick,
                skipped_this_tick,
                trimmed_this_tick,
                0,
            );
        }

        if under_pressure {
            self.pressure_streak = self.pressure_streak.saturating_add(1);
        } else {
            self.pressure_streak = 0;
            self.last_action = if restored_this_tick > 0 {
                format!("Pressure eased; restored {restored_this_tick} process priorities")
            } else {
                "Monitoring continuously; no sustained pressure detected".into()
            };
            return self.status(
                "monitoring",
                restored_this_tick,
                skipped_this_tick,
                trimmed_this_tick,
                0,
            );
        }

        let samples_before_tuning = if aggressive {
            1
        } else {
            PRESSURE_SAMPLES_BEFORE_TUNING
        };
        if self.pressure_streak < samples_before_tuning {
            self.last_action = format!(
                "Confirming sustained pressure ({}/{samples_before_tuning})",
                self.pressure_streak,
            );
            return self.status(
                "observing",
                restored_this_tick,
                skipped_this_tick,
                trimmed_this_tick,
                0,
            );
        }

        let candidates: HashSet<u32> = candidate_pids.into_iter().collect();
        for process in &inventory {
            if self.tuned.len() >= MAX_TUNED_PROCESSES {
                break;
            }
            if !candidates.contains(&process.pid) || self.tuned.contains_key(&process.pid) {
                continue;
            }
            if process.protected
                || process.foreground
                || (!aggressive && !process.background)
                || !process.responsive
                || has_foreground_ancestor(process, &inventory)
                || belongs_to_still(process, &inventory)
                || is_excluded(&process.name, &process.executable, &custom_exclusions)
            {
                skipped_this_tick += 1;
                continue;
            }

            match lower_priority(process.pid) {
                Ok(original_priority) => {
                    self.tuned.insert(
                        process.pid,
                        TunedProcess {
                            name: process.name.clone(),
                            original_priority,
                        },
                    );
                }
                Err(_) => skipped_this_tick += 1,
            }

            let trim_due = self
                .last_trimmed
                .get(&process.pid)
                .is_none_or(|last| last.elapsed() >= TRIM_COOLDOWN);
            if trim_memory
                && trim_due
                && process.memory_bytes >= 96 * 1_048_576
                && trim_working_set(process.pid).is_ok()
            {
                self.last_trimmed.insert(process.pid, Instant::now());
                trimmed_this_tick += 1;
                trimmed_memory_before.insert(process.pid, process.memory_bytes);
            }
        }

        let released_memory_bytes = if trimmed_memory_before.is_empty() {
            0
        } else {
            thread::sleep(Duration::from_millis(180));
            let after = process_inventory()
                .into_iter()
                .map(|process| (process.pid, process.memory_bytes))
                .collect::<HashMap<_, _>>();
            trimmed_memory_before
                .iter()
                .map(|(pid, before)| {
                    before.saturating_sub(after.get(pid).copied().unwrap_or(*before))
                })
                .sum()
        };

        self.last_action = if self.tuned.is_empty() && trimmed_this_tick == 0 {
            "Pressure is sustained, but no safe background process can be tuned".into()
        } else {
            format!(
                "Balancing {} process{}; trimmed {} and released {:.0} MB",
                self.tuned.len(),
                if self.tuned.len() == 1 { "" } else { "es" },
                trimmed_this_tick,
                released_memory_bytes as f64 / 1_048_576.0,
            )
        };
        self.status(
            "balancing",
            restored_this_tick,
            skipped_this_tick,
            trimmed_this_tick,
            released_memory_bytes,
        )
    }

    pub fn optimize_now(
        &mut self,
        custom_exclusions: Vec<String>,
        aggressive: bool,
    ) -> OptimizationResult {
        let before = process_inventory();
        let current_pid = std::process::id();
        let minimum_memory = if aggressive { 48 } else { 96 } * 1_048_576;
        let limit = if aggressive { 16 } else { 8 };
        let mut candidates = before
            .iter()
            .filter(|process| {
                process.pid != current_pid
                    && !process.protected
                    && !process.foreground
                    && process.responsive
                    && process.memory_bytes >= minimum_memory
                    && (aggressive || process.background)
                    && !has_foreground_ancestor(process, &before)
                    && !belongs_to_still(process, &before)
                    && !is_excluded(&process.name, &process.executable, &custom_exclusions)
            })
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_by_key(|process| std::cmp::Reverse(process.memory_bytes));
        candidates.truncate(limit);

        let mut result = OptimizationResult {
            captured_at_ms: Utc::now().timestamp_millis(),
            examined: candidates.len(),
            tuned: 0,
            trimmed: 0,
            released_memory_bytes: 0,
            skipped: 0,
            actions: Vec::new(),
            errors: Vec::new(),
        };
        let before_memory = candidates
            .iter()
            .map(|process| (process.pid, process.memory_bytes))
            .collect::<HashMap<_, _>>();

        for process in candidates {
            let mut acted = false;
            if process.cpu_percent >= 2.0 || aggressive {
                match self.tuned.entry(process.pid) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        if let Ok(original_priority) = lower_priority(process.pid) {
                            entry.insert(TunedProcess {
                                name: process.name.clone(),
                                original_priority,
                            });
                            result.tuned += 1;
                            acted = true;
                        }
                    }
                    std::collections::hash_map::Entry::Occupied(_) => {}
                }
            }
            match trim_working_set(process.pid) {
                Ok(()) => {
                    self.last_trimmed.insert(process.pid, Instant::now());
                    result.trimmed += 1;
                    acted = true;
                    result.actions.push(OptimizationAction {
                        pid: process.pid,
                        name: process.name.clone(),
                        action: "Memory trimmed".into(),
                        detail: format!(
                            "Released unused working-set pages from {:.0} MB",
                            process.memory_bytes as f64 / 1_048_576.0
                        ),
                    });
                }
                Err(error) => {
                    if result.errors.len() < 8 {
                        result.errors.push(format!("{}: {error}", process.name));
                    }
                }
            }
            if !acted {
                result.skipped += 1;
            }
        }

        thread::sleep(Duration::from_millis(350));
        let after = process_inventory();
        let after_memory = after
            .iter()
            .map(|process| (process.pid, process.memory_bytes))
            .collect::<HashMap<_, _>>();
        result.released_memory_bytes = before_memory
            .iter()
            .map(|(pid, before)| {
                before.saturating_sub(after_memory.get(pid).copied().unwrap_or_default())
            })
            .sum();
        self.last_action = format!(
            "Manual optimization trimmed {} processes and released {:.0} MB",
            result.trimmed,
            result.released_memory_bytes as f64 / 1_048_576.0
        );
        result
    }

    fn status(
        &self,
        mode: &str,
        restored_this_tick: usize,
        skipped_this_tick: usize,
        trimmed_this_tick: usize,
        released_memory_bytes: u64,
    ) -> OptimizerStatus {
        let mut tuned_processes: Vec<OptimizedProcess> = self
            .tuned
            .iter()
            .map(|(pid, process)| OptimizedProcess {
                pid: *pid,
                name: process.name.clone(),
                reason: "Sustained pressure · background priority temporarily reduced".into(),
            })
            .collect();
        tuned_processes.sort_by_key(|process| process.pid);
        OptimizerStatus {
            captured_at_ms: Utc::now().timestamp_millis(),
            enabled: mode != "off",
            mode: mode.into(),
            pressure_streak: self.pressure_streak,
            tuned_processes,
            restored_this_tick,
            skipped_this_tick,
            trimmed_this_tick,
            released_memory_bytes,
            last_action: self.last_action.clone(),
        }
    }

    fn restore_all(&mut self) {
        let tuned = std::mem::take(&mut self.tuned);
        for (pid, process) in tuned {
            let _ = set_priority(pid, process.original_priority);
        }
    }
}

impl Drop for AdaptiveOptimizer {
    fn drop(&mut self) {
        self.restore_all();
    }
}

pub(crate) fn is_excluded(name: &str, executable: &str, custom_exclusions: &[String]) -> bool {
    let name = name.to_ascii_lowercase();
    let executable = executable.to_ascii_lowercase();
    BUILT_IN_EXCLUSIONS
        .iter()
        .any(|pattern| name.contains(pattern))
        || custom_exclusions.iter().any(|item| {
            let item = item.to_ascii_lowercase();
            !item.is_empty() && (name == item || executable == item || executable.contains(&item))
        })
}

fn process_by_pid(inventory: &[ProcessInfo], pid: u32) -> Option<&ProcessInfo> {
    inventory.iter().find(|process| process.pid == pid)
}

fn has_foreground_ancestor(process: &ProcessInfo, inventory: &[ProcessInfo]) -> bool {
    let mut parent = process.parent_pid;
    for _ in 0..12 {
        let Some(pid) = parent else { return false };
        let Some(ancestor) = process_by_pid(inventory, pid) else {
            return false;
        };
        if ancestor.foreground {
            return true;
        }
        parent = ancestor.parent_pid;
    }
    false
}

fn belongs_to_still(process: &ProcessInfo, inventory: &[ProcessInfo]) -> bool {
    let mut current = Some(process.pid);
    for _ in 0..12 {
        let Some(pid) = current else { return false };
        let Some(item) = process_by_pid(inventory, pid) else {
            return false;
        };
        if item
            .name
            .to_ascii_lowercase()
            .contains("still-session-lens")
        {
            return true;
        }
        current = item.parent_pid;
    }
    false
}

fn trim_working_set(pid: u32) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, 0, pid);
        if handle.is_null() {
            return Err("Windows denied memory trimming".into());
        }
        let trimmed = K32EmptyWorkingSet(handle);
        CloseHandle(handle);
        if trimmed == 0 {
            Err("Windows could not trim the working set".into())
        } else {
            Ok(())
        }
    }
}

fn lower_priority(pid: u32) -> Result<u32, String> {
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_INFORMATION,
            0,
            pid,
        );
        if handle.is_null() {
            return Err("Windows denied access to tune this process".into());
        }
        let original = GetPriorityClass(handle);
        if original == 0 {
            CloseHandle(handle);
            return Err("Windows could not read this process priority".into());
        }
        if original != NORMAL_PRIORITY_CLASS {
            CloseHandle(handle);
            return Err("Only normal-priority processes may be tuned".into());
        }
        let changed = SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS);
        CloseHandle(handle);
        if changed == 0 {
            Err("Windows could not lower this process priority".into())
        } else {
            Ok(original)
        }
    }
}

fn set_priority(pid: u32, priority: u32) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            return Err("Process is no longer available".into());
        }
        let changed = SetPriorityClass(handle, priority);
        CloseHandle(handle);
        if changed == 0 {
            Err("Windows could not restore this process priority".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_workloads_are_excluded() {
        assert!(is_excluded("vmmemWSL", "", &[]));
        assert!(is_excluded("studio64.exe", "", &[]));
        assert!(is_excluded("com.docker.backend.exe", "", &[]));
        assert!(is_excluded("rustc.exe", "", &[]));
        assert!(!is_excluded(
            "sample-worker.exe",
            "C:\\Temp\\sample-worker.exe",
            &[]
        ));
    }

    #[test]
    fn protected_current_process_is_never_tuned() {
        let pid = std::process::id();
        let mut optimizer = AdaptiveOptimizer::default();
        for _ in 0..PRESSURE_SAMPLES_BEFORE_TUNING {
            let status = optimizer.tick(true, true, vec![pid], Vec::new(), true, false);
            assert!(status.tuned_processes.is_empty());
        }
    }
    #[test]
    fn foreground_descendants_are_not_candidates() {
        let root = ProcessInfo {
            pid: 10,
            parent_pid: None,
            name: "browser.exe".into(),
            executable: String::new(),
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            uptime_seconds: 0,
            status: "Run".into(),
            responsive: true,
            foreground: true,
            background: false,
            orphaned: false,
            child_count: 1,
            protected: false,
            protection_reason: None,
        };
        let child = ProcessInfo {
            pid: 11,
            parent_pid: Some(10),
            foreground: false,
            background: true,
            ..root.clone()
        };
        assert!(has_foreground_ancestor(&child, &[root, child.clone()]));
    }
}
