import { Activity, HardDrive, ListTree, Settings2, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Overview } from "./components/Overview";
import { ProcessesView } from "./components/ProcessesView";
import { SettingsView } from "./components/SettingsView";
import { StorageView } from "./components/StorageView";
import { optimizerCandidates } from "./lib/autopilot";
import { isDesktop, native } from "./lib/native";
import type {
  AppSettings,
  DeepCloseStatus,
  OptimizationResult,
  OptimizerStatus,
  ProcessInfo,
  SystemSnapshot,
  ViewId,
} from "./types";

const defaultSettings: AppSettings = {
  exclusions: [],
  expectedBackground: [],
  refreshSeconds: 5,
  minimumSizeMb: 250,
  oldDays: 30,
  autopilotEnabled: true,
  autopilotCpuThreshold: 20,
  autopilotStorageAudit: true,
  pauseScanOnPressure: true,
  optimizationMode: "aggressive",
  autoTrimMemory: true,
  autoCleanTemporary: true,
  autoCleanDays: 7,
  includeBrowserCaches: true,
  deepCloseEnabled: true,
};

function loadSettings(): AppSettings {
  try {
    const loaded = {
      ...defaultSettings,
      ...JSON.parse(localStorage.getItem("still.settings") ?? "{}"),
    } as AppSettings;
    // v0.2 used a costly two-second triple scan. Migrate that default to the new
    // lightweight five-second cadence while preserving explicit 3/5/10 choices.
    loaded.refreshSeconds = loaded.refreshSeconds <= 2 ? 5 : Math.max(3, loaded.refreshSeconds);
    return loaded;
  } catch {
    return defaultSettings;
  }
}

const navItems: Array<{ id: ViewId; label: string; icon: React.ReactNode }> = [
  { id: "overview", label: "Session", icon: <Activity size={17} /> },
  { id: "processes", label: "Processes", icon: <ListTree size={17} /> },
  { id: "storage", label: "Storage", icon: <HardDrive size={17} /> },
  { id: "settings", label: "Settings", icon: <Settings2 size={17} /> },
];

