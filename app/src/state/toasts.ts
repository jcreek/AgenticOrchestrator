import type { StatusKind } from "./status";

export const COLLECT_MS = 10_000;
export const SHOW_MS = 6_000;

export interface BatchState {
  pending: { ids: string[]; since: number } | null;
  shown: { ids: string[]; until: number } | null;
}

export type BatchAction =
  | { type: "finished"; id: string; now: number }
  | { type: "tick"; now: number }
  | { type: "dismiss" }
  | { type: "forget"; id: string };

export const initialBatch: BatchState = { pending: null, shown: null };

/** Collects finished sessions for 10 s into one toast that then shows for 6 s. */
export function batchReducer(state: BatchState, action: BatchAction): BatchState {
  switch (action.type) {
    case "finished": {
      const pending = state.pending ?? { ids: [], since: action.now };
      if (pending.ids.includes(action.id)) return state;
      return { ...state, pending: { ...pending, ids: [...pending.ids, action.id] } };
    }
    case "tick": {
      let next = state;
      if (next.shown && action.now >= next.shown.until) next = { ...next, shown: null };
      if (next.pending && action.now - next.pending.since >= COLLECT_MS) {
        next = { pending: null, shown: { ids: next.pending.ids, until: action.now + SHOW_MS } };
      }
      return next;
    }
    case "dismiss":
      return state.shown ? { ...state, shown: null } : state;
    case "forget": {
      const drop = (ids: string[]) => ids.filter((id) => id !== action.id);
      const pending = state.pending && { ...state.pending, ids: drop(state.pending.ids) };
      const shown = state.shown && { ...state.shown, ids: drop(state.shown.ids) };
      return {
        pending: pending && pending.ids.length > 0 ? pending : null,
        shown: shown && shown.ids.length > 0 ? shown : null,
      };
    }
  }
}

export interface StickySource {
  id: string;
  kind: StatusKind;
  needsYou: boolean;
  since: number | null;
}

export interface StickyFilter {
  activeId: string | null;
  /** The `since` of the state each toast was dismissed in; a new state shows it again. */
  dismissed: Record<string, number>;
  snoozedUntil: Record<string, number>;
  now: number;
}

/** Sticky toasts for sessions blocked on you whose tab is not in view, permission first. */
export function stickyToasts<T extends StickySource>(tabs: T[], filter: StickyFilter): T[] {
  const rank = (tab: T) => (tab.kind === "permission" ? 0 : 1);
  return tabs
    .filter(
      (tab) =>
        tab.needsYou &&
        (tab.kind === "permission" || tab.kind === "input") &&
        tab.id !== filter.activeId &&
        filter.dismissed[tab.id] !== tab.since &&
        (filter.snoozedUntil[tab.id] ?? 0) <= filter.now,
    )
    .map((tab, index) => ({ tab, index }))
    .sort((a, b) => rank(a.tab) - rank(b.tab) || a.index - b.index)
    .map((entry) => entry.tab);
}
