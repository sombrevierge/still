use crate::core::safety::process_protection;
use crate::core::types::{ProcessActionResult, ProcessInfo};
use std::collections::{HashMap, HashSet};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use sysinfo::{Pid, System};
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsHungAppWindow, IsWindowVisible,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;

#[derive(Default)]
struct WindowInventory {
    visible: HashSet<u32>,
    hung: HashSet<u32>,
}

unsafe extern "system" fn enumerate_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let inventory = &mut *(lparam as *mut WindowInventory);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != 0 && IsWindowVisible(hwnd) != 0 {
        inventory.visible.insert(pid);
        if IsHungAppWindow(hwnd) != 0 {
            inventory.hung.insert(pid);
        }
    }
    1
}

fn window_inventory() -> (WindowInventory, Option<u32>) {
    let mut inventory = WindowInventory::default();
    let foreground = unsafe {
        let _ = EnumWindows(
            Some(enumerate_window),
            &mut inventory as *mut WindowInventory as LPARAM,
        );
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            None
        } else {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            (pid != 0).then_some(pid)
        }
    };
    (inventory, foreground)
}

pub fn process_inventory() -> Vec<ProcessInfo> {
    let mut system = System::new_all();
    system.refresh_all();
    process_inventory_from_system(&system)
}

pub fn process_inventory_from_system(system: &System) -> Vec<ProcessInfo> {
    let (windows, foreground_pid) = window_inventory();
    let all_pids: HashSet<u32> = system.processes().keys().map(|pid| pid.as_u32()).collect();
    let mut children: HashMap<u32, usize> = HashMap::new();
    for process in system.processes().values() {
        if let Some(parent) = process.parent() {
            *children.entry(parent.as_u32()).or_default() += 1;
        }
    }

    let mut items: Vec<ProcessInfo> = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let pid = pid.as_u32();
            let parent_pid = process.parent().map(|value| value.as_u32());
            let name = process.name().to_string();
            let executable = process
                .exe()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default();
            let protection_reason = process_protection(pid, &name, &executable);
            let has_window = windows.visible.contains(&pid);
            let command = process.cmd().join(" ");
            ProcessInfo {
                pid,
                parent_pid,
                name,
                executable,
                command,
                cpu_percent: process.cpu_usage(),
                memory_bytes: process.memory(),
                uptime_seconds: process.run_time(),
                status: format!("{:?}", process.status()),
                responsive: !windows.hung.contains(&pid),
                foreground: foreground_pid == Some(pid),
                background: !has_window,
                orphaned: !has_window
                    && parent_pid.is_some_and(|parent| !all_pids.contains(&parent)),
                child_count: children.get(&pid).copied().unwrap_or_default(),
                protected: protection_reason.is_some(),
                protection_reason,
            }
        })
        .collect();

    items.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
    });
    items
}

fn process_details(pid: u32) -> Result<(String, String, Vec<String>), String> {
    let mut system = System::new_all();
    system.refresh_all();
    let process = system
        .process(Pid::from_u32(pid))
        .ok_or_else(|| "Process no longer exists".to_string())?;
    let name = process.name().to_string();
    let executable = process
        .exe()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(reason) = process_protection(pid, &name, &executable) {
        return Err(reason);
    }
    Ok((name, executable, process.cmd().to_vec()))
}

fn terminate_unchecked(pid: u32) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return Err("Windows denied access to this process".into());
        }
        let result = TerminateProcess(handle, 1);
        CloseHandle(handle);
        if result == 0 {
            Err("Windows could not terminate the process".into())
        } else {
            Ok(())
        }
    }
}

fn wait_for_exit(pid: u32, timeout_ms: u32) -> bool {
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE_ACCESS, 0, pid);
        if handle.is_null() {
            return !process_inventory().iter().any(|process| process.pid == pid);
        }
        let result = WaitForSingleObject(handle, timeout_ms);
        CloseHandle(handle);
        result == WAIT_OBJECT_0
    }
}

