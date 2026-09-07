import {
  ArrowDownRight,
  ArrowUpRight,
  ChevronRight,
  CircleGauge,
  Cpu,
  Fan,
  Gauge,
  HardDrive,
  MemoryStick,
  ShieldCheck,
  Sparkles,
  Thermometer,
  Zap,
} from "lucide-react";
import type {
  OptimizationResult,
  OptimizerStatus,
  ProcessInfo,
  SystemSnapshot,
  ViewId,
} from "../types";
import { clampPercent, formatBytes, formatDuration } from "../lib/format";

const statusCopy = {
  calm: {
    eyebrow: "Session is calm",
    title: "Comfortable headroom",
    caption: "Nothing is competing hard for resources.",
  },
  busy: {
    eyebrow: "Session is busy",
    title: "Working, with pressure",
    caption: "The system is responsive, but a few constraints are visible.",
  },
  pressure: {
    eyebrow: "Session is under pressure",
    title: "Something is holding it back",
    caption: "One or more resources need attention now.",
  },
};

function MetricRail({
  label,
  value,
  detail,
  percent,
  icon,
}: {
  label: string;
  value: string;
  detail: string;
  percent?: number;
  icon: React.ReactNode;
}) {
  return (
    <div className="metric-rail">
      <div className="metric-icon">{icon}</div>
      <div className="metric-copy">
        <div className="metric-label">{label}</div>
        <div className="metric-value">{value}</div>
        <div className="metric-detail">{detail}</div>
      </div>
      {percent !== undefined && (
        <div className="vertical-track" aria-label={`${label} ${Math.round(percent)} percent`}>
          <span style={{ height: `${clampPercent(percent)}%` }} />
        </div>
      )}
    </div>
  );
}

