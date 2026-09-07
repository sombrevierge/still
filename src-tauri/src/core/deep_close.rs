use crate::core::processes;
use crate::core::types::{DeepCloseStatus, ProcessInfo};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const CLOSE_GRACE: Duration = Duration::from_secs(3);
const ACTION_VISIBILITY: Duration = Duration::from_secs(60);

const BUILT_IN_EXCLUSIONS: &[&str] = &[
    "docker",
    "com.docker",
    "vmmem",
    "wsl",
    "wslhost",
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
    "code",
    "cursor",
    "codex",
    "localtypeassist",
    "powershell",
    "pwsh",
    "cmd",
    "conhost",
    "windowsterminal",
    "taskmgr",
    "credentialuibroker",
    "applicationframehost",
    "systemsettings",
    "searchhost",
    "still-session-lens",
    "still-hardware-sensors",
];

const CONNECTION_EXCLUSIONS: &[&str] = &[
    "vpn",
    "wireguard",
    "openvpn",
    "outline",
    "tailscale",
    "zerotier",
    "clash",
    "sing-box",
    "hiddify",
    "v2ray",
    "xray",
    "amnezia",
    "warp",
];

#[derive(Clone, Debug)]
struct GroupMember {
    name: String,
    executable: String,
    depth: usize,
}

impl GroupMember {
    fn matches(&self, process: &ProcessInfo) -> bool {
        if !self.executable.is_empty() && !process.executable.is_empty() {
            self.executable.eq_ignore_ascii_case(&process.executable)
        } else {
            self.name.eq_ignore_ascii_case(&process.name)
        }
    }
}

#[derive(Clone, Debug)]
struct TrackedApp {
    name: String,
    members: HashMap<u32, GroupMember>,
    missing_since: Option<Instant>,
}

pub struct DeepCloseManager {
    tracked: HashMap<u32, TrackedApp>,
    last_action: String,
    last_action_at: Option<Instant>,
}

impl Default for DeepCloseManager {
    fn default() -> Self {
        Self {
            tracked: HashMap::new(),
            last_action: "Watching for application windows to close".into(),
            last_action_at: None,
        }
    }
}

impl DeepCloseManager {
    pub fn tick(
        &mut self,
        enabled: bool,
        custom_exclusions: &[String],
        inventory: &[ProcessInfo],
    ) -> DeepCloseStatus {
        self.tick_at(
            enabled,
            custom_exclusions,
            inventory,
            Instant::now(),
            processes::terminate_automatic,
        )
    }

