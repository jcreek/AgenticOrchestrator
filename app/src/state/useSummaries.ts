import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type StatusChanged_Serialize } from "../bindings";
import type { Summary } from "./model";

export const ACTIVE_MS = 5_000;
/** The open tab refreshes this often even with no change reported, for git changes made outside its folder. */
export const ACTIVE_NET_MS = 30_000;
/** A safety net only: the backend reports folders that changed, so a timer rarely finds anything new. */
export const BACKGROUND_MS = 300_000;
/** Focus and restore refresh only older summaries, so switching windows often does not rerun them all. */
export const FOCUS_MS = 15_000;
/** A folder that keeps changing, such as one being built, refreshes at most this often. */
export const CHANGED_MS = 15_000;
const CONCURRENCY = 3;
/** Higher while a workspace still has no summary: beyond 6 the first summary arrives later for little gain overall. */
const FIRST_FILL_CONCURRENCY = 6;

type Statuses = StatusChanged_Serialize["workspaces"];

/** Workspaces whose summary is at least `maxAge` old, or was never fetched. */
export function staleIds(ids: string[], fetchedAt: ReadonlyMap<string, number>, now: number, maxAge = BACKGROUND_MS): string[] {
  return ids.filter((id) => now - (fetchedAt.get(id) ?? -Infinity) >= maxAge);
}

/** Workspaces to refresh on a tick, given each watched folder's change counter and the counter at its last refresh. */
export function dueIds(
  ids: string[],
  activeId: string | null,
  revisions: Partial<Record<string, number>>,
  seen: ReadonlyMap<string, number | undefined>,
  fetchedAt: ReadonlyMap<string, number>,
  now: number,
): string[] {
  return ids.filter((id) => {
    const active = id === activeId;
    const age = now - (fetchedAt.get(id) ?? -Infinity);
    const revision = revisions[id];
    const unwatched = revision === undefined;
    if (unwatched) return active;
    if (revision === seen.get(id)) return active && age >= ACTIVE_NET_MS;
    return active || age >= CHANGED_MS;
  });
}

/** Workspaces whose status, or the time it began, differs between two snapshots. */
export function changedStatusIds(before: Statuses, after: Statuses): string[] {
  return Object.entries(after)
    .filter(([id, entry]) => before[id]?.status !== entry.status || before[id]?.since !== entry.since)
    .map(([id]) => id);
}

/**
 * Refreshes tab summaries a few at a time, open tab first, so background git work never delays what the user opened.
 * Nothing runs while the page is hidden.
 */
export function useSummaries(ids: string[], activeId: string | null, visible: boolean, focused: boolean, statuses: StatusChanged_Serialize) {
  const [summaries, setSummaries] = useState<Record<string, Summary>>({});
  const idsRef = useRef(ids);
  idsRef.current = ids;
  const activeRef = useRef(activeId);
  activeRef.current = activeId;
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  const seen = useRef(new Map<string, number | undefined>());
  const fetchedAt = useRef(new Map<string, number>());
  const queue = useRef<string[]>([]);
  const running = useRef(new Set<string>());
  const again = useRef(new Set<string>());

  const pump = useCallback(() => {
    const filling = idsRef.current.some((id) => !fetchedAt.current.has(id));
    const limit = filling ? FIRST_FILL_CONCURRENCY : CONCURRENCY;
    while (visibleRef.current && running.current.size < limit && queue.current.length > 0) {
      const id = queue.current.shift()!;
      if (!idsRef.current.includes(id)) continue;
      running.current.add(id);
      commands
        .workspaceSummary(id)
        .then(
          (summary) => {
            if (!idsRef.current.includes(id)) return;
            setSummaries((current) =>
              JSON.stringify(current[id]) === JSON.stringify(summary) ? current : { ...current, [id]: summary },
            );
          },
          () => undefined,
        )
        .finally(() => {
          fetchedAt.current.set(id, Date.now());
          running.current.delete(id);
          if (again.current.delete(id)) queue.current.unshift(id);
          pump();
        });
    }
  }, []);

  // Urgent targets go first, and run again when a fetch for them is already under way.
  const request = useCallback(
    (targets: string[], urgent = false) => {
      for (const id of targets) {
        if (running.current.has(id)) {
          if (urgent) again.current.add(id);
          continue;
        }
        const at = queue.current.indexOf(id);
        if (at >= 0 && !urgent) continue;
        if (at >= 0) queue.current.splice(at, 1);
        if (urgent) queue.current.unshift(id);
        else queue.current.push(id);
      }
      pump();
    },
    [pump],
  );

  useEffect(() => {
    if (!visible) return;
    request(staleIds(idsRef.current, fetchedAt.current, Date.now(), FOCUS_MS));
    if (activeRef.current) request([activeRef.current], true);
    const timer = window.setInterval(() => {
      request(staleIds(idsRef.current, fetchedAt.current, Date.now()));
      void commands.workspaceRevisions().then(
        (revisions) => {
          const due = dueIds(idsRef.current, activeRef.current, revisions, seen.current, fetchedAt.current, Date.now());
          for (const id of due) seen.current.set(id, revisions[id]);
          request(due, true);
        },
        () => undefined,
      );
    }, ACTIVE_MS);
    return () => window.clearInterval(timer);
  }, [visible, request]);

  const idKey = ids.join("|");
  useEffect(() => {
    request(staleIds(idsRef.current, fetchedAt.current, Date.now()));
  }, [idKey, request]);

  useEffect(() => {
    if (activeId) request([activeId], true);
  }, [activeId, request]);

  useEffect(() => {
    if (focused) request(staleIds(idsRef.current, fetchedAt.current, Date.now(), FOCUS_MS));
  }, [focused, request]);

  const previous = useRef<Statuses | null>(null);
  useEffect(() => {
    if (previous.current) request(changedStatusIds(previous.current, statuses.workspaces), true);
    previous.current = statuses.workspaces;
  }, [statuses, request]);

  const forgetSummary = useCallback((id: string) => {
    fetchedAt.current.delete(id);
    setSummaries((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });
  }, []);

  return { summaries, forgetSummary };
}