export default function App() {
  const [view, setView] = useState<ViewId>(() =>
    import.meta.env.DEV && new URLSearchParams(window.location.search).has("cleanup-preview")
      ? "storage"
      : "overview",
  );
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [settings, setSettingsState] = useState<AppSettings>(loadSettings);
  const [optimizer, setOptimizer] = useState<OptimizerStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(true);
  const [optimizingNow, setOptimizingNow] = useState(false);
  const [optimizationResult, setOptimizationResult] = useState<OptimizationResult | null>(null);
  const [maintenanceNotice, setMaintenanceNotice] = useState<string | null>(null);
  const [deepClose, setDeepClose] = useState<DeepCloseStatus | null>(null);
  const [launchedAt] = useState(Date.now);
  const refreshInFlight = useRef(false);
  const auditInFlight = useRef(false);
  const cleanupInFlight = useRef(false);
  const snapshotRef = useRef<SystemSnapshot | null>(null);

  const setSettings = useCallback((value: AppSettings) => {
    setSettingsState(value);
    localStorage.setItem("still.settings", JSON.stringify(value));
  }, []);

  const refreshAll = useCallback(async () => {
    if (refreshInFlight.current) return;
    refreshInFlight.current = true;
    try {
      const {
        snapshot: nextSnapshot,
        processes: nextProcesses,
        deepClose: nextDeepClose,
      } = await native.sample(settings.deepCloseEnabled, settings.expectedBackground);
      const aggressive = settings.optimizationMode === "aggressive";
      const resourcePressure = aggressive
        ? nextSnapshot.cpuPercent >= 72 ||
          nextSnapshot.memoryPercent >= 82 ||
          nextSnapshot.memoryAvailableBytes < 1_500_000_000
        : nextSnapshot.cpuPercent >= 88 ||
          nextSnapshot.memoryPercent >= 90 ||
          nextSnapshot.memoryAvailableBytes < 900_000_000;
      const nextOptimizer = await native.optimize(
        settings.autopilotEnabled,
        resourcePressure,
        optimizerCandidates(nextProcesses, settings.autopilotCpuThreshold, aggressive),
        settings.expectedBackground,
        settings.autoTrimMemory,
        aggressive,
      );
      if (
        isDesktop &&
        settings.autopilotEnabled &&
        settings.pauseScanOnPressure &&
        resourcePressure
      ) {
        const scan = await native.scanStatus();
        if (scan?.running) await native.cancelScan();
      }
      snapshotRef.current = nextSnapshot;
      setSnapshot(nextSnapshot);
      setProcesses(nextProcesses);
      setDeepClose(nextDeepClose);
      setOptimizer(nextOptimizer);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRefreshing(false);
      refreshInFlight.current = false;
    }
  }, [
    settings.autopilotCpuThreshold,
    settings.autopilotEnabled,
    settings.deepCloseEnabled,
    settings.expectedBackground,
    settings.autoTrimMemory,
    settings.optimizationMode,
    settings.pauseScanOnPressure,
  ]);

  const refreshProcesses = useCallback(async () => {
    try {
      setProcesses(await native.processes());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  const runOptimization = useCallback(async () => {
    if (!isDesktop || optimizingNow) return;
    setOptimizingNow(true);
    setError(null);
    try {
      const result = await native.optimizeNow(
        settings.expectedBackground,
        settings.optimizationMode === "aggressive",
      );
      setOptimizationResult(result);
      await refreshAll();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setOptimizingNow(false);
    }
  }, [optimizingNow, refreshAll, settings.expectedBackground, settings.optimizationMode]);

  useEffect(() => {
    let cancelled = false;
    let timer = 0;
    const schedule = (delay?: number) => {
      const cadence = document.hidden
        ? Math.max(15, settings.refreshSeconds) * 1000
        : Math.max(3, settings.refreshSeconds) * 1000;
      timer = window.setTimeout(tick, delay ?? cadence);
    };
    const tick = async () => {
      await refreshAll();
      if (!cancelled) schedule();
    };
    const visibilityChanged = () => {
      window.clearTimeout(timer);
      schedule(document.hidden ? 15_000 : 0);
    };
    document.addEventListener("visibilitychange", visibilityChanged);
    schedule(0);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", visibilityChanged);
    };
  }, [refreshAll, settings.refreshSeconds]);

  useEffect(() => {
    if (!isDesktop || !settings.autopilotEnabled || !settings.autopilotStorageAudit) {
      return;
    }
    let cancelled = false;
    let timer = 0;
    const schedule = (delay: number) => {
      if (!cancelled) timer = window.setTimeout(run, delay);
    };
    const run = () => {
      const currentSnapshot = snapshotRef.current;
      if (
        !currentSnapshot ||
        currentSnapshot.cpuPercent > 55 ||
        currentSnapshot.memoryPercent > 92 ||
        auditInFlight.current
      ) {
        schedule(60_000);
        return;
      }
      const lastAudit = Number(localStorage.getItem("still.lastAutomaticAudit") ?? "0");
      const remaining = 24 * 60 * 60 * 1000 - (Date.now() - lastAudit);
      if (remaining > 0) {
        schedule(Math.max(60_000, remaining));
        return;
      }

      auditInFlight.current = true;
      void (async () => {
        try {
          const current = await native.scanStatus();
          if (current?.running) {
            schedule(5 * 60_000);
            return;
          }
          const roots = await native.defaultRoots();
          if (!roots.length) {
            schedule(5 * 60_000);
            return;
          }
          await native.startScan(
            roots,
            settings.exclusions,
            settings.minimumSizeMb,
            settings.oldDays,
          );
          localStorage.setItem("still.lastAutomaticAudit", String(Date.now()));
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
          schedule(5 * 60_000);
        } finally {
          auditInFlight.current = false;
        }
      })();
    };
    schedule(Math.max(0, 120_000 - (Date.now() - launchedAt)));
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [launchedAt, settings]);

  useEffect(() => {
    if (!isDesktop || !settings.autopilotEnabled || !settings.autoCleanTemporary) {
      return;
    }
    let cancelled = false;
    let timer = 0;
    const schedule = (delay: number) => {
      if (!cancelled) timer = window.setTimeout(run, delay);
    };
    const run = () => {
      const currentSnapshot = snapshotRef.current;
      if (
        !currentSnapshot ||
        currentSnapshot.cpuPercent > 65 ||
        currentSnapshot.memoryPercent > 95 ||
        cleanupInFlight.current
      ) {
        schedule(60_000);
        return;
      }
      const lastCleanup = Number(localStorage.getItem("still.lastAutomaticCleanup") ?? "0");
      const remaining = 24 * 60 * 60 * 1000 - (Date.now() - lastCleanup);
      if (remaining > 0) {
        schedule(Math.max(60_000, remaining));
        return;
      }
      cleanupInFlight.current = true;
      void native
        .automaticMaintenance(settings.autoCleanDays)
        .then((result) => {
          localStorage.setItem("still.lastAutomaticCleanup", String(Date.now()));
          if (result.movedToRecycleBin > 0) {
            setMaintenanceNotice(
              `Automatic cleanup moved ${result.movedToRecycleBin} old temporary items (${Math.round(result.totalSizeBytes / 1_048_576)} MB) to Recycle Bin.`,
            );
          }
        })
        .catch((cause) => {
          setError(cause instanceof Error ? cause.message : String(cause));
          schedule(5 * 60_000);
        })
        .finally(() => {
          cleanupInFlight.current = false;
        });
    };
    schedule(Math.max(0, 60_000 - (Date.now() - launchedAt)));
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [launchedAt, settings]);

  const statusLabel = useMemo(() => {
    if (!snapshot) return "Connecting";
    return snapshot.status === "calm" ? "Calm" : snapshot.status === "busy" ? "Busy" : "Pressure";
  }, [snapshot]);

  return (
    <div className="app-shell">
      <header className="app-header">
        <button className="brand" onClick={() => setView("overview")}>
          <span className="brand-mark">
            <Sparkles size={17} />
          </span>
          <span>
            <strong>Still</strong>
            <small>Session monitor</small>
          </span>
        </button>
        <nav className="main-segmented" aria-label="Main navigation">
          {navItems.map((item) => (
            <button
              key={item.id}
              className={view === item.id ? "active" : ""}
              onClick={() => setView(item.id)}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className={`header-status ${snapshot?.status ?? "loading"}`}>
          <span className="status-pulse" />
          <span>
            <small>Session</small>
            <strong>{statusLabel}</strong>
          </span>
        </div>
      </header>

      {error && (
        <div className="global-error">
          <span>{error}</span>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      <main className={`app-main view-${view}`}>
        {refreshing || !snapshot ? (
          <div className="loading-stage">
            <div className="scan-spinner">
              <span />
            </div>
            <span>Reading this session…</span>
          </div>
        ) : (
          <div className="view-enter" key={view}>
            {view === "overview" && (
              <Overview
                snapshot={snapshot}
                processes={processes}
                optimizer={optimizer}
                optimizingNow={optimizingNow}
                optimizationResult={optimizationResult}
                maintenanceNotice={maintenanceNotice}
                runOptimization={runOptimization}
                navigate={setView}
              />
            )}
            {view === "processes" && (
              <ProcessesView
                processes={processes}
                refresh={refreshProcesses}
                expected={settings.expectedBackground}
                onExpectedChange={(expectedBackground) =>
                  setSettings({ ...settings, expectedBackground })
                }
              />
            )}
            {view === "storage" && <StorageView settings={settings} />}
            {view === "settings" && (
              <SettingsView
                settings={settings}
                hardware={snapshot.hardware}
                deepClose={deepClose}
                onChange={setSettings}
              />
            )}
          </div>
        )}
      </main>
    </div>
  );
}
