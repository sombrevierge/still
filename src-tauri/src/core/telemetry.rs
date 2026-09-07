use crate::core::types::{
    DeepCloseStatus, DiskMetric, HardwareMetrics, MonitorSample, ProcessInfo, SystemSnapshot,
};
use crate::core::{processes, recommendations};
use chrono::Utc;
use std::time::Instant;
use sysinfo::{Disks, System};

pub struct TelemetryCollector {
    system: System,
    disks: Disks,
    last_refresh: Instant,
    last_disk_list_refresh: Instant,
}

impl TelemetryCollector {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            last_refresh: Instant::now(),
            last_disk_list_refresh: Instant::now(),
        }
    }

    pub fn sample(&mut self, hardware: HardwareMetrics) -> MonitorSample {
        let elapsed = self.last_refresh.elapsed().as_secs_f64().max(0.25);
        self.system.refresh_all();
        if self.last_disk_list_refresh.elapsed().as_secs() >= 30 {
            self.disks.refresh_list();
            self.last_disk_list_refresh = Instant::now();
        }
        self.disks.refresh();
        self.last_refresh = Instant::now();
        let processes = processes::process_inventory_from_system(&self.system);
        let snapshot = self.snapshot_from_current(&processes, elapsed, hardware);
        MonitorSample {
            snapshot,
            processes,
            deep_close: DeepCloseStatus {
                enabled: false,
                tracked_apps: 0,
                pending_apps: 0,
                closed_this_tick: 0,
                last_action: "Deep close has not been configured yet".into(),
            },
        }
    }

    fn snapshot_from_current(
        &self,
        processes: &[ProcessInfo],
        elapsed: f64,
        hardware: HardwareMetrics,
    ) -> SystemSnapshot {
        let cpu_percent = self.system.global_cpu_info().cpu_usage().clamp(0.0, 100.0);
        let memory_total_bytes = self.system.total_memory();
        let memory_used_bytes = self.system.used_memory();
        let memory_available_bytes = memory_total_bytes.saturating_sub(memory_used_bytes);
        let memory_percent = if memory_total_bytes == 0 {
            0.0
        } else {
            memory_used_bytes as f32 * 100.0 / memory_total_bytes as f32
        };
        let swap_total_bytes = self.system.total_swap();
        let swap_used_bytes = self.system.used_swap();

        let (disk_read, disk_write) =
            self.system
                .processes()
                .values()
                .fold((0u64, 0u64), |(read, write), process| {
                    let usage = process.disk_usage();
                    (
                        read.saturating_add(usage.read_bytes),
                        write.saturating_add(usage.written_bytes),
                    )
                });

        let disks: Vec<DiskMetric> = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                let used_percent = if total == 0 {
                    0.0
                } else {
                    (total.saturating_sub(available)) as f32 * 100.0 / total as f32
                };
                DiskMetric {
                    name: disk.name().to_string_lossy().to_string(),
                    mount: disk.mount_point().to_string_lossy().to_string(),
                    total_bytes: total,
                    available_bytes: available,
                    used_percent,
                    removable: disk.is_removable(),
                }
            })
            .collect();

        let hung_process_count = processes
            .iter()
            .filter(|process| !process.responsive)
            .count();
        let heavy_process_count = processes
            .iter()
            .filter(|process| {
                process.cpu_percent >= 20.0
                    || (memory_total_bytes > 0
                        && process.memory_bytes as f64 / memory_total_bytes as f64 >= 0.08)
            })
            .count();

        let mut reasons = Vec::new();
        let mut pressure_points = 0;
        let mut busy_points = 0;

        if memory_percent >= 90.0 || memory_available_bytes < 900_000_000 {
            pressure_points += 1;
            reasons.push(format!(
                "RAM is {:.0}% used; only {:.1} GB is available",
                memory_percent,
                memory_available_bytes as f64 / 1_073_741_824.0
            ));
        } else if memory_percent >= 78.0 {
            busy_points += 1;
            reasons.push(format!("RAM usage is elevated at {:.0}%", memory_percent));
        }
        if cpu_percent >= 88.0 {
            pressure_points += 1;
            reasons.push(format!("CPU is saturated at {:.0}%", cpu_percent));
        } else if cpu_percent >= 65.0 {
            busy_points += 1;
            reasons.push(format!("CPU is busy at {:.0}%", cpu_percent));
        }
        if hung_process_count > 0 {
            pressure_points += 1;
            reasons.push(format!(
                "{hung_process_count} app windows are not responding"
            ));
        }
        if let Some(temperature) = hardware.cpu_temp_c {
            if temperature >= 95.0 {
                pressure_points += 1;
                reasons.push(format!("CPU temperature is critical at {temperature:.0}°C"));
            } else if temperature >= 85.0 {
                busy_points += 1;
                reasons.push(format!("CPU temperature is elevated at {temperature:.0}°C"));
            }
        }
        if let Some(temperature) = hardware.gpu_temp_c {
            if temperature >= 90.0 {
                pressure_points += 1;
                reasons.push(format!("GPU temperature is critical at {temperature:.0}°C"));
            } else if temperature >= 80.0 {
                busy_points += 1;
                reasons.push(format!("GPU temperature is elevated at {temperature:.0}°C"));
            }
        }
        if let Some(temperature) = hardware.storage_temp_c {
            if temperature >= 70.0 {
                pressure_points += 1;
                reasons.push(format!(
                    "Storage temperature is critical at {temperature:.0}°C"
                ));
            } else if temperature >= 60.0 {
                busy_points += 1;
                reasons.push(format!(
                    "Storage temperature is elevated at {temperature:.0}°C"
                ));
            }
        }
        for disk in disks.iter().filter(|disk| !disk.removable) {
            let free_percent = 100.0 - disk.used_percent;
            if free_percent < 5.0 {
                pressure_points += 1;
                reasons.push(format!("{} has less than 5% free space", disk.mount));
            } else if free_percent < 10.0 {
                busy_points += 1;
                reasons.push(format!("{} has only {:.0}% free", disk.mount, free_percent));
            }
        }
        if reasons.is_empty() {
            reasons.push("Resources have comfortable headroom".into());
        }
        let status = if pressure_points > 0 {
            "pressure"
        } else if busy_points > 0 {
            "busy"
        } else {
            "calm"
        }
        .to_string();

        let mut snapshot = SystemSnapshot {
            captured_at_ms: Utc::now().timestamp_millis(),
            status,
            reasons,
            cpu_percent,
            memory_total_bytes,
            memory_used_bytes,
            memory_available_bytes,
            memory_percent,
            swap_total_bytes,
            swap_used_bytes,
            disk_read_bytes_per_sec: (disk_read as f64 / elapsed) as u64,
            disk_write_bytes_per_sec: (disk_write as f64 / elapsed) as u64,
            process_count: processes.len(),
            heavy_process_count,
            hung_process_count,
            uptime_seconds: System::uptime(),
            disks,
            hardware,
            recommendations: Vec::new(),
        };
        snapshot.recommendations = recommendations::build(&snapshot, processes);
        snapshot
    }
}

impl Default for TelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_snapshot_reports_real_system_resources() {
        let sample = TelemetryCollector::new().sample(HardwareMetrics::unavailable("test"));
        let snapshot = sample.snapshot;
        assert!(snapshot.memory_total_bytes > 0);
        assert!(snapshot.process_count > 0);
        assert!(!snapshot.disks.is_empty());
        assert!((0.0..=100.0).contains(&snapshot.cpu_percent));
        assert!(["calm", "busy", "pressure"].contains(&snapshot.status.as_str()));
    }
}
