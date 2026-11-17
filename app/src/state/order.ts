import type { StatusKind } from "./status";

export interface TabSource {
  id: string;
  name: string;
  path: string;
  group: { key: string; name: string } | null;
}

export interface TabGroup<T extends TabSource> {
  key: string;
  name: string;
  items: T[];
}

/** Groups tabs by repo, in the order each repo first appears; tabs keep their list order. */
export function groupTabs<T extends TabSource>(sources: T[]): TabGroup<T>[] {
  const groups = new Map<string, TabGroup<T>>();
  for (const source of sources) {
    const key = source.group?.key ?? `folder:${source.path}`;
    const group = groups.get(key) ?? { key, name: source.group?.name ?? source.name, items: [] };
    group.items.push(source);
    groups.set(key, group);
  }
  return [...groups.values()];
}

function moved<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  next.splice(to, 0, ...next.splice(from, 1));
  return next;
}

/** Tab ids after dropping one tab on another: inside a repo group the tab moves, across groups its whole group does. */
export function moveTab<T extends TabSource>(groups: TabGroup<T>[], draggedId: string, targetId: string): string[] {
  const ids = groups.map((group) => group.items.map((tab) => tab.id));
  const from = ids.findIndex((group) => group.includes(draggedId));
  const to = ids.findIndex((group) => group.includes(targetId));
  if (from < 0 || to < 0) return ids.flat();
  if (from !== to) return moved(ids, from, to).flat();
  ids[from] = moved(ids[from], ids[from].indexOf(draggedId), ids[from].indexOf(targetId));
  return ids.flat();
}

const PRIORITY: Partial<Record<StatusKind, number>> = { permission: 0, input: 1, finished: 2 };

export interface NeedSource {
  id: string;
  kind: StatusKind;
  needsYou: boolean;
}

function priority(tab: NeedSource): number | undefined {
  if (tab.kind === "finished") return PRIORITY.finished;
  return tab.needsYou ? PRIORITY[tab.kind] : undefined;
}

/**
 * The tab Ctrl+J goes to: permission first, then input, then finished-unseen, each in tab order,
 * moving past the current tab and wrapping. Null when nothing needs you.
 */
export function nextNeedingYou(tabs: NeedSource[], currentId: string | null): string | null {
  const queue = tabs
    .map((tab, index) => ({ tab, index, rank: priority(tab) }))
    .filter((entry) => entry.rank !== undefined)
    .sort((a, b) => a.rank! - b.rank! || a.index - b.index)
    .map((entry) => entry.tab.id);
  if (queue.length === 0) return null;
  const at = currentId === null ? -1 : queue.indexOf(currentId);
  return queue[(at + 1) % queue.length];
}

/** Next or previous position in a cycle of `count` (Overview is position 0). */
export function cycle(index: number, count: number, step: 1 | -1): number {
  return (index + step + count) % count;
}
