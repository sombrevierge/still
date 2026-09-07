import { describe, expect, it } from "vitest";
import { clampPercent, formatBytes, formatDuration } from "./format";

describe("format helpers", () => {
  it("formats binary sizes", () => {
    expect(formatBytes(1_073_741_824)).toBe("1.0 GB");
  });

  it("formats useful durations", () => {
    expect(formatDuration(3720)).toBe("1h 2m");
  });

  it("keeps resource percentages safe", () => {
    expect(clampPercent(140)).toBe(100);
    expect(clampPercent(-4)).toBe(0);
  });
});
