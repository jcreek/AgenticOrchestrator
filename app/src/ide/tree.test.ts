import { describe, expect, it } from "vitest";
import type { DirEntry } from "../bindings";
import { emptyTree, foldersToRefresh, mergeListing, setExpanded, treeKey, visibleRows } from "./tree";

const dir = (relPath: string): DirEntry => ({ relPath, name: relPath.split("/").pop()!, isDir: true });
const file = (relPath: string): DirEntry => ({ relPath, name: relPath.split("/").pop()!, isDir: false });

const loaded = () => {
  let tree = mergeListing(emptyTree, "", [dir("src"), dir("docs"), file("README.md")]);
  tree = setExpanded(tree, "src", true);
  tree = mergeListing(tree, "src", [dir("src/ide"), file("src/main.ts")]);
  tree = setExpanded(tree, "src/ide", true);
  return mergeListing(tree, "src/ide", [file("src/ide/IdePane.tsx")]);
};

describe("tree rows", () => {
  it("flattens expanded folders depth-first in listing order", () => {
    expect(visibleRows(loaded()).map((row) => `${row.depth}:${row.entry.relPath}`)).toEqual([
      "0:src",
      "1:src/ide",
      "2:src/ide/IdePane.tsx",
      "1:src/main.ts",
      "0:docs",
      "0:README.md",
    ]);
  });

  it("hides children of collapsed folders but keeps their listing", () => {
    const tree = setExpanded(loaded(), "src", false);
    expect(visibleRows(tree).map((row) => row.entry.relPath)).toEqual(["src", "docs", "README.md"]);
    expect(tree.listings["src/ide"]).toBeDefined();
  });

  it("refreshes the root and every expanded folder", () => {
    expect(foldersToRefresh(loaded())).toEqual(["", "src", "src/ide"]);
  });
});

describe("merging listings", () => {
  it("forgets folders that disappeared, with everything under them", () => {
    const tree = mergeListing(loaded(), "", [dir("docs"), file("README.md")]);
    expect(tree.expanded).toEqual([]);
    expect(Object.keys(tree.listings)).toEqual([""]);
  });

  it("keeps state for folders that still exist", () => {
    const tree = mergeListing(loaded(), "src", [dir("src/ide"), file("src/other.ts")]);
    expect(tree.expanded).toEqual(["src", "src/ide"]);
    expect(visibleRows(tree).map((row) => row.entry.relPath)).toContain("src/ide/IdePane.tsx");
  });

  it("does not confuse a sibling with a shared prefix", () => {
    let tree = mergeListing(emptyTree, "", [dir("a"), dir("ab")]);
    tree = setExpanded(setExpanded(tree, "a", true), "ab", true);
    tree = mergeListing(tree, "", [dir("ab")]);
    expect(tree.expanded).toEqual(["ab"]);
  });
});

describe("tree keyboard", () => {
  const rows = visibleRows(loaded());

  it("moves with arrows and clamps at the ends", () => {
    expect(treeKey(rows, 0, "ArrowDown")).toEqual({ type: "focus", index: 1 });
    expect(treeKey(rows, 0, "ArrowUp")).toEqual({ type: "focus", index: 0 });
    expect(treeKey(rows, 5, "ArrowDown")).toEqual({ type: "focus", index: 5 });
  });

  it("expands, enters and collapses folders", () => {
    expect(treeKey(rows, 4, "ArrowRight")).toEqual({ type: "expand", path: "docs" });
    expect(treeKey(rows, 0, "ArrowRight")).toEqual({ type: "focus", index: 1 });
    expect(treeKey(rows, 0, "ArrowLeft")).toEqual({ type: "collapse", path: "src" });
    expect(treeKey(rows, 0, "Enter")).toEqual({ type: "collapse", path: "src" });
  });

  it("goes to the parent from a child and opens files on Enter", () => {
    expect(treeKey(rows, 3, "ArrowLeft")).toEqual({ type: "focus", index: 0 });
    expect(treeKey(rows, 2, "Enter")).toEqual({ type: "open", path: "src/ide/IdePane.tsx" });
    expect(treeKey(rows, 2, "ArrowRight")).toBeNull();
  });
});
