use crate::core::types::HardwareMetrics;
use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SW_HIDE};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_READING_AGE: Duration = Duration::from_secs(20);
const ELEVATED_START_TIMEOUT: Duration = Duration::from_secs(15);

pub struct HardwareBridge {
    helper_path: Option<PathBuf>,
    cache_path: PathBuf,
    child: Option<Child>,
    elevated_requested: bool,
    elevated_requested_at: Option<Instant>,
    allow_rediscovery: bool,
}

impl HardwareBridge {
    pub fn new(helper_path: Option<PathBuf>) -> Self {
        let allow_rediscovery = helper_path.is_none();
        let cache_path = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Still")
            .join("hardware-sensors.json");
        // Never surface readings left by a previous app or privilege level.
        let _ = fs::remove_file(&cache_path);
        let mut bridge = Self {
            helper_path: helper_path.filter(|path| path.is_file()),
            cache_path,
            child: None,
            elevated_requested: false,
            elevated_requested_at: None,
            allow_rediscovery,
        };
        let _ = bridge.start_standard();
        bridge
    }

    pub fn read(&mut self) -> HardwareMetrics {
        self.rediscover_helper();
        if self.helper_path.is_none() {
            return self.waiting_status();
        }
        if !self.elevated_requested {
            let needs_restart = self
                .child
                .as_mut()
                .is_none_or(|child| child.try_wait().ok().flatten().is_some());
            if needs_restart {
                let _ = self.start_standard();
            }
        }

        let Ok(metadata) = fs::metadata(&self.cache_path) else {
            if self.elevated_start_timed_out() {
                return self.restore_standard_after_elevation_failure();
            }
            return self.waiting_status();
        };
        let fresh = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age <= MAX_READING_AGE);
        if !fresh {
            if self.elevated_start_timed_out() {
                return self.restore_standard_after_elevation_failure();
            }
            return HardwareMetrics::unavailable("Hardware sensor reading is stale");
        }
        let Ok(contents) = fs::read_to_string(&self.cache_path) else {
            return HardwareMetrics::unavailable("Hardware sensor cache is temporarily locked");
        };
        match serde_json::from_str::<HardwareMetrics>(&contents) {
            Ok(mut metrics) => {
                metrics.cpu_temp_c = valid_temperature(metrics.cpu_temp_c);
                metrics.gpu_temp_c = valid_temperature(metrics.gpu_temp_c);
                metrics.storage_temp_c = valid_temperature(metrics.storage_temp_c);
                metrics.gpu_usage_percent = valid_percent(metrics.gpu_usage_percent);
                if metrics.elevated {
                    self.elevated_requested_at = None;
                }
                metrics
            }
            Err(_) => {
                HardwareMetrics::unavailable("Hardware sensor provider returned invalid data")
            }
        }
    }

    pub fn enable_elevated(&mut self) -> Result<String, String> {
        self.rediscover_helper();
        let helper = self
            .helper_path
            .clone()
            .ok_or_else(|| "Hardware sensor helper is not installed".to_string())?;
        self.stop_child();
        let _ = fs::remove_file(&self.cache_path);
        let parameters = format!(
            "--watch --parent {} --output \"{}\"",
            std::process::id(),
            self.cache_path.to_string_lossy()
        );
        // Shell elevation of a .NET apphost is sensitive to its working directory.
        // Keep the helper self-contained and resolve its directory explicitly so
        // Windows does not fail with SE_ERR_PNF before it can show the UAC prompt.
        let mut result = execute_elevated(&helper, &parameters, helper.parent());
        // Some Windows security configurations refuse `runas` for executables in
        // LocalAppData with SE_ERR_PNF even though the file exists. Elevating the
        // signed system command processor first avoids that shell-policy dead end;
        // the self-contained helper then inherits its administrator token.
        if matches!(result as isize, 2 | 3) {
            result = self.execute_elevated_via_command_processor(&helper);
        }
        if result as isize <= 32 {
            self.elevated_requested = false;
            self.elevated_requested_at = None;
            let _ = self.start_standard();
            return Err(elevation_error(result as isize));
        }
        self.elevated_requested = true;
        self.elevated_requested_at = Some(Instant::now());
        Ok("Enhanced sensor provider started; readings will refresh within a few seconds".into())
    }

    fn start_standard(&mut self) -> Result<(), String> {
        let helper = self
            .helper_path
            .as_ref()
            .ok_or_else(|| "Hardware sensor helper is not installed".to_string())?;
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not prepare sensor cache: {error}"))?;
        }
        let child = Command::new(helper)
            .args([
                "--watch",
                "--parent",
                &std::process::id().to_string(),
                "--output",
                &self.cache_path.to_string_lossy(),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not start hardware sensor helper: {error}"))?;
        self.child = Some(child);
        self.elevated_requested = false;
        self.elevated_requested_at = None;
        Ok(())
    }

    fn execute_elevated_via_command_processor(&self, helper: &std::path::Path) -> isize {
        let Some(system_root) = std::env::var_os("SystemRoot") else {
            return 3;
        };
        let command_processor = PathBuf::from(system_root).join("System32").join("cmd.exe");
        if !command_processor.is_file() {
            return 2;
        }
        let parameters =
            command_processor_parameters(helper, std::process::id(), self.cache_path.as_path());
        execute_elevated(&command_processor, &parameters, helper.parent())
    }

    fn rediscover_helper(&mut self) {
        if self.helper_path.as_ref().is_some_and(|path| path.is_file()) || !self.allow_rediscovery {
            return;
        }
        self.helper_path = std::env::current_exe()
            .ok()
            .and_then(|executable| executable.parent().map(PathBuf::from))
            .map(|directory| {
                directory
                    .join("sensor-helper")
                    .join("still-hardware-sensors.exe")
            })
            .filter(|path| path.is_file());
        if self.helper_path.is_some() && self.child.is_none() && !self.elevated_requested {
            let _ = self.start_standard();
        }
    }

    fn elevated_start_timed_out(&self) -> bool {
        self.elevated_requested
            && self
                .elevated_requested_at
                .is_some_and(|requested_at| requested_at.elapsed() >= ELEVATED_START_TIMEOUT)
    }

    fn restore_standard_after_elevation_failure(&mut self) -> HardwareMetrics {
        self.elevated_requested = false;
        self.elevated_requested_at = None;
        let _ = self.start_standard();
        HardwareMetrics::unavailable(
            "Enhanced sensor helper did not report; restoring standard access",
        )
    }

    fn waiting_status(&self) -> HardwareMetrics {
        HardwareMetrics::unavailable(if self.elevated_requested {
            "Waiting for enhanced hardware sensor access"
        } else if self.helper_path.is_some() {
            "Hardware sensor provider is starting"
        } else {
            "Hardware sensor helper is not installed"
        })
    }

    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for HardwareBridge {
    fn drop(&mut self) {
        self.stop_child();
    }
}

fn valid_temperature(value: Option<f32>) -> Option<f32> {
    value.filter(|value| (1.0..150.0).contains(value))
}

fn valid_percent(value: Option<f32>) -> Option<f32> {
    value.filter(|value| (0.0..=100.0).contains(value))
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

fn execute_elevated(
    executable: &std::path::Path,
    parameters: &str,
    working_directory: Option<&std::path::Path>,
) -> isize {
    let operation = wide("runas");
    let executable = wide(executable.as_os_str());
    let parameters = wide(parameters);
    let working_directory = working_directory.map(|directory| wide(directory.as_os_str()));
    unsafe {
        ShellExecuteW(
            GetForegroundWindow(),
            operation.as_ptr(),
            executable.as_ptr(),
            parameters.as_ptr(),
            working_directory
                .as_ref()
                .map_or(std::ptr::null(), |directory| directory.as_ptr()),
            SW_HIDE,
        ) as isize
    }
}

fn command_processor_parameters(
    helper: &std::path::Path,
    parent_pid: u32,
    output: &std::path::Path,
) -> String {
    format!(
        "/d /v:off /s /c \"\"{}\" --watch --parent {} --output \"{}\"\"",
        helper.to_string_lossy(),
        parent_pid,
        output.to_string_lossy()
    )
}

fn elevation_error(code: isize) -> String {
    match code {
        2 | 3 => {
            format!("Windows could not resolve the sensor helper path (ShellExecute code {code})")
        }
        5 => {
            "Administrator approval was cancelled or Windows denied elevation (ShellExecute code 5)"
                .into()
        }
        _ => {
            format!("Windows could not start the enhanced sensor helper (ShellExecute code {code})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_impossible_sensor_values() {
        assert_eq!(valid_temperature(Some(0.0)), None);
        assert_eq!(valid_temperature(Some(151.0)), None);
        assert_eq!(valid_temperature(Some(58.0)), Some(58.0));
        assert_eq!(valid_percent(Some(101.0)), None);
    }

    #[test]
    fn missing_helper_has_explainable_status() {
        let mut bridge =
            HardwareBridge::new(Some(std::path::Path::new("Z:\\missing-helper.exe").into()));
        assert!(bridge.read().status.contains("not installed"));
    }

    #[test]
    fn partial_helper_payload_uses_safe_defaults() {
        let metrics: HardwareMetrics = serde_json::from_str(
            r#"{"gpuUsagePercent":12.5,"source":"test","status":"available"}"#,
        )
        .unwrap();
        assert_eq!(metrics.gpu_usage_percent, Some(12.5));
        assert_eq!(metrics.cpu_temp_c, None);
        assert!(!metrics.elevated);
    }

    #[test]
    fn elevated_start_timeout_is_detected() {
        let mut bridge =
            HardwareBridge::new(Some(std::path::Path::new("Z:\\missing-helper.exe").into()));
        bridge.elevated_requested = true;
        bridge.elevated_requested_at = Some(Instant::now() - ELEVATED_START_TIMEOUT);
        assert!(bridge.elevated_start_timed_out());
    }

    #[test]
    fn elevation_errors_preserve_the_windows_failure_reason() {
        assert!(elevation_error(3).contains("resolve"));
        assert!(elevation_error(5).contains("cancelled"));
        assert!(elevation_error(31).contains("code 31"));
    }

    #[test]
    fn command_processor_fallback_quotes_all_paths() {
        let parameters = command_processor_parameters(
            std::path::Path::new("C:\\Program Files\\Still\\sensor.exe"),
            42,
            std::path::Path::new("C:\\Users\\Test User\\sensor cache.json"),
        );
        assert_eq!(
            parameters,
            "/d /v:off /s /c \"\"C:\\Program Files\\Still\\sensor.exe\" --watch --parent 42 --output \"C:\\Users\\Test User\\sensor cache.json\"\""
        );
    }
}
