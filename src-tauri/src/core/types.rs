use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskMetric {
    pub name: String,
    pub mount: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f32,
    pub removable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HardwareMetrics {
    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub storage_temp_c: Option<f32>,
    pub storage_name: Option<String>,
    pub storage_sensor_name: Option<String>,
    pub gpu_usage_percent: Option<f32>,
    pub gpu_vram_used_bytes: Option<u64>,
    pub cpu_package_power_w: Option<f32>,
    pub gpu_power_w: Option<f32>,
    pub fan_rpm: Option<f32>,
    pub cpu_clock_mhz: Option<f32>,
    pub gpu_clock_mhz: Option<f32>,
    pub elevated: bool,
    pub source: String,
    pub status: String,
}

impl HardwareMetrics {
    pub fn unavailable(status: impl Into<String>) -> Self {
        Self {
            cpu_temp_c: None,
            gpu_temp_c: None,
            storage_temp_c: None,
            storage_name: None,
            storage_sensor_name: None,
            gpu_usage_percent: None,
            gpu_vram_used_bytes: None,
            cpu_package_power_w: None,
            gpu_power_w: None,
            fan_rpm: None,
            cpu_clock_mhz: None,
            gpu_clock_mhz: None,
            elevated: false,
            source: "Windows standard APIs".into(),
            status: status.into(),
        }
    }
}

impl Default for HardwareMetrics {
    fn default() -> Self {
        Self::unavailable("Hardware sensors are unavailable")
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub action_label: Option<String>,
    pub target_view: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub captured_at_ms: i64,
    pub status: String,
    pub reasons: Vec<String>,
    pub cpu_percent: f32,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub memory_available_bytes: u64,
    pub memory_percent: f32,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub process_count: usize,
    pub heavy_process_count: usize,
    pub hung_process_count: usize,
    pub uptime_seconds: u64,
    pub disks: Vec<DiskMetric>,
    pub hardware: HardwareMetrics,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable: String,
    pub command: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub uptime_seconds: u64,
    pub status: String,
    pub responsive: bool,
    pub foreground: bool,
    pub background: bool,
    pub orphaned: bool,
    pub child_count: usize,
    pub protected: bool,
    pub protection_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSample {
    pub snapshot: SystemSnapshot,
    pub processes: Vec<ProcessInfo>,
    pub deep_close: DeepCloseStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepCloseStatus {
    pub enabled: bool,
    pub tracked_apps: usize,
    pub pending_apps: usize,
    pub closed_this_tick: usize,
    pub last_action: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizedProcess {
    pub pid: u32,
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerStatus {
    pub captured_at_ms: i64,
    pub enabled: bool,
    pub mode: String,
    pub pressure_streak: u32,
    pub tuned_processes: Vec<OptimizedProcess>,
    pub restored_this_tick: usize,
    pub skipped_this_tick: usize,
    pub trimmed_this_tick: usize,
    pub released_memory_bytes: u64,
    pub last_action: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationAction {
    pub pid: u32,
    pub name: String,
    pub action: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationResult {
    pub captured_at_ms: i64,
    pub examined: usize,
    pub tuned: usize,
    pub trimmed: usize,
    pub released_memory_bytes: u64,
    pub skipped: usize,
    pub actions: Vec<OptimizationAction>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessActionResult {
    pub pid: u32,
    pub action: String,
    pub affected_count: usize,
    pub elevated: bool,
    pub confirmed_exited: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceTarget {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub category: String,
    pub path: String,
    pub reclaimable_bytes: u64,
    pub item_count: usize,
    pub minimum_age_days: u64,
    pub automatic_eligible: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceReport {
    pub captured_at_ms: i64,
    pub total_reclaimable_bytes: u64,
    pub automatic_reclaimable_bytes: u64,
    pub targets: Vec<MaintenanceTarget>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCandidate {
    pub id: String,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub modified_at_ms: i64,
    pub category: String,
    pub series: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupGroup {
    pub id: String,
    pub title: String,
    pub category: String,
    pub summary: String,
    pub file_count: usize,
    pub total_size_bytes: u64,
    pub oldest_at_ms: i64,
    pub newest_at_ms: i64,
    pub files: Vec<FileCandidate>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub id: String,
    pub running: bool,
    pub complete: bool,
    pub cancelled: bool,
    pub scanned_files: u64,
    pub scanned_bytes: u64,
    pub current_path: String,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub error: Option<String>,
    pub groups: Vec<CleanupGroup>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPreview {
    pub requested: usize,
    pub eligible: usize,
    pub total_size_bytes: u64,
    pub rejected: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub moved_to_recycle_bin: usize,
    pub total_size_bytes: u64,
    pub failed: Vec<String>,
}