fn taskkill(pid: u32, tree: bool, elevated: bool) -> Result<(), String> {
    let mut arguments = vec!["/PID".to_string(), pid.to_string()];
    if tree {
        arguments.push("/T".into());
    }
    arguments.push("/F".into());

    let status = if elevated {
        let quoted_arguments = arguments
            .iter()
            .map(|argument| format!("'{}'", argument.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        // Every interpolated value above is either a validated u32 or a fixed flag.
        let script = format!(
            "$p = Start-Process -FilePath \"$env:WINDIR\\System32\\taskkill.exe\" \
             -ArgumentList @({quoted_arguments}) -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
        );
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    } else {
        Command::new("taskkill.exe")
            .args(&arguments)
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    }
    .map_err(|error| format!("Could not launch Windows process termination: {error}"))?;

    if status.success() {
        Ok(())
    } else if elevated {
        Err("Administrator approval was declined or Windows refused termination".into())
    } else {
        Err("Windows taskkill could not terminate this process".into())
    }
}

fn force_terminate(pid: u32, tree: bool) -> Result<bool, String> {
    if !tree && terminate_unchecked(pid).is_ok() && wait_for_exit(pid, 1_500) {
        return Ok(false);
    }
    if taskkill(pid, tree, false).is_ok() && wait_for_exit(pid, 2_000) {
        return Ok(false);
    }
    taskkill(pid, tree, true)?;
    if wait_for_exit(pid, 4_000) {
        Ok(true)
    } else {
        Err("Windows reported success, but the process is still running".into())
    }
}

pub(crate) fn terminate_automatic(pid: u32) -> Result<(), String> {
    process_details(pid)?;
    if terminate_unchecked(pid).is_ok() && wait_for_exit(pid, 750) {
        return Ok(());
    }
    taskkill(pid, false, false)?;
    if wait_for_exit(pid, 1_500) {
        Ok(())
    } else {
        Err("Windows did not confirm that the leftover process exited".into())
    }
}

pub fn terminate(pid: u32) -> Result<ProcessActionResult, String> {
    let (name, _, _) = process_details(pid)?;
    let elevated = force_terminate(pid, false)?;
    Ok(ProcessActionResult {
        pid,
        action: format!("Terminated {name}"),
        affected_count: 1,
        elevated,
        confirmed_exited: true,
    })
}

pub fn terminate_tree(pid: u32) -> Result<ProcessActionResult, String> {
    let mut system = System::new_all();
    system.refresh_all();
    let mut descendants = Vec::<u32>::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for (child_pid, process) in system.processes() {
            if process.parent().map(|value| value.as_u32()) == Some(parent) {
                let child = child_pid.as_u32();
                descendants.push(child);
                frontier.push(child);
            }
        }
    }
    let mut ordered = descendants;
    ordered.reverse();
    ordered.push(pid);

    for candidate in &ordered {
        let process = system
            .process(Pid::from_u32(*candidate))
            .ok_or_else(|| format!("Process {candidate} disappeared during validation"))?;
        let executable = process
            .exe()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(reason) = process_protection(*candidate, process.name(), &executable) {
            return Err(format!(
                "Tree contains protected process {candidate}: {reason}"
            ));
        }
    }

    let affected_count = ordered.len();
    let elevated = force_terminate(pid, true)?;
    Ok(ProcessActionResult {
        pid,
        action: "Terminated process tree".into(),
        affected_count,
        elevated,
        confirmed_exited: true,
    })
}

pub fn restart(pid: u32) -> Result<(), String> {
    let (_, executable, command_line) = process_details(pid)?;
    if executable.is_empty() {
        return Err("The executable path is unavailable".into());
    }
    force_terminate(pid, false)?;
    let mut command = Command::new(&executable);
    if command_line.len() > 1 {
        command.args(command_line.iter().skip(1));
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not restart application: {error}"))
}

pub fn open_location(pid: u32) -> Result<(), String> {
    let (_, executable, _) = process_details(pid)?;
    if executable.is_empty() {
        return Err("The executable path is unavailable".into());
    }
    Command::new("explorer.exe")
        .arg("/select,")
        .arg(&executable)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open File Explorer: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn live_inventory_contains_the_test_process() {
        let inventory = process_inventory();
        assert!(!inventory.is_empty());
        assert!(inventory
            .iter()
            .any(|process| process.pid == std::process::id()));
        let current = inventory
            .iter()
            .find(|process| process.pid == std::process::id())
            .unwrap();
        assert!(
            current.protected,
            "Still/test host must protect its own PID"
        );
    }

    #[test]
    fn termination_is_confirmed_for_a_real_process() {
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("test worker should start");
        let pid = child.id();
        thread::sleep(Duration::from_millis(200));

        let result = terminate(pid).expect("test worker should terminate");
        assert_eq!(result.pid, pid);
        assert!(result.confirmed_exited);
        assert!(child.wait().is_ok());
    }
}
