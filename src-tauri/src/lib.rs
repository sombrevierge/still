mod core;

use crate::core::cleanup;
use crate::core::deep_close::DeepCloseManager;
use crate::core::disk_analysis::ScanManager;
use crate::core::hardware::HardwareBridge;
use crate::core::maintenance;
use crate::core::optimizer::AdaptiveOptimizer;
use crate::core::processes;
use crate::core::telemetry::TelemetryCollector;
use crate::core::types::{
    CleanupPreview, CleanupResult, MaintenanceReport, MonitorSample, OptimizationResult,
    OptimizerStatus, ProcessActionResult, ProcessInfo, ScanStatus,
};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::path::BaseDirectory;
use tauri::{Manager, State};

pub struct AppState {
    telemetry: Mutex<TelemetryCollector>,
    optimizer: Mutex<AdaptiveOptimizer>,
    hardware: Mutex<HardwareBridge>,
    deep_close: Mutex<DeepCloseManager>,
    scans: ScanManager,
}

impl AppState {
    fn new(helper_path: Option<PathBuf>) -> Self {
        Self {
            telemetry: Mutex::new(TelemetryCollector::new()),
            optimizer: Mutex::new(AdaptiveOptimizer::default()),
            hardware: Mutex::new(HardwareBridge::new(helper_path)),
            deep_close: Mutex::new(DeepCloseManager::default()),
            scans: ScanManager::default(),
        }
    }
}

#[tauri::command]
fn get_monitor_sample(
    deep_close_enabled: bool,
    deep_close_exclusions: Vec<String>,
    state: State<'_, AppState>,
) -> Result<MonitorSample, String> {
    let hardware = state
        .hardware
        .lock()
        .map_err(|_| "Hardware sensor state is unavailable".to_string())?
        .read();
    let mut telemetry = state
        .telemetry
        .lock()
        .map_err(|_| "Telemetry state is unavailable".to_string())?;
    let mut sample = telemetry.sample(hardware);
    drop(telemetry);
    sample.deep_close = state
        .deep_close
        .lock()
        .map_err(|_| "Deep close state is unavailable".to_string())?
        .tick(
            deep_close_enabled,
            &deep_close_exclusions,
            &sample.processes,
        );
    Ok(sample)
}

#[tauri::command]
fn enable_elevated_hardware_sensors(state: State<'_, AppState>) -> Result<String, String> {
    state
        .hardware
        .lock()
        .map_err(|_| "Hardware sensor state is unavailable".to_string())?
        .enable_elevated()
}

#[tauri::command]
fn get_processes() -> Vec<ProcessInfo> {
    processes::process_inventory()
}

#[tauri::command]
fn optimizer_tick(
    enabled: bool,
    under_pressure: bool,
    candidate_pids: Vec<u32>,
    excluded_names: Vec<String>,
    trim_memory: bool,
    aggressive: bool,
    state: State<'_, AppState>,
) -> Result<OptimizerStatus, String> {
    let mut optimizer = state
        .optimizer
        .lock()
        .map_err(|_| "Adaptive optimizer state is unavailable".to_string())?;
    Ok(optimizer.tick(
        enabled,
        under_pressure,
        candidate_pids,
        excluded_names,
        trim_memory,
        aggressive,
    ))
}

#[tauri::command]
fn optimize_now(
    excluded_names: Vec<String>,
    aggressive: bool,
    state: State<'_, AppState>,
) -> Result<OptimizationResult, String> {
    let mut optimizer = state
        .optimizer
        .lock()
        .map_err(|_| "Adaptive optimizer state is unavailable".to_string())?;
    Ok(optimizer.optimize_now(excluded_names, aggressive))
}

#[tauri::command]
fn terminate_process(pid: u32) -> Result<ProcessActionResult, String> {
    processes::terminate(pid)
}

#[tauri::command]
fn terminate_process_tree(pid: u32) -> Result<ProcessActionResult, String> {
    processes::terminate_tree(pid)
}

#[tauri::command]
fn restart_process(pid: u32) -> Result<(), String> {
    processes::restart(pid)
}

#[tauri::command]
fn open_process_location(pid: u32) -> Result<(), String> {
    processes::open_location(pid)
}

#[tauri::command]
fn default_scan_roots() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    ["Downloads", "Desktop", "Documents"]
        .iter()
        .map(|name| home.join(name))
        .filter(|path| path.is_dir())
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

#[tauri::command]
fn start_storage_scan(
    roots: Vec<String>,
    exclusions: Vec<String>,
    minimum_size_mb: u64,
    old_days: u64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .scans
        .start(roots, exclusions, minimum_size_mb, old_days)
}

#[tauri::command]
fn storage_scan_status(state: State<'_, AppState>) -> Option<ScanStatus> {
    state.scans.status()
}

#[tauri::command]
fn cancel_storage_scan(state: State<'_, AppState>) -> bool {
    state.scans.cancel()
}

#[tauri::command]
fn dry_run_cleanup(paths: Vec<String>) -> CleanupPreview {
    cleanup::dry_run(paths)
}

#[tauri::command]
fn recycle_selected(paths: Vec<String>) -> CleanupResult {
    cleanup::move_to_recycle_bin(paths)
}

#[tauri::command]
async fn maintenance_report(include_browser_caches: bool) -> Result<MaintenanceReport, String> {
    tauri::async_runtime::spawn_blocking(move || maintenance::analyze(include_browser_caches))
        .await
        .map_err(|error| format!("Maintenance analysis stopped unexpectedly: {error}"))
}

#[tauri::command]
async fn clean_maintenance(
    target_ids: Vec<String>,
    include_browser_caches: bool,
) -> Result<CleanupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        maintenance::clean_selected(target_ids, include_browser_caches)
    })
    .await
    .map_err(|error| format!("Maintenance cleanup stopped unexpectedly: {error}"))
}

#[tauri::command]
async fn automatic_maintenance(minimum_age_days: u64) -> Result<CleanupResult, String> {
    tauri::async_runtime::spawn_blocking(move || maintenance::clean_automatic(minimum_age_days))
        .await
        .map_err(|error| format!("Automatic maintenance stopped unexpectedly: {error}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let installed_sibling = std::env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(PathBuf::from))
                .map(|directory| {
                    directory
                        .join("sensor-helper")
                        .join("still-hardware-sensors.exe")
                });
            let bundled = app
                .path()
                .resolve(
                    "sensor-helper/still-hardware-sensors.exe",
                    BaseDirectory::Resource,
                )
                .ok();
            #[cfg(debug_assertions)]
            let helper = installed_sibling
                .filter(|path| path.is_file())
                .or_else(|| bundled.filter(|path| path.is_file()))
                .or_else(|| {
                    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("..")
                        .join("sensor-helper")
                        .join("publish")
                        .join("still-hardware-sensors.exe");
                    development.is_file().then_some(development)
                });
            #[cfg(not(debug_assertions))]
            let helper = installed_sibling
                .filter(|path| path.is_file())
                .or_else(|| bundled.filter(|path| path.is_file()));
            app.manage(AppState::new(helper));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_monitor_sample,
            enable_elevated_hardware_sensors,
            get_processes,
            optimizer_tick,
            optimize_now,
            terminate_process,
            terminate_process_tree,
            restart_process,
            open_process_location,
            default_scan_roots,
            start_storage_scan,
            storage_scan_status,
            cancel_storage_scan,
            dry_run_cleanup,
            recycle_selected,
            maintenance_report,
            clean_maintenance,
            automatic_maintenance,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Still");
}
