import type { BranchFile_Serialize, ChangedFile_Serialize, FileKind } from "../bindings";

export type Tone = "modified" | "added" | "deleted" | "renamed" | "conflicted";

export interface ChangeRow {
  path: string;
  kind: FileKind;
  originalPath: string | null;
  added: number | null;
  removed: number | null;
  /** Undefined for "This branch" rows, which cannot be staged from the list. */
  staged?: boolean;
  unstaged?: boolean;
}

const LETTERS: Record<FileKind, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflicted: "C",
};

const TONES: Record<FileKind, Tone> = {
  modified: "modified",
  added: "added",
  deleted: "deleted",
  renamed: "renamed",
  untracked: "added",
  conflicted: "conflicted",
};

const NAMES: Record<FileKind, string> = {
  modified: "Modified",
  added: "Added",
  deleted: "Deleted",
  renamed: "Renamed",
  untracked: "Untracked",
  conflicted: "Conflicted",
};

export const statusLetter = (kind: FileKind) => LETTERS[kind];
export const statusTone = (kind: FileKind) => TONES[kind];
export const statusName = (kind: FileKind) => NAMES[kind];

export const fromStatus = (file: ChangedFile_Serialize): ChangeRow => ({ ...file });

export const fromBranch = (file: BranchFile_Serialize): ChangeRow => ({
  ...file,
  added: null,
  removed: null,
});

/** The row for `path`, preferring a non-deleted one: `git rm --cached` lists a file both deleted and untracked. */
export function rowFor(rows: ChangeRow[], path: string | null): ChangeRow | null {
  const matches = rows.filter((row) => row.path === path);
  return matches.find((row) => row.kind !== "deleted") ?? matches[0] ?? null;
}

export const rowKey = (row: ChangeRow) => `${row.kind}:${row.path}`;

export type ChangeItem =
  | { type: "folder"; path: string; name: string; depth: number; expanded: boolean }
  | { type: "file"; row: ChangeRow; name: string; depth: number };

export const flatItems = (rows: ChangeRow[]): ChangeItem[] => rows.map((row) => ({ type: "file", row, name: row.path, depth: 0 }));

/** The rows as a folder tree in display order, folders before files; what is inside a `collapsed` folder is left out. */
export function changeTree(rows: ChangeRow[], collapsed: string[]): ChangeItem[] {
  const items: ChangeItem[] = [];
  const paths = rows.map((row) => row.path);
  const walk =(prefix: string, depth: number) => {
    const inside = rows
      .filter((row) => row.path.startsWith(prefix))
      .map((row) => ({ row, name: row.path.slice(prefix.length) }))
      .sort((a, b) => a.name.localeCompare(b.name));
    const folders = new Set(inside.filter((entry) => entry.name.includes("/")).map((entry) => entry.name.split("/")[0]));
    for (const first of folders) {
      let path = prefix + first;
      for (;;) {
        const below = paths.filter((other) => other.startsWith(`${path}/`)).map((other) => other.slice(path.length + 1));
        const onlyFolder = `${below[0].split("/")[0]}/`;
        if (!below.every((rest) => rest.startsWith(onlyFolder))) break;
        path += `/${onlyFolder.slice(0, -1)}`;
      }
      const expanded = !collapsed.includes(path);
      items.push({ type: "folder", path, name: path.slice(prefix.length), depth, expanded });
      if (expanded) walk(`${path}/`, depth + 1);
    }
    for (const { row, name } of inside) if (!name.includes("/")) items.push({ type: "file", row, name, depth });
  };
  walk("", 0);
  return items;
}

export function totals(rows: ChangeRow[]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const row of rows) {
    added += row.added ?? 0;
    removed += row.removed ?? 0;
  }
  return { added, removed };
}

/** "+42 −9", "+96", "−48", or "" when there is nothing to count. */
export function formatCounts(added: number | null, removed: number | null): string {
  const parts: string[] = [];
  if (added) parts.push(`+${added}`);
  if (removed) parts.push(`−${removed}`);
  return parts.join(" ");
}

export type StageState = "checked" | "mixed" | "unchecked";

export function stageState(row: ChangeRow): StageState {
  if (!row.staged) return "unchecked";
  return row.unstaged ? "mixed" : "checked";
}

/** Fully staged rows unstage; anything else stages its remaining changes. */
export function stageToggle(row: ChangeRow): { action: "stage" | "unstage"; paths: string[] } {
  if (stageState(row) === "checked") {
    const paths = row.originalPath ? [row.path, row.originalPath] : [row.path];
    return { action: "unstage", paths };
  }
  return { action: "stage", paths: [row.path] };
}

export const hasStaged = (rows: ChangeRow[]) => rows.some((row) => row.staged);
