import { invoke } from "@tauri-apps/api/core";
import type {
  CleanupPreview,
  CleanupResult,
  MaintenanceReport,
  MonitorSample,
  OptimizationResult,
  OptimizerStatus,
  ProcessActionResult,
  ProcessInfo,
  ScanStatus,
  SystemSnapshot,
} from "../types";

export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const mockProcesses: ProcessInfo[] = [
  {
    pid: 4108,
    parentPid: 900,
    name: "chrome.exe",
    executable: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    command: "chrome.exe",
    cpuPercent: 18.2,
    memoryBytes: 2_481_231_872,
    uptimeSeconds: 14_420,
    status: "Run",
    responsive: true,
    foreground: true,
    background: false,
    orphaned: false,
    childCount: 12,
    protected: false,
    protectionReason: null,
  },
  {
    pid: 7732,
    parentPid: 4108,
    name: "Code.exe",
    executable: "C:\\Users\\Catherine\\AppData\\Local\\Programs\\Code\\Code.exe",
    command: "Code.exe --unity-launch",
    cpuPercent: 8.4,
    memoryBytes: 1_389_838_336,
    uptimeSeconds: 8_200,
    status: "Run",
    responsive: true,
    foreground: false,
    background: false,
    orphaned: false,
    childCount: 6,
    protected: false,
    protectionReason: null,
  },
  {
    pid: 4,
    parentPid: null,
    name: "System",
    executable: "",
    command: "",
    cpuPercent: 2.1,
    memoryBytes: 211_812_352,
    uptimeSeconds: 96_200,
    status: "Run",
    responsive: true,
    foreground: false,
    background: true,
    orphaned: false,
    childCount: 2,
    protected: true,
    protectionReason: "Windows kernel process",
  },
];

const mockSnapshot: SystemSnapshot = {
  capturedAtMs: Date.now(),
  status: "busy",
  reasons: ["RAM usage is elevated at 82%", "C:\\ has only 9% free"],
  cpuPercent: 42,
  memoryTotalBytes: 8_589_934_592,
  memoryUsedBytes: 7_043_940_352,
  memoryAvailableBytes: 1_545_994_240,
  memoryPercent: 82,
  swapTotalBytes: 22_552_707_072,
  swapUsedBytes: 7_012_532_224,
  diskReadBytesPerSec: 18_420_000,
  diskWriteBytesPerSec: 8_230_000,
  processCount: 184,
  heavyProcessCount: 3,
  hungProcessCount: 0,
  uptimeSeconds: 96_200,
  disks: [
    {
      name: "NVMe SSD",
      mount: "C:\\",
      totalBytes: 409_450_000_000,
      availableBytes: 36_380_000_000,
      usedPercent: 91.1,
      removable: false,
    },
  ],
  hardware: {
    cpuTempC: null,
    gpuTempC: 54,
    storageTempC: 41,
    storageName: "NVMe SSD",
    storageSensorName: "Composite temperature",
    gpuUsagePercent: null,
    gpuVramUsedBytes: null,
    cpuPackagePowerW: null,
    gpuPowerW: null,
    fanRpm: null,
    cpuClockMhz: null,
    gpuClockMhz: 800,
    elevated: false,
    source: "LibreHardwareMonitor 0.9.6",
    status: "Hardware sensor provider is starting",
  },
  recommendations: [
    {
      id: "memory-pressure",
      kind: "pressure",
      title: "Memory pressure",
      detail: "Only 1.4 GB is available. Largest users: chrome.exe 2.3 GB, Code.exe 1.3 GB.",
      actionLabel: "Inspect processes",
      targetView: "processes",
    },
    {
      id: "disk-space",
      kind: "attention",
      title: "C:\\ is running low",
      detail: "33.9 GB is free. Scan downloads and old bundles before Windows needs the space.",
      actionLabel: "Scan storage",
      targetView: "storage",
    },
  ],
};

const mockOptimizer: OptimizerStatus = {
  capturedAtMs: Date.now(),
  enabled: true,
  mode: "monitoring",
  pressureStreak: 0,
  tunedProcesses: [],
  restoredThisTick: 0,
  skippedThisTick: 0,
  trimmedThisTick: 0,
  releasedMemoryBytes: 0,
  lastAction: "Monitoring continuously; no sustained pressure detected",
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export const native = {
  sample: async (
    deepCloseEnabled: boolean,
    deepCloseExclusions: string[],
  ): Promise<MonitorSample> =>
    isDesktop
      ? call<MonitorSample>("get_monitor_sample", { deepCloseEnabled, deepCloseExclusions })
      : {
          snapshot: mockSnapshot,
          processes: mockProcesses,
          deepClose: {
            enabled: deepCloseEnabled,
            trackedApps: 2,
            pendingApps: 0,
            closedThisTick: 0,
            lastAction: "Watching 2 windowed applications; no leftovers detected",
          },
        },
  enableElevatedSensors: () => call<string>("enable_elevated_hardware_sensors"),
  processes: async () => (isDesktop ? call<ProcessInfo[]>("get_processes") : mockProcesses),
  optimize: async (
    enabled: boolean,
    underPressure: boolean,
    candidatePids: number[],
    excludedNames: string[],
    trimMemory: boolean,
    aggressive: boolean,
  ): Promise<OptimizerStatus> =>
    isDesktop
      ? call<OptimizerStatus>("optimizer_tick", {
          enabled,
          underPressure,
          candidatePids,
          excludedNames,
          trimMemory,
          aggressive,
        })
      : { ...mockOptimizer, enabled, mode: enabled ? "monitoring" : "off" },
  optimizeNow: (excludedNames: string[], aggressive: boolean) =>
    call<OptimizationResult>("optimize_now", { excludedNames, aggressive }),
  terminate: (pid: number) => call<ProcessActionResult>("terminate_process", { pid }),
  terminateTree: (pid: number) => call<ProcessActionResult>("terminate_process_tree", { pid }),
  restart: (pid: number) => call<void>("restart_process", { pid }),
  openLocation: (pid: number) => call<void>("open_process_location", { pid }),
  defaultRoots: async () =>
    isDesktop ? call<string[]>("default_scan_roots") : ["C:\\Users\\Catherine\\Downloads"],
  startScan: (roots: string[], exclusions: string[], minimumSizeMb: number, oldDays: number) =>
    call<string>("start_storage_scan", { roots, exclusions, minimumSizeMb, oldDays }),
  scanStatus: () => call<ScanStatus | null>("storage_scan_status"),
  cancelScan: () => call<boolean>("cancel_storage_scan"),
  dryRun: (paths: string[]) => call<CleanupPreview>("dry_run_cleanup", { paths }),
  recycle: (paths: string[]) => call<CleanupResult>("recycle_selected", { paths }),
  maintenanceReport: (includeBrowserCaches: boolean) =>
    call<MaintenanceReport>("maintenance_report", { includeBrowserCaches }),
  cleanMaintenance: (targetIds: string[], includeBrowserCaches: boolean) =>
    call<CleanupResult>("clean_maintenance", { targetIds, includeBrowserCaches }),
  automaticMaintenance: (minimumAgeDays: number) =>
    call<CleanupResult>("automatic_maintenance", { minimumAgeDays }),
};
