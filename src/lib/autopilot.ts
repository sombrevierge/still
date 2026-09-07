import type { ProcessInfo } from "../types";

export function optimizerCandidates(
  processes: ProcessInfo[],
  cpuThreshold: number,
  aggressive = false,
): number[] {
  const memoryThreshold = (aggressive ? 128 : 256) * 1_048_576;
  return processes
    .filter(
      (process) =>
        (aggressive || process.background) &&
        !process.foreground &&
        !process.protected &&
        process.responsive &&
        (process.cpuPercent >= cpuThreshold || process.memoryBytes >= memoryThreshold),
    )
    .sort(
      (left, right) =>
        right.memoryBytes +
        right.cpuPercent * 16_777_216 -
        (left.memoryBytes + left.cpuPercent * 16_777_216),
    )
    .slice(0, aggressive ? 16 : 8)
    .map((process) => process.pid);
}
