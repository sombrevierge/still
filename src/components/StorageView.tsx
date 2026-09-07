import { open } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  Broom,
  Check,
  ChevronDown,
  ChevronRight,
  FolderPlus,
  HardDrive,
  PackageOpen,
  RotateCcw,
  ScanSearch,
  ShieldCheck,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { basename, formatBytes, formatDate } from "../lib/format";
import { isDesktop, native } from "../lib/native";
import type { AppSettings, CleanupPreview, MaintenanceReport, ScanStatus } from "../types";

function GroupIcon({ category }: { category: string }) {
  if (category === "installers") return <PackageOpen size={20} />;
  if (category === "old-builds") return <RotateCcw size={20} />;
  return <Archive size={20} />;
}

export function StorageView({ settings }: { settings: AppSettings }) {
  const qaCleanupPreview =
    import.meta.env.DEV && new URLSearchParams(window.location.search).has("cleanup-preview");
  const [roots, setRoots] = useState<string[]>([]);
  const [scan, setScan] = useState<ScanStatus | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [cleaning, setCleaning] = useState(false);
  const [maintenance, setMaintenance] = useState<MaintenanceReport | null>(null);
  const [maintenanceSelection, setMaintenanceSelection] = useState<Set<string>>(new Set());
  const [maintenanceBusy, setMaintenanceBusy] = useState(isDesktop);
  const modalCloseRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    void native.defaultRoots().then(setRoots);
  }, []);

  useEffect(() => {
    if (!isDesktop) return;
    let active = true;
    void native
      .maintenanceReport(settings.includeBrowserCaches)
      .then((report) => {
        if (!active) return;
        setMaintenance(report);
        setMaintenanceSelection(
          new Set(
            report.targets
              .filter((target) => target.automaticEligible && target.reclaimableBytes > 0)
              .map((target) => target.id),
          ),
        );
      })
      .catch((error) => active && setNotice(String(error)))
      .finally(() => active && setMaintenanceBusy(false));
    return () => {
      active = false;
    };
  }, [settings.includeBrowserCaches]);

  useEffect(() => {
    if (!qaCleanupPreview) return;
    const originalMinHeight = document.body.style.minHeight;
    document.body.style.minHeight = "2800px";
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      window.scrollTo(0, 1400);
      secondFrame = window.requestAnimationFrame(() => {
        setPreview({
          requested: 11,
          eligible: 11,
          totalSizeBytes: 1_825_361_920,
          rejected: [],
        });
      });
    });
    return () => {
      window.cancelAnimationFrame(firstFrame);
      window.cancelAnimationFrame(secondFrame);
      document.body.style.minHeight = originalMinHeight;
    };
  }, [qaCleanupPreview]);

  useEffect(() => {
    if (!preview) return;
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !cleaning) setPreview(null);
    };
    document.body.classList.add("modal-open");
    window.addEventListener("keydown", onKeyDown);
    window.requestAnimationFrame(() => modalCloseRef.current?.focus());
    return () => {
      document.body.classList.remove("modal-open");
      window.removeEventListener("keydown", onKeyDown);
      previouslyFocused?.focus();
    };
  }, [preview, cleaning]);

  useEffect(() => {
    if (!isDesktop) return;
    let active = true;
    const poll = async () => {
      try {
        const value = await native.scanStatus();
        if (active && value) setScan(value);
        if (active && value?.running) window.setTimeout(poll, 650);
      } catch (error) {
        if (active) setNotice(String(error));
      }
    };
    void poll();
    return () => {
      active = false;
    };
  }, [scan?.id]);

  const selectedBytes = useMemo(
    () =>
      scan?.groups
        .flatMap((group) => group.files)
        .filter((file) => selected.has(file.path))
        .reduce((total, file) => total + file.sizeBytes, 0) ?? 0,
    [scan, selected],
  );

  async function addRoot() {
    const path = await open({
      directory: true,
      multiple: false,
      title: "Choose a user folder to scan",
    });
    if (typeof path === "string" && !roots.includes(path)) setRoots([...roots, path]);
  }

  async function startScan() {
    setNotice(null);
    setSelected(new Set());
    try {
      const id = await native.startScan(
        roots,
        settings.exclusions,
        settings.minimumSizeMb,
        settings.oldDays,
      );
      setScan({
        id,
        running: true,
        complete: false,
        cancelled: false,
        scannedFiles: 0,
        scannedBytes: 0,
        currentPath: roots[0] ?? "",
        startedAtMs: Date.now(),
        finishedAtMs: null,
        error: null,
        groups: [],
      });
    } catch (error) {
      setNotice(String(error));
    }
  }

  function toggleGroup(groupId: string) {
    const next = new Set(expanded);
    if (next.has(groupId)) next.delete(groupId);
    else next.add(groupId);
    setExpanded(next);
  }

  function toggleFile(path: string) {
    const next = new Set(selected);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    setSelected(next);
  }

  function toggleWholeGroup(paths: string[]) {
    const next = new Set(selected);
    const allSelected = paths.every((path) => next.has(path));
    paths.forEach((path) => {
      if (allSelected) next.delete(path);
      else next.add(path);
    });
    setSelected(next);
  }

  async function reviewCleanup() {
    try {
      setPreview(await native.dryRun([...selected]));
    } catch (error) {
      setNotice(String(error));
    }
  }

  async function confirmCleanup() {
    setCleaning(true);
    try {
      const result = await native.recycle([...selected]);
      setPreview(null);
      setSelected(new Set());
      setNotice(
        `Moved ${result.movedToRecycleBin} items (${formatBytes(result.totalSizeBytes)}) to Recycle Bin${
          result.failed.length ? `; ${result.failed.length} could not be moved` : ""
        }`,
      );
    } catch (error) {
      setNotice(String(error));
    } finally {
      setCleaning(false);
    }
  }

  async function refreshMaintenance() {
    setMaintenanceBusy(true);
    setNotice(null);
    try {
      setMaintenance(await native.maintenanceReport(settings.includeBrowserCaches));
    } catch (error) {
      setNotice(String(error));
    } finally {
      setMaintenanceBusy(false);
    }
  }

  function toggleMaintenance(id: string) {
    const next = new Set(maintenanceSelection);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setMaintenanceSelection(next);
  }

  async function cleanMaintenance() {
    if (!maintenanceSelection.size) return;
    const selectedTargets = maintenance?.targets.filter((target) =>
      maintenanceSelection.has(target.id),
    );
    const bytes =
      selectedTargets?.reduce((total, target) => total + target.reclaimableBytes, 0) ?? 0;
    if (
      !window.confirm(
        `Move ${formatBytes(bytes)} of old temporary and cache files to Recycle Bin? Active or locked files will be skipped.`,
      )
    ) {
      return;
    }
    setMaintenanceBusy(true);
    try {
      const result = await native.cleanMaintenance(
        [...maintenanceSelection],
        settings.includeBrowserCaches,
      );
      setNotice(
        `Moved ${result.movedToRecycleBin} maintenance items (${formatBytes(result.totalSizeBytes)}) to Recycle Bin${result.failed.length ? `; ${result.failed.length} locked or unavailable` : ""}`,
      );
      setMaintenanceSelection(new Set());
      setMaintenance(await native.maintenanceReport(settings.includeBrowserCaches));
    } catch (error) {
      setNotice(String(error));
    } finally {
      setMaintenanceBusy(false);
    }
  }

  return (
    <div className="view-stack storage-view">
      <header className="page-heading storage-heading">
        <div>
          <span className="eyebrow">SMART CLEANUP</span>
          <h1>See groups, not clutter</h1>
          <p>
            Reclaim known caches automatically, then review large personal files as meaningful
            groups.
          </p>
        </div>
        <button
          className="dark-button scan-button"
          onClick={() => void startScan()}
          disabled={!isDesktop || scan?.running || roots.length === 0}
        >
          <ScanSearch size={18} /> {scan?.running ? "Scanning…" : "Start scan"}
        </button>
      </header>

      {notice && (
        <div className="action-notice">
          <ShieldCheck size={16} />
          <span>{notice}</span>
          <button onClick={() => setNotice(null)}>
            <X size={15} />
          </button>
        </div>
      )}

      <section className="maintenance-panel floating-panel">
        <div className="maintenance-heading">
          <span className="setting-icon dark">
            <Broom size={20} />
          </span>
          <div>
            <span className="eyebrow">SYSTEM MAINTENANCE</span>
            <h2>Safe places Windows can rebuild</h2>
            <p>
              Temp files, crash dumps and graphics caches are eligible for autopilot. Developer and
              browser caches always require this manual selection.
            </p>
          </div>
          <div className="maintenance-total">
            <strong>{maintenance ? formatBytes(maintenance.totalReclaimableBytes) : "—"}</strong>
            <span>reclaimable now</span>
          </div>
        </div>
        <div className="maintenance-targets">
          {maintenance?.targets
            .filter((target) => target.reclaimableBytes > 0)
            .map((target) => (
              <button
                key={target.id}
                className={`maintenance-target ${maintenanceSelection.has(target.id) ? "selected" : ""}`}
                onClick={() => toggleMaintenance(target.id)}
                title={target.path}
              >
                <span
                  className={`selection-toggle ${maintenanceSelection.has(target.id) ? "checked" : ""}`}
                >
                  {maintenanceSelection.has(target.id) && <Check size={15} />}
                </span>
                <span>
                  <strong>{target.title}</strong>
                  <small>
                    {target.itemCount} items · older than {target.minimumAgeDays} days
                  </small>
                </span>
                {target.automaticEligible && <em>Auto-safe</em>}
                <b>{formatBytes(target.reclaimableBytes)}</b>
              </button>
            ))}
          {!maintenanceBusy && maintenance?.targets.every((target) => !target.reclaimableBytes) && (
            <div className="maintenance-empty">Known temporary locations are already clean.</div>
          )}
        </div>
        <div className="maintenance-actions">
          <button
            className="soft-button"
            disabled={maintenanceBusy}
            onClick={() => void refreshMaintenance()}
          >
            <RotateCcw size={16} /> {maintenanceBusy ? "Analyzing…" : "Analyze again"}
          </button>
          <button
            className="dark-button"
            disabled={maintenanceBusy || !maintenanceSelection.size}
            onClick={() => void cleanMaintenance()}
          >
            <Trash2 size={16} /> Clean selected
          </button>
        </div>
      </section>

      <section className="scan-control floating-panel">
        <div className="scan-roots">
          <span className="eyebrow">SCAN LOCATIONS</span>
          <div className="root-chips">
            {roots.map((root) => (
              <span className="root-chip" key={root} title={root}>
                <HardDrive size={15} /> {basename(root)}
                <button
                  onClick={() => setRoots(roots.filter((item) => item !== root))}
                  aria-label={`Remove ${root}`}
                >
                  <X size={13} />
                </button>
              </span>
            ))}
            <button className="add-root" onClick={() => void addRoot()} disabled={!isDesktop}>
              <FolderPlus size={15} /> Add folder
            </button>
          </div>
        </div>
        <div className="scan-safety">
          <ShieldCheck size={18} />
          <span>
            <strong>Dry-run by default</strong>Protected roots and junctions are skipped.
          </span>
        </div>
      </section>

      {scan?.running && (
        <section className="scan-progress-card">
          <div className="scan-spinner">
            <span />
          </div>
          <div className="scan-progress-copy">
            <span className="eyebrow">SCANNING IN BACKGROUND</span>
            <h2>{scan.scannedFiles.toLocaleString()} files inspected</h2>
            <p title={scan.currentPath}>
              {basename(scan.currentPath)} · {formatBytes(scan.scannedBytes)} observed
            </p>
            <div className="indeterminate-track">
              <span />
            </div>
          </div>
          <button className="soft-button" onClick={() => void native.cancelScan()}>
            Cancel
          </button>
        </section>
      )}

      {!scan?.running && scan?.groups.length ? (
        <section className="storage-results">
          <div className="results-heading">
            <div>
              <span className="eyebrow">RECOMMENDATIONS</span>
              <h2>{scan.groups.length} meaningful groups</h2>
            </div>
            <div className="selection-summary">
              <span>{selected.size} selected</span>
              <strong>{formatBytes(selectedBytes)}</strong>
              <button
                className="dark-button"
                disabled={!selected.size}
                onClick={() => void reviewCleanup()}
              >
                <Trash2 size={16} /> Review cleanup
              </button>
            </div>
          </div>
          <div className="storage-groups">
            {scan.groups.map((group) => {
              const paths = group.files.map((file) => file.path);
              const groupSelected = paths.length > 0 && paths.every((path) => selected.has(path));
              const isExpanded = expanded.has(group.id);
              return (
                <article className="storage-group floating-panel" key={group.id}>
                  <button
                    className={`selection-toggle ${groupSelected ? "checked" : ""}`}
                    onClick={() => toggleWholeGroup(paths)}
                    aria-label={`Select ${group.title}`}
                  >
                    {groupSelected && <Check size={16} />}
                  </button>
                  <span className="group-icon">
                    <GroupIcon category={group.category} />
                  </span>
                  <button className="group-main" onClick={() => toggleGroup(group.id)}>
                    <span>
                      <strong>{group.title}</strong>
                      <small>{group.summary}</small>
                    </span>
                    <span className="group-dates">
                      {formatDate(group.oldestAtMs)} — {formatDate(group.newestAtMs)}
                    </span>
                    <span className="group-total">
                      <strong>{formatBytes(group.totalSizeBytes)}</strong>
                      <small>{group.fileCount} items</small>
                    </span>
                    {isExpanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                  </button>
                  {isExpanded && (
                    <div className="group-files">
                      {group.files.map((file) => (
                        <button
                          className="group-file"
                          key={file.id}
                          onClick={() => toggleFile(file.path)}
                        >
                          <span
                            className={`selection-toggle small ${selected.has(file.path) ? "checked" : ""}`}
                          >
                            {selected.has(file.path) && <Check size={13} />}
                          </span>
                          <span className="file-name">
                            <strong>{file.name}</strong>
                            <small title={file.path}>{file.path}</small>
                          </span>
                          <span>{formatDate(file.modifiedAtMs)}</span>
                          <strong>{formatBytes(file.sizeBytes)}</strong>
                        </button>
                      ))}
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        </section>
      ) : !scan?.running ? (
        <section className="empty-scan-state">
          <div className="physical-stack" aria-hidden="true">
            <span />
            <span />
            <span>
              <ScanSearch size={34} />
            </span>
          </div>
          <div>
            <h2>{scan?.cancelled ? "Scan cancelled safely" : "Ready when you are"}</h2>
            <p>
              {isDesktop
                ? "Choose user folders, then run a cancellable background scan."
                : "Storage scanning is available in the Windows desktop build."}
            </p>
          </div>
        </section>
      ) : null}

      {preview &&
        createPortal(
          <div
            className="modal-backdrop"
            onMouseDown={(event) => {
              if (event.currentTarget === event.target && !cleaning) setPreview(null);
            }}
          >
            <section
              className="cleanup-modal"
              role="dialog"
              aria-modal="true"
              aria-labelledby="cleanup-modal-title"
              aria-describedby="cleanup-modal-description"
              aria-busy={cleaning}
            >
              <button
                ref={modalCloseRef}
                className="modal-close"
                aria-label="Close cleanup confirmation"
                disabled={cleaning}
                onClick={() => setPreview(null)}
              >
                <X size={18} />
              </button>
              <span className="modal-icon">
                <Trash2 size={23} />
              </span>
              <span className="eyebrow">RECOVERABLE CLEANUP</span>
              <h2 id="cleanup-modal-title">Move {preview.eligible} items to Recycle Bin?</h2>
              <p id="cleanup-modal-description">
                {formatBytes(preview.totalSizeBytes)} will become recoverable from Windows Recycle
                Bin. Permanent deletion is not used.
              </p>
              {preview.rejected.length > 0 && (
                <div className="rejected-note">
                  {preview.rejected.length} protected or unavailable items were excluded.
                </div>
              )}
              <div className="modal-actions">
                <button
                  className="soft-button"
                  disabled={cleaning}
                  onClick={() => setPreview(null)}
                >
                  Keep everything
                </button>
                <button
                  className="dark-button"
                  disabled={cleaning || preview.eligible === 0}
                  onClick={() => void confirmCleanup()}
                >
                  {cleaning ? "Moving…" : "Move to Recycle Bin"}
                </button>
              </div>
            </section>
          </div>,
          document.body,
        )}
    </div>
  );
}
