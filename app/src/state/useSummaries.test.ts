import { describe, expect, it } from "vitest";
import { ACTIVE_NET_MS, BACKGROUND_MS, CHANGED_MS, changedStatusIds, dueIds, FOCUS_MS, staleIds } from "./useSummaries";

describe("staleIds", () => {
  it("returns never-fetched workspaces and those past the background age only", () => {
    const now = 1_000_000;
    const fetchedAt = new Map([
      ["fresh", now - BACKGROUND_MS + 1],
      ["old", now - BACKGROUND_MS],
    ]);
    expect(staleIds(["fresh", "old", "never"], fetchedAt, now)).toEqual(["old", "never"]);
  });

  it("takes a shorter age for focus and restore", () => {
    const now = 1_000_000;
    const fetchedAt = new Map([
      ["recent", now - FOCUS_MS + 1],
      ["older", now - FOCUS_MS],
    ]);
    expect(staleIds(["recent", "older"], fetchedAt, now, FOCUS_MS)).toEqual(["older"]);
  });
});

describe("dueIds", () => {
  const now = 1_000_000;
  const fresh = new Map([["open", now - 5_000], ["back", now - 5_000], ["older", now - CHANGED_MS]]);

  it("refreshes nothing while no folder has changed", () => {
    const revisions = { open: 3, back: 7, older: 1 };
    const seen = new Map(Object.entries(revisions));
    expect(dueIds(["open", "back", "older"], "open", revisions, seen, fresh, now)).toEqual([]);
  });

  it("refreshes a changed open tab at once and a changed background tab only when 15 s old", () => {
    const seen = new Map([["open", 3], ["back", 7], ["older", 1]]);
    expect(dueIds(["open", "back", "older"], "open", { open: 4, back: 8, older: 2 }, seen, fresh, now)).toEqual(["open", "older"]);
  });

  it("refreshes an unchanged open tab once its summary is 30 s old", () => {
    const seen = new Map([["open", 3]]);
    expect(dueIds(["open"], "open", { open: 3 }, seen, new Map([["open", now - ACTIVE_NET_MS]]), now)).toEqual(["open"]);
  });

  it("polls an open tab whose folder is not watched, and leaves unwatched background tabs to the timer", () => {
    expect(dueIds(["open", "back"], "open", {}, new Map(), fresh, now)).toEqual(["open"]);
  });
});

describe("changedStatusIds", () => {
  const entry = (status: "idle" | "working", since: number) => ({ status, since, exitCode: null });

  it("reports workspaces whose status or start time changed, and new ones", () => {
    const before = { a: entry("idle", 1), b: entry("idle", 1), c: entry("working", 5) };
    const after = { a: entry("idle", 1), b: entry("working", 9), c: entry("working", 6), d: entry("idle", 1) };
    expect(changedStatusIds(before, after)).toEqual(["b", "c", "d"]);
  });
});
