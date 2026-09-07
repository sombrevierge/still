import {
  Ban,
  ChevronRight,
  CircleStop,
  ExternalLink,
  GitBranch,
  RefreshCcw,
  Search,
  ShieldCheck,
  Sparkles,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import { basename, formatBytes, formatDuration } from "../lib/format";
import { native } from "../lib/native";
import type { ProcessInfo } from "../types";

type ProcessAction = "end" | "tree" | "restart" | "location";

export function ProcessesView({
  processes,
  refresh,
  expected,
  onExpectedChange,
}: {
  processes: ProcessInfo[];
  refresh: () => Promise<void>;
  expected: string[];
  onExpectedChange: (value: string[]) => void;
}) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"cpu" | "memory">("memory");
  const [selectedPid, setSelectedPid] = useState<number | null>(null);
  const [busyPid, setBusyPid] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const rows = useMemo(() => {
    const lowered = query.trim().toLowerCase();
    return [...processes]
      .filter((process) =>
        lowered
          ? `${process.name} ${process.executable} ${process.pid}`.toLowerCase().includes(lowered)
          : true,
      )
      .sort((a, b) =>
        sort === "memory" ? b.memoryBytes - a.memoryBytes : b.cpuPercent - a.cpuPercent,
      );
  }, [processes, query, sort]);
  const selected = processes.find((process) => process.pid === selectedPid) ?? null;

  async function action(process: ProcessInfo, type: ProcessAction) {
    if (process.protected && type !== "location") {
      setNotice(process.protectionReason ?? "This process is protected");
      return;
    }
    const destructive = type === "end" || type === "tree" || type === "restart";
    if (
      destructive &&
      !window.confirm(
        type === "tree"
          ? `End ${process.name} and its ${process.childCount} child processes? Unsaved work may be lost.`
          : `${type === "restart" ? "Restart" : "End"} ${process.name}? Unsaved work may be lost.`,
      )
    ) {
      return;
    }
    setBusyPid(process.pid);
    setNotice(null);
    try {
      const result =
        type === "end"
          ? await native.terminate(process.pid)
          : type === "tree"
            ? await native.terminateTree(process.pid)
            : null;
      if (type === "restart") await native.restart(process.pid);
      if (type === "location") await native.openLocation(process.pid);
      setNotice(
        result
          ? `${result.action}; Windows confirmed ${result.affectedCount} process${result.affectedCount === 1 ? "" : "es"} exited${result.elevated ? " with administrator approval" : ""}`
          : type === "location"
            ? "Opened the executable location"
            : `${process.name} restarted`,
      );
      await new Promise((resolve) => window.setTimeout(resolve, 350));
      await refresh();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusyPid(null);
    }
  }

  function toggleExpected(process: ProcessInfo) {
    const key = process.executable || process.name;
    onExpectedChange(
      expected.includes(key) ? expected.filter((item) => item !== key) : [...expected, key],
    );
  }

  return (
    <div className="view-stack processes-view">
      <header className="page-heading">
        <div>
          <span className="eyebrow">PROCESS LENS</span>
          <h1>What is alive right now</h1>
          <p>
            Visible apps, background workers and protected Windows processes in one tree-aware list.
          </p>
        </div>
        <div className="page-stat">
          <strong>{processes.length}</strong>
          <span>running processes</span>
        </div>
      </header>

      {notice && (
        <div className="action-notice">
          <Sparkles size={16} />
          <span>{notice}</span>
          <button onClick={() => setNotice(null)} aria-label="Dismiss">
            <X size={15} />
          </button>
        </div>
      )}

      <section className="process-layout">
        <div className="floating-panel process-table-panel">
          <div className="process-toolbar">
            <label className="search-field">
              <Search size={17} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search app, executable or PID"
              />
            </label>
            <div className="mini-segmented" aria-label="Sort processes">
              <button
                className={sort === "memory" ? "active" : ""}
                onClick={() => setSort("memory")}
              >
                Memory
              </button>
              <button className={sort === "cpu" ? "active" : ""} onClick={() => setSort("cpu")}>
                CPU
              </button>
            </div>
            <button
              className="icon-button pale"
              onClick={() => void refresh()}
              aria-label="Refresh processes"
            >
              <RefreshCcw size={17} />
            </button>
          </div>
          <div className="process-table-header">
            <span>Application</span>
            <span>State</span>
            <span>CPU</span>
            <span>Memory</span>
            <span />
          </div>
          <div className="process-table-body">
            {rows.map((process) => {
              const isExpected = expected.includes(process.executable || process.name);
              return (
                <button
                  className={`process-table-row ${selectedPid === process.pid ? "selected" : ""} ${busyPid === process.pid ? "busy" : ""}`}
                  key={process.pid}
                  onClick={() => setSelectedPid(process.pid)}
                >
                  <span className="table-process">
                    <span className="app-glyph">{process.name.slice(0, 1).toUpperCase()}</span>
                    <span>
                      <strong>{process.name}</strong>
                      <small>
                        PID {process.pid} · {process.childCount} children
                      </small>
                    </span>
                  </span>
                  <span className="state-cell">
                    {process.protected ? (
                      <ShieldCheck size={14} />
                    ) : process.responsive ? (
                      <span className="tiny-dot" />
                    ) : (
                      <Ban size={14} />
                    )}
                    {process.protected
                      ? "Protected"
                      : isExpected
                        ? "Expected"
                        : process.orphaned
                          ? "Possible leftover"
                          : process.foreground
                            ? "Foreground"
                            : "Background"}
                  </span>
                  <span className="mono-value">{process.cpuPercent.toFixed(1)}%</span>
                  <span className="mono-value">{formatBytes(process.memoryBytes)}</span>
                  <ChevronRight size={16} />
                </button>
              );
            })}
          </div>
        </div>

        <aside className={`floating-panel process-details ${selected ? "visible" : "empty"}`}>
          {selected ? (
            <>
              <div className="details-top">
                <span className="app-glyph large">{selected.name.slice(0, 1).toUpperCase()}</span>
                <div>
                  <span className="eyebrow">PROCESS DETAILS</span>
                  <h2>{selected.name}</h2>
                  <p>{basename(selected.executable) || `PID ${selected.pid}`}</p>
                </div>
              </div>
              <div className="detail-pills">
                <span>{selected.responsive ? "Responding" : "Not responding"}</span>
                <span>{selected.background ? "Background" : "Visible app"}</span>
                {selected.orphaned && <span className="warning">No living parent</span>}
              </div>
              <dl className="detail-grid">
                <div>
                  <dt>CPU</dt>
                  <dd>{selected.cpuPercent.toFixed(1)}%</dd>
                </div>
                <div>
                  <dt>Memory</dt>
                  <dd>{formatBytes(selected.memoryBytes)}</dd>
                </div>
                <div>
                  <dt>Uptime</dt>
                  <dd>{formatDuration(selected.uptimeSeconds)}</dd>
                </div>
                <div>
                  <dt>Parent</dt>
                  <dd>{selected.parentPid ?? "None"}</dd>
                </div>
              </dl>
              <div className="path-box">
                <small>Executable</small>
                <span>{selected.executable || "Unavailable"}</span>
              </div>
              {selected.protected ? (
                <div className="protected-note">
                  <ShieldCheck size={17} />
                  <span>
                    <strong>Protected by policy</strong>
                    {selected.protectionReason}
                  </span>
                </div>
              ) : (
                <div className="process-actions">
                  <button className="dark-button" onClick={() => void action(selected, "end")}>
                    <CircleStop size={17} /> End process
                  </button>
                  <button className="soft-button" onClick={() => void action(selected, "tree")}>
                    <GitBranch size={17} /> End tree
                  </button>
                  <button className="soft-button" onClick={() => void action(selected, "restart")}>
                    <RefreshCcw size={17} /> Restart
                  </button>
                </div>
              )}
              <div className="detail-footer-actions">
                <button onClick={() => void action(selected, "location")}>
                  <ExternalLink size={15} /> Open file location
                </button>
                <button onClick={() => toggleExpected(selected)}>
                  {expected.includes(selected.executable || selected.name) ? (
                    <X size={15} />
                  ) : (
                    <ShieldCheck size={15} />
                  )}
                  {expected.includes(selected.executable || selected.name)
                    ? "Remove expected mark"
                    : "Mark expected background"}
                </button>
              </div>
            </>
          ) : (
            <div className="empty-details">
              <span className="app-glyph large">?</span>
              <h2>Select a process</h2>
              <p>See its executable, parent, children, responsiveness and safe actions.</p>
            </div>
          )}
        </aside>
      </section>
    </div>
  );
}
