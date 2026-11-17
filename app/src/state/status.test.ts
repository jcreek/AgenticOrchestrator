import { describe, expect, it } from "vitest";
import { KILLED_EXIT_CODE, type SessionStatus } from "../bindings";
import { formatAgo, formatInState, presentMissingFolder, presentStatus, STARTING_GRACE_MS } from "./status";

const NOW = 1_000_000;
const entry = (status: SessionStatus, exitCode: number | null = null, since = NOW - 5_000) => ({
  status,
  since,
  exitCode,
});

describe("presentStatus", () => {
  it.each([
    ["working", "working", "Working", "Working", "var(--status-working)", false],
    ["needsInput", "input", "Waiting for input", "Input", "var(--status-input)", true],
    ["needsPermission", "permission", "Needs permission", "Permission", "var(--status-permission)", true],
    ["doneUnseen", "finished", "Finished", "Finished", "var(--status-finished)", false],
    ["idle", "idle", "Idle", "Idle", "var(--status-idle)", false],
  ] as const)("maps %s", (status, kind, label, tabLabel, colorVar, needsYou) => {
    expect(presentStatus(entry(status), false, NOW)).toMatchObject({ kind, label, tabLabel, colorVar, needsYou });
  });

  it("shows starting as input-styled, then asks you to check the terminal", () => {
    const fresh = presentStatus(entry("starting", null, NOW - 1_000), false, NOW);
    expect(fresh).toMatchObject({ kind: "input", tabLabel: "Starting", needsYou: false });
    const stuck = presentStatus(entry("starting", null, NOW - STARTING_GRACE_MS), false, NOW);
    expect(stuck).toMatchObject({ kind: "input", tabLabel: "Check terminal", needsYou: true });
  });

  it("treats a non-zero exit that the app did not cause as a crash", () => {
    expect(presentStatus(entry("exited", 1), false, NOW)).toMatchObject({
      kind: "crashed",
      label: "Crashed · exit 1",
      tabLabel: "Crashed",
      colorVar: "var(--status-crashed)",
    });
  });

  it.each([0, KILLED_EXIT_CODE, null])("shows exit code %s as an idle-styled exit", (code) => {
    expect(presentStatus(entry("exited", code), false, NOW)).toMatchObject({ kind: "idle", tabLabel: "Exited" });
  });

  it("shows running sessions as unknown when status is unavailable, without flashing", () => {
    const shown = presentStatus(entry("needsPermission"), true, NOW);
    expect(shown).toMatchObject({ kind: "unknown", label: "Status unknown", needsYou: false, flashOnce: false });
    expect(presentStatus(entry("exited", 2), true, NOW).kind).toBe("crashed");
  });

  it("shows a workspace without a Claude terminal as not started", () => {
    expect(presentStatus(undefined, false, NOW)).toMatchObject({ kind: "idle", label: "Not started", since: null });
  });

  it("shows a missing folder as unavailable", () => {
    expect(presentMissingFolder()).toMatchObject({ kind: "unknown", tabLabel: "Unavailable", needsYou: false });
  });
});

describe("formatInState", () => {
  it.each([
    [0, "0s"],
    [38_400, "38s"],
    [59_999, "59s"],
    [60_000, "1m"],
    [150_000, "2m"],
    [3_599_999, "59m"],
    [3_600_000, "1h"],
    [7_300_000, "2h"],
    [-500, "0s"],
  ])("formats %d ms as %s", (ms, text) => {
    expect(formatInState(ms)).toBe(text);
  });
});

describe("formatAgo", () => {
  it.each([
    [10_000, "just now"],
    [60_000, "1 minute ago"],
    [180_000, "3 minutes ago"],
    [3_600_000, "1 hour ago"],
    [10_800_000, "3 hours ago"],
  ])("formats %d ms as %s", (ms, text) => {
    expect(formatAgo(ms)).toBe(text);
  });
});
