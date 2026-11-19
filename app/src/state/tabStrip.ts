/** A repo or folder with one session is shown as a single tab with no group label. */
export function isMerged(group: { items: unknown[] }): boolean {
  return group.items.length === 1;
}

/** A collapsed group still shows its active tab, so the session in use never leaves the strip. */
export function shownTabs<T extends { id: string }>(items: T[], collapsed: boolean, activeId: string | null): T[] {
  return collapsed ? items.filter((tab) => tab.id === activeId) : items;
}

export function collapsedHidden(
  groups: { key: string; items: { id: string }[] }[],
  collapsed: ReadonlySet<string>,
  activeId: string | null,
): string[] {
  return groups.flatMap((group) => {
    if (!collapsed.has(group.key)) return [];
    const shown = new Set(shownTabs(group.items, true, activeId));
    return group.items.filter((tab) => !shown.has(tab)).map((tab) => tab.id);
  });
}

export interface TextSource {
  branch: string;
  group: unknown;
}

export interface TabText {
  primary: string;
  /** Branch shown before the status on a merged tab; null for folders and labelled tabs. */
  branch: string | null;
}

/** Merged tabs lead with the repo or folder name; tabs under a group label lead with their branch. */
export function tabText(tab: TextSource, groupName: string, merged: boolean): TabText {
  if (!merged) return { primary: tab.branch, branch: null };
  return { primary: groupName, branch: tab.group ? tab.branch : null };
}

/** Full text for a tab's tooltip and accessible name. */
export function describeTab(tab: TextSource, groupName: string, status: string, time: string): string {
  return [groupName, tab.group ? tab.branch : "", status, time].filter(Boolean).join(" · ");
}

export interface Span {
  id: string;
  start: number;
  end: number;
}

/** Ids of tabs not fully inside the visible part of the strip (1 px slack for rounding). */
export function hiddenTabs(spans: Span[], viewStart: number, viewEnd: number): string[] {
  return spans.filter((span) => span.start < viewStart - 1 || span.end > viewEnd + 1).map((span) => span.id);
}

export function overflowCounts(hidden: string[], tabs: { id: string; needsYou: boolean }[]): { hidden: number; needYou: number } {
  const ids = new Set(hidden);
  return { hidden: ids.size, needYou: tabs.filter((tab) => tab.needsYou && ids.has(tab.id)).length };
}

/**
 * The scroll position that shows [start, end] fully, clear of `margin` at each edge,
 * or null when it is already shown.
 */
export function revealScroll(start: number, end: number, scrollLeft: number, width: number, margin: number): number | null {
  if (start < scrollLeft + margin) return Math.max(0, start - margin);
  if (end > scrollLeft + width - margin) return end - width + margin;
  return null;
}
