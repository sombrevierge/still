import { describe, expect, it } from "vitest";
import type { ProcessInfo } from "../types";
import { optimizerCandidates } from "./autopilot";

function process(overrides: Partial<ProcessInfo>): ProcessInfo {
  return {
    pid: 1,
    parentPid: null,
    name: "worker.exe",
    executable: "C:\\Temp\\worker.exe",
    command: "worker.exe",
    cpuPercent: 40,
    memoryBytes: 128 * 1024 * 1024,
    uptimeSeconds: 120,
    status: "Run",
    responsive: true,
    foreground: false,
    background: true,
    orphaned: false,
    childCount: 0,
    protected: false,
    protectionReason: null,
    ...overrides,
  };
}

describe("optimizerCandidates", () => {
  it("keeps only responsive unprotected background work above the threshold", () => {
    const result = optimizerCandidates(
      [
        process({ pid: 10, cpuPercent: 25 }),
        process({ pid: 11, cpuPercent: 60, protected: true }),
        process({ pid: 12, cpuPercent: 55, foreground: true, background: false }),
        process({ pid: 13, cpuPercent: 10 }),
        process({ pid: 14, cpuPercent: 45, responsive: false }),
        process({ pid: 15, cpuPercent: 50 }),
      ],
      20,
    );
    expect(result).toEqual([15, 10]);
  });

  it("includes high-memory idle work and allows visible nonforeground work in aggressive mode", () => {
    const result = optimizerCandidates(
      [
        process({ pid: 20, cpuPercent: 0.2, memoryBytes: 400 * 1_048_576 }),
        process({
          pid: 21,
          cpuPercent: 1,
          memoryBytes: 160 * 1_048_576,
          background: false,
        }),
      ],
      20,
      true,
    );
    expect(result).toEqual([20, 21]);
  });
});
