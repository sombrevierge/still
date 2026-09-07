import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderPlus,
  Gauge,
  KeyRound,
  LockKeyhole,
  Power,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Thermometer,
  Trash2,
  X,
} from "lucide-react";
import { useState } from "react";
import { isDesktop, native } from "../lib/native";
import type { AppSettings, DeepCloseStatus, HardwareMetrics } from "../types";

export function SettingsView({
  settings,
  hardware,
  deepClose,
  onChange,
}: {
  settings: AppSettings;
  hardware: HardwareMetrics;
  deepClose: DeepCloseStatus | null;
  onChange: (settings: AppSettings) => void;
}) {
  const [sensorNotice, setSensorNotice] = useState<string | null>(null);
  const [enablingSensors, setEnablingSensors] = useState(false);

  async function addExclusion() {
    const path = await open({
      directory: true,
      multiple: false,
      title: "Exclude a folder from scans",
    });
    if (typeof path === "string" && !settings.exclusions.includes(path)) {
      onChange({ ...settings, exclusions: [...settings.exclusions, path] });
    }
  }

  async function enableEnhancedSensors() {
    setEnablingSensors(true);
    setSensorNotice(null);
    try {
      setSensorNotice(await native.enableElevatedSensors());
    } catch (error) {
      setSensorNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setEnablingSensors(false);
    }
  }

  return (
    <div className="view-stack settings-view">
      <header className="page-heading">
        <div>
          <span className="eyebrow">LOCAL PREFERENCES</span>
          <h1>Quiet, explicit defaults</h1>
          <p>Still keeps settings on this PC and does not send usage or file data anywhere.</p>
        </div>
        <div className="privacy-seal">
          <LockKeyhole size={19} />
          <span>
            <strong>Local only</strong>No cloud telemetry
          </span>
        </div>
      </header>

      <section className="settings-grid">
        <article className="floating-panel setting-card wide hardware-setting">
          <span className="setting-icon dark">
            <Thermometer size={20} />
          </span>
          <div className="setting-copy">
            <h2>Hardware sensor access</h2>
            <p>
              {hardware.status}. Provider: {hardware.source}. Zero and impossible readings are
              rejected instead of being presented as real temperatures.
            </p>
            {sensorNotice && <span className="sensor-notice">{sensorNotice}</span>}
          </div>
          <button
            className={hardware.elevated ? "soft-button" : "dark-button"}
            disabled={!isDesktop || hardware.elevated || enablingSensors}
            onClick={() => void enableEnhancedSensors()}
          >
            <KeyRound size={16} />
            {hardware.elevated
              ? "Enhanced access active"
              : enablingSensors
                ? "Waiting for UAC…"
                : "Enable enhanced sensors"}
          </button>
          <div className="hardware-access-note">
            <ShieldCheck size={15} /> Only the sensor helper is elevated. Still, cleanup and process
            automation keep their normal user permissions.
          </div>
        </article>

        <article className="floating-panel setting-card wide deep-close-setting">
          <span className="setting-icon dark">
            <Power size={20} />
          </span>
          <div className="setting-copy">
            <h2>Deep close applications</h2>
            <p>
              After the last visible window closes, Still allows a normal exit, then stops the
              remaining processes from that application tree.
            </p>
            <span className="deep-close-status">
              {deepClose?.lastAction ?? "Preparing the application window monitor"}
            </span>
          </div>
          <button
            className={`physical-switch ${settings.deepCloseEnabled ? "active" : ""}`}
            role="switch"
            aria-checked={settings.deepCloseEnabled}
            aria-label="Deep close applications"
            onClick={() => onChange({ ...settings, deepCloseEnabled: !settings.deepCloseEnabled })}
          >
            <span />
          </button>
          <div className="hardware-access-note">
            <ShieldCheck size={15} /> No automatic UAC prompts. Windows, VPN connections, Docker,
            WSL, Android Studio, developer tools and apps marked as expected background are
            excluded.
          </div>
        </article>

        <article className="floating-panel setting-card wide autopilot-setting">
          <span className="setting-icon dark">
            <Sparkles size={20} />
          </span>
          <div className="setting-copy">
            <h2>Adaptive autopilot</h2>
            <p>
              Responds to CPU and memory pressure by lowering safe background priorities and
              releasing unused working-set memory. Changes are measured and priorities are restored.
            </p>
          </div>
          <button
            className={`physical-switch ${settings.autopilotEnabled ? "active" : ""}`}
            role="switch"
            aria-checked={settings.autopilotEnabled}
            aria-label="Adaptive autopilot"
            onClick={() => onChange({ ...settings, autopilotEnabled: !settings.autopilotEnabled })}
          >
            <span />
          </button>
          <div className="autopilot-options">
            <div className="optimization-mode-row">
              <span>
                <Sparkles size={15} /> Optimization strength
              </span>
              <div className="mini-segmented" aria-label="Optimization strength">
                {(["balanced", "aggressive"] as const).map((mode) => (
                  <button
                    key={mode}
                    className={settings.optimizationMode === mode ? "active" : ""}
                    disabled={!settings.autopilotEnabled}
                    onClick={() => onChange({ ...settings, optimizationMode: mode })}
                  >
                    {mode === "balanced" ? "Balanced" : "Aggressive"}
                  </button>
                ))}
              </div>
            </div>
            <label>
              <span>
                <Gauge size={15} /> Background CPU trigger
              </span>
              <select
                value={settings.autopilotCpuThreshold}
                disabled={!settings.autopilotEnabled}
                onChange={(event) =>
                  onChange({ ...settings, autopilotCpuThreshold: Number(event.target.value) })
                }
              >
                <option value={15}>15% CPU</option>
                <option value={20}>20% CPU</option>
                <option value={30}>30% CPU</option>
                <option value={40}>40% CPU</option>
              </select>
            </label>
            <button
              className={`option-toggle ${settings.autoTrimMemory ? "active" : ""}`}
              disabled={!settings.autopilotEnabled}
              onClick={() => onChange({ ...settings, autoTrimMemory: !settings.autoTrimMemory })}
            >
              <span className="mini-check">{settings.autoTrimMemory ? "✓" : ""}</span>
              <span>
                <strong>Release idle RAM</strong>
                Trims rebuildable working-set pages without closing apps or browser tabs
              </span>
            </button>
            <button
              className={`option-toggle ${settings.autopilotStorageAudit ? "active" : ""}`}
              disabled={!settings.autopilotEnabled}
              onClick={() =>
                onChange({
                  ...settings,
                  autopilotStorageAudit: !settings.autopilotStorageAudit,
                })
              }
            >
              <span className="mini-check">{settings.autopilotStorageAudit ? "✓" : ""}</span>
              <span>
                <strong>Daily calm-time storage audit</strong>
                Read-only scan; it never selects or removes files
              </span>
            </button>
            <button
              className={`option-toggle ${settings.autoCleanTemporary ? "active" : ""}`}
              disabled={!settings.autopilotEnabled}
              onClick={() =>
                onChange({ ...settings, autoCleanTemporary: !settings.autoCleanTemporary })
              }
            >
              <span className="mini-check">{settings.autoCleanTemporary ? "✓" : ""}</span>
              <span>
                <strong>Daily recoverable temp cleanup</strong>
                Only allow-listed old temp files, crash dumps and graphics caches; Recycle Bin only
              </span>
            </button>
            <label>
              <span>Automatic cleanup age</span>
              <select
                value={settings.autoCleanDays}
                disabled={!settings.autopilotEnabled || !settings.autoCleanTemporary}
                onChange={(event) =>
                  onChange({ ...settings, autoCleanDays: Number(event.target.value) })
                }
              >
                <option value={3}>Older than 3 days</option>
                <option value={7}>Older than 7 days</option>
                <option value={14}>Older than 14 days</option>
                <option value={30}>Older than 30 days</option>
              </select>
            </label>
            <button
              className={`option-toggle ${settings.includeBrowserCaches ? "active" : ""}`}
              onClick={() =>
                onChange({ ...settings, includeBrowserCaches: !settings.includeBrowserCaches })
              }
            >
              <span className="mini-check">{settings.includeBrowserCaches ? "✓" : ""}</span>
              <span>
                <strong>Find browser caches</strong>
                Shows Chrome, Edge and Firefox caches for manual review; never closes tabs
              </span>
            </button>
            <button
              className={`option-toggle ${settings.pauseScanOnPressure ? "active" : ""}`}
              disabled={!settings.autopilotEnabled}
              onClick={() =>
                onChange({ ...settings, pauseScanOnPressure: !settings.pauseScanOnPressure })
              }
            >
              <span className="mini-check">{settings.pauseScanOnPressure ? "✓" : ""}</span>
              <span>
                <strong>Yield during system pressure</strong>
                Cancels Still’s own storage scan before competing with your work
              </span>
            </button>
          </div>
          <div className="autopilot-safety-note">
            <ShieldCheck size={15} /> System processes, foreground apps, Docker, WSL, Android Studio
            and active developer tools are always excluded from automatic process actions.
          </div>
        </article>

        <article className="floating-panel setting-card">
          <span className="setting-icon">
            <RotateCcw size={20} />
          </span>
          <div className="setting-copy">
            <h2>Refresh cadence</h2>
            <p>System metrics should feel live without becoming their own workload.</p>
          </div>
          <div className="setting-control mini-segmented">
            {[3, 5, 10].map((seconds) => (
              <button
                key={seconds}
                className={settings.refreshSeconds === seconds ? "active" : ""}
                onClick={() => onChange({ ...settings, refreshSeconds: seconds })}
              >
                {seconds}s
              </button>
            ))}
          </div>
        </article>

        <article className="floating-panel setting-card">
          <span className="setting-icon">
            <ShieldCheck size={20} />
          </span>
          <div className="setting-copy">
            <h2>Storage thresholds</h2>
            <p>Metadata thresholds reduce noise before semantic grouping begins.</p>
          </div>
          <div className="threshold-pair">
            <label>
              <span>Large file</span>
              <select
                value={settings.minimumSizeMb}
                onChange={(event) =>
                  onChange({ ...settings, minimumSizeMb: Number(event.target.value) })
                }
              >
                <option value={100}>100 MB</option>
                <option value={250}>250 MB</option>
                <option value={500}>500 MB</option>
                <option value={1024}>1 GB</option>
              </select>
            </label>
            <label>
              <span>Old build</span>
              <select
                value={settings.oldDays}
                onChange={(event) => onChange({ ...settings, oldDays: Number(event.target.value) })}
              >
                <option value={14}>14 days</option>
                <option value={30}>30 days</option>
                <option value={60}>60 days</option>
                <option value={90}>90 days</option>
              </select>
            </label>
          </div>
        </article>

        <article className="floating-panel setting-card wide">
          <span className="setting-icon">
            <FolderPlus size={20} />
          </span>
          <div className="setting-copy">
            <h2>Protected exclusions</h2>
            <p>
              These folders are never traversed during a storage scan. Windows and program roots are
              protected independently.
            </p>
          </div>
          <button className="soft-button" onClick={() => void addExclusion()} disabled={!isDesktop}>
            <FolderPlus size={16} /> Add folder
          </button>
          <div className="setting-list">
            {settings.exclusions.length ? (
              settings.exclusions.map((path) => (
                <div key={path}>
                  <span title={path}>{path}</span>
                  <button
                    onClick={() =>
                      onChange({
                        ...settings,
                        exclusions: settings.exclusions.filter((item) => item !== path),
                      })
                    }
                  >
                    <X size={15} />
                  </button>
                </div>
              ))
            ) : (
              <p className="list-empty">
                No custom exclusions. Built-in protected roots remain active.
              </p>
            )}
          </div>
        </article>

        <article className="floating-panel setting-card wide">
          <span className="setting-icon">
            <ShieldCheck size={20} />
          </span>
          <div className="setting-copy">
            <h2>Expected background apps</h2>
            <p>
              Marked executables stay visible, are not described as leftovers and are excluded from
              Deep close.
            </p>
          </div>
          <div className="setting-list">
            {settings.expectedBackground.length ? (
              settings.expectedBackground.map((path) => (
                <div key={path}>
                  <span title={path}>{path}</span>
                  <button
                    onClick={() =>
                      onChange({
                        ...settings,
                        expectedBackground: settings.expectedBackground.filter(
                          (item) => item !== path,
                        ),
                      })
                    }
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              ))
            ) : (
              <p className="list-empty">Nothing has been marked as expected yet.</p>
            )}
          </div>
        </article>
      </section>
    </div>
  );
}
