import { describe, expect, it } from "vitest";
import { batchReducer, COLLECT_MS, initialBatch, SHOW_MS, stickyToasts } from "./toasts";

describe("batchReducer", () => {
  it("collects finished sessions for 10 s, then shows one toast for 6 s", () => {
    let state = batchReducer(initialBatch, { type: "finished", id: "a", now: 0 });
    state = batchReducer(state, { type: "finished", id: "b", now: 4_000 });
    state = batchReducer(state, { type: "finished", id: "a", now: 5_000 });
    state = batchReducer(state, { type: "tick", now: COLLECT_MS - 1 });
    expect(state.shown).toBeNull();

    state = batchReducer(state, { type: "tick", now: COLLECT_MS });
    expect(state).toEqual({ pending: null, shown: { ids: ["a", "b"], until: COLLECT_MS + SHOW_MS } });

    state = batchReducer(state, { type: "tick", now: COLLECT_MS + SHOW_MS });
    expect(state).toEqual(initialBatch);
  });

  it("starts a new batch while the previous toast is showing", () => {
    let state = batchReducer(initialBatch, { type: "finished", id: "a", now: 0 });
    state = batchReducer(state, { type: "tick", now: COLLECT_MS });
    state = batchReducer(state, { type: "finished", id: "b", now: COLLECT_MS + 1 });
    expect(state.shown?.ids).toEqual(["a"]);
    state = batchReducer(state, { type: "tick", now: 2 * COLLECT_MS + 1 });
    expect(state.shown?.ids).toEqual(["b"]);
  });

  it("dismisses the shown toast and forgets closed sessions", () => {
    let state = batchReducer(initialBatch, { type: "finished", id: "a", now: 0 });
    state = batchReducer(state, { type: "forget", id: "a" });
    expect(state).toEqual(initialBatch);

    state = batchReducer(initialBatch, { type: "finished", id: "a", now: 0 });
    state = batchReducer(state, { type: "tick", now: COLLECT_MS });
    expect(batchReducer(state, { type: "dismiss" }).shown).toBeNull();
  });
});

describe("stickyToasts", () => {
  const tabs = [
    { id: "i", kind: "input" as const, needsYou: true, since: 10 },
    { id: "p", kind: "permission" as const, needsYou: true, since: 20 },
    { id: "s", kind: "input" as const, needsYou: false, since: 30 },
    { id: "w", kind: "working" as const, needsYou: false, since: 40 },
  ];
  const base = { activeId: null, dismissed: {}, snoozedUntil: {}, now: 1_000 };

  it("shows input and permission tabs that are not in view, permission first", () => {
    expect(stickyToasts(tabs, base).map((tab) => tab.id)).toEqual(["p", "i"]);
    expect(stickyToasts(tabs, { ...base, activeId: "p" }).map((tab) => tab.id)).toEqual(["i"]);
  });

  it("hides a dismissed toast until the session enters a new state", () => {
    expect(stickyToasts(tabs, { ...base, dismissed: { i: 10 } }).map((tab) => tab.id)).toEqual(["p"]);
    expect(stickyToasts(tabs, { ...base, dismissed: { i: 9 } }).map((tab) => tab.id)).toEqual(["p", "i"]);
  });

  it("hides a snoozed toast until the snooze ends", () => {
    expect(stickyToasts(tabs, { ...base, snoozedUntil: { p: 1_001 } }).map((tab) => tab.id)).toEqual(["i"]);
    expect(stickyToasts(tabs, { ...base, snoozedUntil: { p: 1_000 } }).map((tab) => tab.id)).toEqual(["p", "i"]);
  });
});