    fn tick_at<F>(
        &mut self,
        enabled: bool,
        custom_exclusions: &[String],
        inventory: &[ProcessInfo],
        now: Instant,
        mut terminate: F,
    ) -> DeepCloseStatus
    where
        F: FnMut(u32) -> Result<(), String>,
    {
        if !enabled {
            self.tracked.clear();
            self.last_action =
                "Deep close is off; no applications are terminated automatically".into();
            self.last_action_at = None;
            return self.status(false, 0);
        }

        let by_pid = inventory
            .iter()
            .map(|process| (process.pid, process))
            .collect::<HashMap<_, _>>();
        let mut visible_roots = HashSet::new();

        for process in inventory.iter().filter(|process| !process.background) {
            if !eligible(process, custom_exclusions) {
                continue;
            }
            let root = group_root(process, &by_pid, custom_exclusions);
            if !eligible(root, custom_exclusions) || !visible_roots.insert(root.pid) {
                continue;
            }
            let members = group_members(root.pid, inventory, &by_pid);
            self.tracked
                .entry(root.pid)
                .and_modify(|tracked| {
                    tracked.name = root.name.clone();
                    tracked.members = members.clone();
                    tracked.missing_since = None;
                })
                .or_insert_with(|| TrackedApp {
                    name: root.name.clone(),
                    members,
                    missing_since: None,
                });
        }

        let mut ready = Vec::new();
        for (root_pid, tracked) in &mut self.tracked {
            if visible_roots.contains(root_pid) {
                tracked.missing_since = None;
                continue;
            }
            let missing_since = tracked.missing_since.get_or_insert(now);
            if now.duration_since(*missing_since) >= CLOSE_GRACE {
                ready.push(*root_pid);
            }
        }

        let mut closed_this_tick = 0;
        let mut processed_any = false;
        for root_pid in ready {
            processed_any = true;
            let Some(tracked) = self.tracked.remove(&root_pid) else {
                continue;
            };
            let mut candidates = tracked
                .members
                .iter()
                .filter_map(|(pid, member)| {
                    let current = by_pid.get(pid)?;
                    (member.matches(current) && eligible(current, custom_exclusions))
                        .then_some((*pid, member.depth))
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.1));

            if candidates.is_empty() {
                continue;
            }

            let mut failures = Vec::new();
            let mut group_closed = 0;
            for (pid, _) in candidates {
                match terminate(pid) {
                    Ok(()) => {
                        closed_this_tick += 1;
                        group_closed += 1;
                    }
                    Err(error) => failures.push(error),
                }
            }
            self.last_action = if failures.is_empty() {
                format!(
                    "Deep-closed {}; stopped {} leftover process{}",
                    tracked.name,
                    group_closed,
                    if group_closed == 1 { "" } else { "es" }
                )
            } else {
                format!(
                    "Could not close every {} process without administrator access",
                    tracked.name
                )
            };
            self.last_action_at = Some(now);
        }

        if !processed_any {
            let pending = self
                .tracked
                .values()
                .filter(|tracked| tracked.missing_since.is_some())
                .count();
            let recent_action = self
                .last_action_at
                .is_some_and(|action_at| now.duration_since(action_at) < ACTION_VISIBILITY);
            self.last_action = if recent_action {
                self.last_action.clone()
            } else if pending > 0 {
                format!(
                    "Waiting for {pending} application{} to exit cleanly",
                    if pending == 1 { "" } else { "s" }
                )
            } else {
                format!(
                    "Watching {} windowed application{}; no leftovers detected",
                    self.tracked.len(),
                    if self.tracked.len() == 1 { "" } else { "s" }
                )
            };
        }

        self.status(true, closed_this_tick)
    }

    fn status(&self, enabled: bool, closed_this_tick: usize) -> DeepCloseStatus {
        DeepCloseStatus {
            enabled,
            tracked_apps: self.tracked.len(),
            pending_apps: self
                .tracked
                .values()
                .filter(|tracked| tracked.missing_since.is_some())
                .count(),
            closed_this_tick,
            last_action: self.last_action.clone(),
        }
    }
}

fn group_root<'a>(
    process: &'a ProcessInfo,
    by_pid: &HashMap<u32, &'a ProcessInfo>,
    custom_exclusions: &[String],
) -> &'a ProcessInfo {
    let mut root = process;
    while let Some(parent) = root.parent_pid.and_then(|pid| by_pid.get(&pid).copied()) {
        if !eligible(parent, custom_exclusions) || !same_application(root, parent) {
            break;
        }
        root = parent;
    }
    root
}

fn same_application(child: &ProcessInfo, parent: &ProcessInfo) -> bool {
    if !child.executable.is_empty() && !parent.executable.is_empty() {
        child.executable.eq_ignore_ascii_case(&parent.executable)
    } else {
        child.name.eq_ignore_ascii_case(&parent.name)
    }
}

fn group_members(
    root_pid: u32,
    inventory: &[ProcessInfo],
    by_pid: &HashMap<u32, &ProcessInfo>,
) -> HashMap<u32, GroupMember> {
    inventory
        .iter()
        .filter_map(|process| {
            let depth = descendant_depth(process, root_pid, by_pid)?;
            Some((
                process.pid,
                GroupMember {
                    name: process.name.clone(),
                    executable: process.executable.clone(),
                    depth,
                },
            ))
        })
        .collect()
}

fn descendant_depth(
    process: &ProcessInfo,
    root_pid: u32,
    by_pid: &HashMap<u32, &ProcessInfo>,
) -> Option<usize> {
    if process.pid == root_pid {
        return Some(0);
    }
    let mut current = process;
    let mut depth = 0usize;
    let mut visited = HashSet::new();
    while let Some(parent_pid) = current.parent_pid {
        if !visited.insert(parent_pid) {
            return None;
        }
        depth += 1;
        if parent_pid == root_pid {
            return Some(depth);
        }
        current = by_pid.get(&parent_pid).copied()?;
    }
    None
}