export function Overview({
  snapshot,
  processes,
  optimizer,
  optimizingNow,
  optimizationResult,
  maintenanceNotice,
  runOptimization,
  navigate,
}: {
  snapshot: SystemSnapshot;
  processes: ProcessInfo[];
  optimizer: OptimizerStatus | null;
  optimizingNow: boolean;
  optimizationResult: OptimizationResult | null;
  maintenanceNotice: string | null;
  runOptimization: () => Promise<void>;
  navigate: (view: ViewId) => void;
}) {
  const copy = statusCopy[snapshot.status];
  const topProcesses = [...processes].sort((a, b) => b.memoryBytes - a.memoryBytes).slice(0, 4);
  const primaryDisk = snapshot.disks.find((disk) => !disk.removable) ?? snapshot.disks[0];
  const hardwareReadings = [
    snapshot.hardware.gpuTempC == null
      ? null
      : {
          label: "GPU temperature",
          value: `${Math.round(snapshot.hardware.gpuTempC)}°C`,
          icon: <Thermometer size={16} />,
        },
    snapshot.hardware.gpuUsagePercent == null
      ? null
      : {
          label: "GPU activity",
          value: `${Math.round(snapshot.hardware.gpuUsagePercent)}%`,
          icon: <Gauge size={16} />,
        },
    snapshot.hardware.gpuVramUsedBytes == null
      ? null
      : {
          label: "VRAM used",
          value: formatBytes(snapshot.hardware.gpuVramUsedBytes),
          icon: <MemoryStick size={16} />,
        },
    snapshot.hardware.cpuClockMhz == null
      ? null
      : {
          label: "CPU clock",
          value: `${Math.round(snapshot.hardware.cpuClockMhz)} MHz`,
          icon: <Cpu size={16} />,
        },
    snapshot.hardware.gpuClockMhz == null
      ? null
      : {
          label: "GPU clock",
          value: `${Math.round(snapshot.hardware.gpuClockMhz)} MHz`,
          icon: <Gauge size={16} />,
        },
    snapshot.hardware.cpuPackagePowerW == null
      ? null
      : {
          label: "CPU package",
          value: `${snapshot.hardware.cpuPackagePowerW.toFixed(1)} W`,
          icon: <Zap size={16} />,
        },
    snapshot.hardware.gpuPowerW == null
      ? null
      : {
          label: "GPU power",
          value: `${snapshot.hardware.gpuPowerW.toFixed(1)} W`,
          icon: <Zap size={16} />,
        },
    snapshot.hardware.fanRpm == null
      ? null
      : {
          label: "Fastest fan",
          value: `${Math.round(snapshot.hardware.fanRpm)} RPM`,
          icon: <Fan size={16} />,
        },
    snapshot.hardware.storageTempC == null
      ? null
      : {
          label: "Hottest drive sensor",
          value: `${Math.round(snapshot.hardware.storageTempC)}°C`,
          detail: [snapshot.hardware.storageName, snapshot.hardware.storageSensorName]
            .filter(Boolean)
            .join(" · "),
          icon: <HardDrive size={16} />,
        },
  ].filter((reading): reading is NonNullable<typeof reading> => reading !== null);

  return (
    <div className="view-stack overview-view">
      <section className={`session-hero status-${snapshot.status}`}>
        <div className="hero-copy">
          <div className="eyebrow with-pulse">
            <span className="status-pulse" />
            {copy.eyebrow}
          </div>
          <h1>{copy.title}</h1>
          <p className="hero-caption">{copy.caption}</p>
          <div className="reason-stack">
            {snapshot.reasons.slice(0, 3).map((reason) => (
              <div className="reason-pill" key={reason}>
                <CircleGauge size={15} />
                {reason}
              </div>
            ))}
          </div>
        </div>
        <div className="hero-orbit" aria-hidden="true">
          <div className="orbit-shell">
            <div className="orbit-core">{Math.round(snapshot.memoryPercent)}%</div>
          </div>
          <span>current memory load</span>
        </div>
        <div className="session-meta">
          <span>SESSION</span>
          <strong>{formatDuration(snapshot.uptimeSeconds)}</strong>
        </div>
      </section>

      <section className="metrics-shell">
        <MetricRail
          label="Memory"
          value={`${formatBytes(snapshot.memoryUsedBytes)} / ${formatBytes(snapshot.memoryTotalBytes)}`}
          detail={`${formatBytes(snapshot.memoryAvailableBytes)} available`}
          percent={snapshot.memoryPercent}
          icon={<MemoryStick size={20} />}
        />
        <MetricRail
          label="CPU"
          value={`${Math.round(snapshot.cpuPercent)}%`}
          detail={`${snapshot.heavyProcessCount} heavy processes`}
          percent={snapshot.cpuPercent}
          icon={<Cpu size={20} />}
        />
        <MetricRail
          label="System drive"
          value={primaryDisk ? `${formatBytes(primaryDisk.availableBytes)} free` : "Unavailable"}
          detail={
            primaryDisk ? `${Math.round(primaryDisk.usedPercent)}% used` : "No fixed disk reported"
          }
          percent={primaryDisk?.usedPercent}
          icon={<HardDrive size={20} />}
        />
        <MetricRail
          label="CPU temperature"
          value={
            snapshot.hardware.cpuTempC == null
              ? "Unavailable"
              : `${Math.round(snapshot.hardware.cpuTempC)}°`
          }
          detail={
            snapshot.hardware.cpuTempC == null
              ? "No valid CPU temperature sensor was exposed"
              : snapshot.hardware.source
          }
          icon={<Thermometer size={20} />}
        />
      </section>

      <section className="hardware-strip floating-panel">
        <div className="hardware-strip-heading">
          <div>
            <span className="eyebrow">HARDWARE SENSORS</span>
            <h2>
              {hardwareReadings.length ? "Live silicon telemetry" : "Waiting for supported sensors"}
            </h2>
            <p>{snapshot.hardware.status}</p>
          </div>
          <span className={`sensor-access ${snapshot.hardware.elevated ? "enhanced" : "standard"}`}>
            <ShieldCheck size={14} />{" "}
            {snapshot.hardware.elevated ? "Enhanced access" : "Standard access"}
          </span>
        </div>
        {hardwareReadings.length > 0 && (
          <div className="hardware-reading-list">
            {hardwareReadings.map((reading) => (
              <div
                className={`hardware-reading ${"detail" in reading && reading.detail ? "has-detail" : ""}`}
                key={reading.label}
                title={"detail" in reading ? reading.detail : undefined}
              >
                <span>{reading.icon}</span>
                <small>{reading.label}</small>
                <strong>{reading.value}</strong>
                {"detail" in reading && reading.detail ? <em>{reading.detail}</em> : null}
              </div>
            ))}
          </div>
        )}
      </section>

      <section className={`autopilot-strip mode-${optimizer?.mode ?? "monitoring"}`}>
        <span className="autopilot-icon">
          <Sparkles size={20} />
        </span>
        <div className="autopilot-copy">
          <span className="eyebrow">ADAPTIVE AUTOPILOT</span>
          <h2>
            {!optimizer?.enabled
              ? "Paused by preference"
              : optimizer.tunedProcesses.length
                ? `Balancing ${optimizer.tunedProcesses.length} background process${optimizer.tunedProcesses.length === 1 ? "" : "es"}`
                : optimizer.mode === "observing"
                  ? "Confirming sustained pressure"
                  : "Watching continuously"}
          </h2>
          <p>{optimizer?.lastAction ?? "Starting the safe optimization controller…"}</p>
        </div>
        {optimizer?.tunedProcesses.length ? (
          <div className="tuned-processes">
            {optimizer.tunedProcesses.map((process) => (
              <span key={process.pid} title={process.reason}>
                {process.name} · {process.pid}
              </span>
            ))}
          </div>
        ) : (
          <div className="autopilot-guard">
            <ShieldCheck size={16} />
            <span>
              <strong>Protected work stays intact</strong>No tab closing, service killing or
              permanent deletion
            </span>
          </div>
        )}
        <div className="autopilot-actions">
          <button
            className="dark-button optimize-now-button"
            disabled={optimizingNow}
            onClick={() => void runOptimization()}
          >
            <Zap size={16} /> {optimizingNow ? "Optimizing…" : "Optimize now"}
          </button>
          <button className="quiet-button" onClick={() => navigate("settings")}>
            Configure <ChevronRight size={15} />
          </button>
        </div>
      </section>

      {(optimizationResult || maintenanceNotice) && (
        <section className="optimization-receipt" aria-live="polite">
          <Sparkles size={17} />
          <div>
            <strong>Latest measurable impact</strong>
            <span>
              {optimizationResult
                ? `${optimizationResult.trimmed} working sets trimmed · ${formatBytes(optimizationResult.releasedMemoryBytes)} RAM released${optimizationResult.tuned ? ` · ${optimizationResult.tuned} priorities lowered` : ""}`
                : maintenanceNotice}
            </span>
          </div>
          {optimizationResult?.errors.length ? (
            <small>{optimizationResult.errors.length} protected or locked processes skipped</small>
          ) : null}
        </section>
      )}

      <div className="overview-grid">
        <section className="floating-panel process-peek">
          <div className="section-heading">
            <div>
              <span className="eyebrow">HEAVY PROCESSES</span>
              <h2>Where memory is going</h2>
            </div>
            <button className="quiet-button" onClick={() => navigate("processes")}>
              All processes <ChevronRight size={16} />
            </button>
          </div>
          <div className="peek-list">
            {topProcesses.map((process, index) => (
              <button className="peek-row" key={process.pid} onClick={() => navigate("processes")}>
                <span className="process-rank">{String(index + 1).padStart(2, "0")}</span>
                <span className="process-identity">
                  <strong>{process.name}</strong>
                  <small>
                    {process.foreground ? "Foreground" : "Background"} · PID {process.pid}
                  </small>
                </span>
                <span className="process-stat">
                  <strong>{formatBytes(process.memoryBytes)}</strong>
                  <small>{process.cpuPercent.toFixed(1)}% CPU</small>
                </span>
              </button>
            ))}
          </div>
        </section>

        <section className="floating-panel io-panel">
          <div className="section-heading compact">
            <div>
              <span className="eyebrow">LIVE I/O</span>
              <h2>Disk movement</h2>
            </div>
          </div>
          <div className="io-pair">
            <div>
              <span className="io-icon">
                <ArrowDownRight size={18} />
              </span>
              <small>Read</small>
              <strong>{formatBytes(snapshot.diskReadBytesPerSec)}/s</strong>
            </div>
            <div>
              <span className="io-icon">
                <ArrowUpRight size={18} />
              </span>
              <small>Write</small>
              <strong>{formatBytes(snapshot.diskWriteBytesPerSec)}/s</strong>
            </div>
          </div>
          <div className="io-footer">
            <span>{snapshot.processCount} processes</span>
            <span>
              {snapshot.hungProcessCount
                ? `${snapshot.hungProcessCount} not responding`
                : "All app windows responding"}
            </span>
          </div>
        </section>
      </div>

      <section className="recommendation-strip">
        <div className="recommendation-intro">
          <span className="eyebrow">WHAT MATTERS</span>
          <h2>Explainable recommendations</h2>
        </div>
        <div className="recommendation-cards">
          {snapshot.recommendations.length ? (
            snapshot.recommendations.slice(0, 3).map((item) => (
              <article className={`recommendation-card ${item.kind}`} key={item.id}>
                <span className="recommendation-dot" />
                <div>
                  <h3>{item.title}</h3>
                  <p>{item.detail}</p>
                  {item.actionLabel && item.targetView && (
                    <button onClick={() => navigate(item.targetView!)}>
                      {item.actionLabel} <ChevronRight size={15} />
                    </button>
                  )}
                </div>
              </article>
            ))
          ) : (
            <article className="recommendation-card info">
              <span className="recommendation-dot" />
              <div>
                <h3>No action needed</h3>
                <p>The current session has enough headroom and no stalled app windows.</p>
              </div>
            </article>
          )}
        </div>
      </section>
    </div>
  );
}
