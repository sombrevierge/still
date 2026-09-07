export type ViewId = "overview" | "processes" | "storage" | "settings";

export interface DiskMetric {
  name: string;
  mount: string;
  totalBytes: number;
  availableBytes: number;
  usedPercent: number;
  removable: boolean;
}

export interface HardwareMetrics {
  cpuTempC: number | null;
  gpuTempC: number | null;
  storageTempC: number | null;
  storageName: string | null;
  storageSensorName: string | null;
  gpuUsagePercent: number | null;
  gpuVramUsedBytes: number | null;
  cpuPackagePowerW: number | null;
  gpuPowerW: number | null;
  fanRpm: number | null;
  cpuClockMhz: number | null;
  gpuClockMhz: number | null;
  elevated: boolean;
  source: string;
  status: string;
}

export interface DeepCloseStatus {
  enabled: boolean;
  trackedApps: number;
  pendingApps: number;
  closedThisTick: number;
  lastAction: string;
}

export interface Recommendation {
  id: string;
  kind: "pressure" | "attention" | "info";
  title: string;
  detail: string;
  actionLabel: string | null;
  targetView: ViewId | null;
}

export interface SystemSnapshot {
  capturedAtMs: number;
  status: "calm" | "busy" | "pressure";
  reasons: string[];
  cpuPercent: number;
  memoryTotalBytes: number;
  memoryUsedBytes: number;
  memoryAvailableBytes: number;
  memoryPercent: number;
  swapTotalBytes: number;
  swapUsedBytes: number;
  diskReadBytesPerSec: number;
  diskWriteBytesPerSec: number;
  processCount: number;
  heavyProcessCount: number;
  hungProcessCount: number;
  uptimeSeconds: number;
  disks: DiskMetric[];
  hardware: HardwareMetrics;
  recommendations: Recommendation[];
}

export interface ProcessInfo {
  pid: number;
  parentPid: number | null;
  name: string;
  executable: string;
  command: string;
  cpuPercent: number;
  memoryBytes: number;
  uptimeSeconds: number;
  status: string;
  responsive: boolean;
  foreground: boolean;
  background: boolean;
  orphaned: boolean;
  childCount: number;
  protected: boolean;
  protectionReason: string | null;
}

export interface OptimizedProcess {
  pid: number;
  name: string;
  reason: string;
}

export interface OptimizerStatus {
  capturedAtMs: number;
  enabled: boolean;
  mode: "off" | "monitoring" | "observing" | "balancing";
  pressureStreak: number;
  tunedProcesses: OptimizedProcess[];
  restoredThisTick: number;
  skippedThisTick: number;
  trimmedThisTick: number;
  releasedMemoryBytes: number;
  lastAction: string;
}

export interface OptimizationAction {
  pid: number;
  name: string;
  action: string;
  detail: string;
}

export interface OptimizationResult {
  capturedAtMs: number;
  examined: number;
  tuned: number;
  trimmed: number;
  releasedMemoryBytes: number;
  skipped: number;
  actions: OptimizationAction[];
  errors: string[];
}

export interface ProcessActionResult {
  pid: number;
  action: string;
  affectedCount: number;
  elevated: boolean;
  confirmedExited: boolean;
}

export interface FileCandidate {
  id: string;
  path: string;
  name: string;
  sizeBytes: number;
  modifiedAtMs: number;
  category: string;
  series: string;
  reason: string;
}

export interface CleanupGroup {
  id: string;
  title: string;
  category: string;
  summary: string;
  fileCount: number;
  totalSizeBytes: number;
  oldestAtMs: number;
  newestAtMs: number;
  files: FileCandidate[];
}

export interface ScanStatus {
  id: string;
  running: boolean;
  complete: boolean;
  cancelled: boolean;
  scannedFiles: number;
  scannedBytes: number;
  currentPath: string;
  startedAtMs: number;
  finishedAtMs: number | null;
  error: string | null;
  groups: CleanupGroup[];
}

export interface CleanupPreview {
  requested: number;
  eligible: number;
  totalSizeBytes: number;
  rejected: string[];
}

export interface CleanupResult {
  movedToRecycleBin: number;
  totalSizeBytes: number;
  failed: string[];
}

export interface MaintenanceTarget {
  id: string;
  title: string;
  detail: string;
  category: string;
  path: string;
  reclaimableBytes: number;
  itemCount: number;
  minimumAgeDays: number;
  automaticEligible: boolean;
}

export interface MaintenanceReport {
  capturedAtMs: number;
  totalReclaimableBytes: number;
  automaticReclaimableBytes: number;
  targets: MaintenanceTarget[];
}

export interface AppSettings {
  exclusions: string[];
  expectedBackground: string[];
  refreshSeconds: number;
  minimumSizeMb: number;
  oldDays: number;
  autopilotEnabled: boolean;
  autopilotCpuThreshold: number;
  autopilotStorageAudit: boolean;
  pauseScanOnPressure: boolean;
  optimizationMode: "balanced" | "aggressive";
  autoTrimMemory: boolean;
  autoCleanTemporary: boolean;
  autoCleanDays: number;
  includeBrowserCaches: boolean;
  deepCloseEnabled: boolean;
}

export interface MonitorSample {
  snapshot: SystemSnapshot;
  processes: ProcessInfo[];
  deepClose: DeepCloseStatus;
}