fn eligible(process: &ProcessInfo, custom_exclusions: &[String]) -> bool {
    if process.protected {
        return false;
    }
    let name = process.name.to_ascii_lowercase();
    let stem = name.trim_end_matches(".exe");
    if BUILT_IN_EXCLUSIONS
        .iter()
        .any(|excluded| stem == *excluded || stem.starts_with(&format!("{excluded}-")))
        || CONNECTION_EXCLUSIONS
            .iter()
            .any(|excluded| stem.contains(excluded))
    {
        return false;
    }
    let executable = process.executable.to_ascii_lowercase();
    !custom_exclusions.iter().any(|excluded| {
        let excluded = excluded.trim().to_ascii_lowercase();
        !excluded.is_empty()
            && (name == excluded
                || stem == excluded.trim_end_matches(".exe")
                || executable == excluded
                || executable.contains(&excluded))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: u32, parent_pid: Option<u32>, background: bool) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid,
            name: "sample-app.exe".into(),
            executable: "C:\\Apps\\Sample\\sample-app.exe".into(),
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 1,
            uptime_seconds: 1,
            status: "Run".into(),
            responsive: true,
            foreground: false,
            background,
            orphaned: false,
            child_count: 0,
            protected: false,
            protection_reason: None,
        }
    }

    #[test]
    fn closes_leftovers_only_after_the_last_window_and_grace_period() {
        let start = Instant::now();
        let mut manager = DeepCloseManager::default();
        let mut terminated = Vec::new();
        let visible = vec![app(400, None, false), app(401, Some(400), true)];
        manager.tick_at(true, &[], &visible, start, |pid| {
            terminated.push(pid);
            Ok(())
        });
        let hidden = vec![app(400, None, true), app(401, Some(400), true)];
        manager.tick_at(true, &[], &hidden, start + Duration::from_secs(1), |pid| {
            terminated.push(pid);
            Ok(())
        });
        assert!(terminated.is_empty());
        let status = manager.tick_at(true, &[], &hidden, start + Duration::from_secs(5), |pid| {
            terminated.push(pid);
            Ok(())
        });
        assert_eq!(terminated, vec![401, 400]);
        assert_eq!(status.closed_this_tick, 2);
        let retained = manager.tick_at(true, &[], &[], start + Duration::from_secs(6), |_| Ok(()));
        assert!(retained.last_action.starts_with("Deep-closed"));

        manager.tick_at(
            true,
            &[],
            &[app(700, None, false)],
            start + Duration::from_secs(7),
            |_| Ok(()),
        );
        manager.tick_at(true, &[], &[], start + Duration::from_secs(8), |_| Ok(()));
        let after_clean_exit =
            manager.tick_at(true, &[], &[], start + Duration::from_secs(12), |_| Ok(()));
        assert!(after_clean_exit.last_action.starts_with("Deep-closed"));
    }

    #[test]
    fn reopening_a_window_cancels_deep_close() {
        let start = Instant::now();
        let mut manager = DeepCloseManager::default();
        let mut terminated = Vec::new();
        manager.tick_at(true, &[], &[app(500, None, false)], start, |_| Ok(()));
        manager.tick_at(
            true,
            &[],
            &[app(500, None, true)],
            start + Duration::from_secs(1),
            |_| Ok(()),
        );
        manager.tick_at(
            true,
            &[],
            &[app(500, None, false)],
            start + Duration::from_secs(2),
            |pid| {
                terminated.push(pid);
                Ok(())
            },
        );
        manager.tick_at(
            true,
            &[],
            &[app(500, None, false)],
            start + Duration::from_secs(8),
            |pid| {
                terminated.push(pid);
                Ok(())
            },
        );
        assert!(terminated.is_empty());
    }

    #[test]
    fn developer_workloads_are_never_tracked() {
        let mut process = app(600, None, false);
        process.name = "studio64.exe".into();
        let mut manager = DeepCloseManager::default();
        let status = manager.tick_at(true, &[], &[process], Instant::now(), |_| Ok(()));
        assert_eq!(status.tracked_apps, 0);
    }
}
