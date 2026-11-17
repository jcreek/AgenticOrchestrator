import type { DirEntry } from "../bindings";

export const ROOT = "";

export interface TreeState {
  /** Listings by folder relPath; a missing key means not loaded yet. */
  listings: Record<string, DirEntry[]>;
  expanded: string[];
}

export interface TreeRow {
  entry: DirEntry;
  depth: number;
  expanded: boolean;
}

export const emptyTree: TreeState = { listings: {}, expanded: [] };

const isUnder = (path: string, folder: string) => path.startsWith(`${folder}/`);

/** Stores a fresh listing and forgets anything below folders that no longer exist. */
export function mergeListing(tree: TreeState, folder: string, entries: DirEntry[]): TreeState {
  const listings = { ...tree.listings, [folder]: entries };
  const previous = tree.listings[folder] ?? [];
  const kept = new Set(entries.filter((entry) => entry.isDir).map((entry) => entry.relPath));
  const gone = previous.filter((entry) => entry.isDir && !kept.has(entry.relPath)).map((entry) => entry.relPath);
  const isGone = (path: string) => gone.some((folderGone) => path === folderGone || isUnder(path, folderGone));
  for (const key of Object.keys(listings)) if (isGone(key)) delete listings[key];
  return { listings, expanded: tree.expanded.filter((path) => !isGone(path)) };
}

export function setExpanded(tree: TreeState, folder: string, open: boolean): TreeState {
  const without = tree.expanded.filter((path) => path !== folder);
  return { ...tree, expanded: open ? [...without, folder] : without };
}

/** Folders whose listing should be (re)fetched: the root and every expanded folder. */
export const foldersToRefresh = (tree: TreeState) => [ROOT, ...tree.expanded];

export function visibleRows(tree: TreeState): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (folder: string, depth: number) => {
    for (const entry of tree.listings[folder] ?? []) {
      const expanded = entry.isDir && tree.expanded.includes(entry.relPath);
      rows.push({ entry, depth, expanded });
      if (expanded) walk(entry.relPath, depth + 1);
    }
  };
  walk(ROOT, 0);
  return rows;
}

export type TreeKeyResult =
  | { type: "focus"; index: number }
  | { type: "expand"; path: string }
  | { type: "collapse"; path: string }
  | { type: "open"; path: string }
  | null;

export function treeKey(rows: TreeRow[], index: number, key: string): TreeKeyResult {
  const row = rows[index];
  if (!row) return rows.length ? { type: "focus", index: 0 } : null;
  switch (key) {
    case "ArrowDown":
      return { type: "focus", index: Math.min(index + 1, rows.length - 1) };
    case "ArrowUp":
      return { type: "focus", index: Math.max(index - 1, 0) };
    case "Home":
      return { type: "focus", index: 0 };
    case "End":
      return { type: "focus", index: rows.length - 1 };
    case "ArrowRight":
      if (!row.entry.isDir) return null;
      return row.expanded ? { type: "focus", index: Math.min(index + 1, rows.length - 1) } : { type: "expand", path: row.entry.relPath };
    case "ArrowLeft": {
      if (row.expanded) return { type: "collapse", path: row.entry.relPath };
      for (let at = index - 1; at >= 0; at--) {
        if (rows[at].depth < row.depth) return { type: "focus", index: at };
      }
      return null;
    }
    case "Enter":
      if (!row.entry.isDir) return { type: "open", path: row.entry.relPath };
      return row.expanded ? { type: "collapse", path: row.entry.relPath } : { type: "expand", path: row.entry.relPath };
    default:
      return null;
  }
}
