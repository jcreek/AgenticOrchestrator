import { describe, expect, it } from "vitest";
import type { FileKind } from "../bindings";
import {
  type ChangeRow,
  changeTree,
  formatCounts,
  fromBranch,
  rowFor,
  rowKey,
  stageState,
  stageToggle,
  statusLetter,
  statusTone,
  totals,
} from "./status";

const row = (overrides: Partial<ChangeRow> = {}): ChangeRow => ({
  path: "src/a.ts",
  kind: "modified",
  originalPath: null,
  added: null,
  removed: null,
  staged: false,
  unstaged: true,
  ...overrides,
});

describe("rows for a file that git rm --cached left on disk", () => {
  const rows = [
    row({ path: "notes.md", kind: "deleted", staged: true, unstaged: false }),
    row({ path: "notes.md", kind: "untracked" }),
  ];

  it("opens the untracked row, not the deleted one", () => {
    expect(rowFor(rows, "notes.md")?.kind).toBe("untracked");
    expect(rowFor([rows[0]], "notes.md")?.kind).toBe("deleted");
    expect(rowFor(rows, "other.md")).toBeNull();
    expect(rowFor(rows, null)).toBeNull();
  });

  it("keys each row uniquely", () => {
    expect(new Set(rows.map(rowKey)).size).toBe(rows.length);
  });
});

describe("changeTree", () => {
  const rows = [row({ path: "readme.md" }), row({ path: "src/b.ts" }), row({ path: "src/ide/a.ts" }), row({ path: "docs/x.md" })];
  const shape = (collapsed: string[]) =>
    changeTree(rows, collapsed).map((item) => `${"  ".repeat(item.depth)}${item.type === "folder" ? `${item.name}/` : item.name}`);

  it("nests files under their folders, folders before files", () => {
    expect(shape([])).toEqual(["docs/", "  x.md", "src/", "  ide/", "    a.ts", "  b.ts", "readme.md"]);
  });

  it("leaves out what is inside a collapsed folder", () => {
    expect(shape(["src"])).toEqual(["docs/", "  x.md", "src/", "readme.md"]);
  });

  it("merges a chain of folders that each hold only the next into one row", () => {
    const deep = [row({ path: "app/src/ide/a.ts" }), row({ path: "app/src/ide/css/b.css" }), row({ path: "lib/x/y.ts" })];
    const names = changeTree(deep, ["lib/x"]).map((item) => `${"  ".repeat(item.depth)}${item.name}`);
    expect(names).toEqual(["app/src/ide", "  css", "    b.css", "  a.ts", "lib/x"]);
  });

  it("keeps both rows of a file listed as deleted and untracked", () => {
    const pair = [row({ path: "src/n.md", kind: "deleted" }), row({ path: "src/n.md", kind: "untracked" })];
    expect(changeTree(pair, []).filter((item) => item.type === "file")).toHaveLength(2);
  });
});

describe("status letters and tones", () => {
  it.each<[FileKind, string, string]>([
    ["modified", "M", "modified"],
    ["added", "A", "added"],
    ["deleted", "D", "deleted"],
    ["renamed", "R", "renamed"],
    ["untracked", "U", "added"],
    ["conflicted", "C", "conflicted"],
  ])("%s is %s in the %s tone", (kind, letter, tone) => {
    expect(statusLetter(kind)).toBe(letter);
    expect(statusTone(kind)).toBe(tone);
  });
});

describe("totals and counts", () => {
  it("sums known counts and skips untracked or binary files", () => {
    const rows = [row({ added: 42, removed: 9 }), row({ added: 96, removed: 0 }), row({ kind: "untracked" })];
    expect(totals(rows)).toEqual({ added: 138, removed: 9 });
  });

  it("formats counts as drawn", () => {
    expect(formatCounts(42, 9)).toBe("+42 −9");
    expect(formatCounts(96, 0)).toBe("+96");
    expect(formatCounts(0, 48)).toBe("−48");
    expect(formatCounts(null, null)).toBe("");
  });

  it("branch rows carry no counts", () => {
    expect(fromBranch({ path: "a", kind: "added", originalPath: null })).toMatchObject({ added: null, removed: null });
  });
});

describe("staging", () => {
  it("derives the marker state", () => {
    expect(stageState(row())).toBe("unchecked");
    expect(stageState(row({ staged: true, unstaged: false }))).toBe("checked");
    expect(stageState(row({ staged: true, unstaged: true }))).toBe("mixed");
  });

  it("stages partially staged files and unstages fully staged ones", () => {
    expect(stageToggle(row({ staged: true, unstaged: true }))).toEqual({ action: "stage", paths: ["src/a.ts"] });
    expect(stageToggle(row({ staged: true, unstaged: false }))).toEqual({ action: "unstage", paths: ["src/a.ts"] });
  });

  it("unstages both sides of a rename", () => {
    const renamed = row({ kind: "renamed", originalPath: "src/old.ts", staged: true, unstaged: false });
    expect(stageToggle(renamed).paths).toEqual(["src/a.ts", "src/old.ts"]);
  });
});
