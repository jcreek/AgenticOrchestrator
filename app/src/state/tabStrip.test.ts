import { describe, expect, it } from "vitest";
import { describeTab, hiddenTabs, isMerged, overflowCounts, revealScroll, tabText } from "./tabStrip";

const repoTab = { branch: "main", group: { key: "k", name: "web-console" } };
const folderTab = { branch: "notes", group: null };

describe("isMerged", () => {
  it("merges a group with one session and labels larger groups", () => {
    expect(isMerged({ items: [1] })).toBe(true);
    expect(isMerged({ items: [1, 2] })).toBe(false);
  });
});

describe("tabText", () => {
  it("leads a merged repo tab with the repo name and keeps the branch", () => {
    expect(tabText(repoTab, "web-console", true)).toEqual({ primary: "web-console", branch: "main" });
  });

  it("shows only the folder name for a folder that is not a repo", () => {
    expect(tabText(folderTab, "notes", true)).toEqual({ primary: "notes", branch: null });
  });

  it("leads with the branch under a group label", () => {
    expect(tabText(repoTab, "web-console", false)).toEqual({ primary: "main", branch: null });
  });
});

describe("describeTab", () => {
  it("joins repo, branch, status and time", () => {
    expect(describeTab(repoTab, "web-console", "Working", "2m")).toBe("web-console · main · Working · 2m");
  });

  it("leaves out the branch for folders and an empty time", () => {
    expect(describeTab(folderTab, "notes", "Not started", "")).toBe("notes · Not started");
  });
});

describe("hiddenTabs", () => {
  const spans = [
    { id: "a", start: 0, end: 100 },
    { id: "b", start: 102, end: 202 },
    { id: "c", start: 204, end: 304 },
  ];

  it("lists tabs cut off at either edge", () => {
    expect(hiddenTabs(spans, 50, 250)).toEqual(["a", "c"]);
  });

  it("tolerates a pixel of rounding", () => {
    expect(hiddenTabs(spans, 1, 305)).toEqual([]);
  });
});

describe("overflowCounts", () => {
  it("counts hidden sessions and the hidden ones that need you", () => {
    const tabs = [
      { id: "a", needsYou: true },
      { id: "b", needsYou: true },
      { id: "c", needsYou: false },
    ];
    expect(overflowCounts(["a", "c"], tabs)).toEqual({ hidden: 2, needYou: 1 });
    expect(overflowCounts([], tabs)).toEqual({ hidden: 0, needYou: 0 });
  });
});

describe("revealScroll", () => {
  it("leaves a fully shown tab alone", () => {
    expect(revealScroll(100, 200, 50, 300, 24)).toBeNull();
  });

  it("scrolls back to a tab before the view, clear of the fade", () => {
    expect(revealScroll(100, 200, 150, 300, 24)).toBe(76);
    expect(revealScroll(10, 110, 50, 300, 24)).toBe(0);
  });

  it("scrolls forward to a tab past the view", () => {
    expect(revealScroll(400, 500, 0, 300, 24)).toBe(224);
  });
});
