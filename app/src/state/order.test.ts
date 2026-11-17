import { describe, expect, it } from "vitest";
import { cycle, groupTabs, moveTab, nextNeedingYou, type NeedSource } from "./order";
import type { StatusKind } from "./status";

const ws = (id: string, group: string | null) => ({
  id,
  name: `name-${id}`,
  path: `C:\\dev\\${id}`,
  group: group ? { key: `key-${group}`, name: group } : null,
});

describe("groupTabs", () => {
  it("groups by repo in order of first appearance and keeps list order inside a group", () => {
    const groups = groupTabs([ws("a", "web"), ws("b", "api"), ws("c", "web"), ws("d", null)]);
    expect(groups.map((group) => group.name)).toEqual(["web", "api", "name-d"]);
    expect(groups[0].items.map((item) => item.id)).toEqual(["a", "c"]);
  });

  it("gives each folder that is not a repo its own group", () => {
    const groups = groupTabs([ws("x", null), ws("y", null)]);
    expect(groups.map((group) => group.key)).toEqual(["folder:C:\\dev\\x", "folder:C:\\dev\\y"]);
  });
});

describe("moveTab", () => {
  const groups = groupTabs([ws("a", "web"), ws("b", "web"), ws("c", "web"), ws("d", "api"), ws("e", null)]);

  it("moves a tab to the place of another tab in its group", () => {
    expect(moveTab(groups, "a", "c")).toEqual(["b", "c", "a", "d", "e"]);
    expect(moveTab(groups, "c", "a")).toEqual(["c", "a", "b", "d", "e"]);
  });

  it("moves the whole group when dropped on a tab of another group", () => {
    expect(moveTab(groups, "b", "e")).toEqual(["d", "e", "a", "b", "c"]);
    expect(moveTab(groups, "e", "b")).toEqual(["e", "a", "b", "c", "d"]);
  });

  it("changes nothing when dropped on itself or when a tab has gone", () => {
    expect(moveTab(groups, "b", "b")).toEqual(["a", "b", "c", "d", "e"]);
    expect(moveTab(groups, "b", "gone")).toEqual(["a", "b", "c", "d", "e"]);
    expect(moveTab(groups, "gone", "b")).toEqual(["a", "b", "c", "d", "e"]);
  });
});

const tab = (id: string, kind: StatusKind, needsYou = kind === "input" || kind === "permission"): NeedSource => ({
  id,
  kind,
  needsYou,
});

describe("nextNeedingYou", () => {
  const tabs = [
    tab("w1", "working"),
    tab("f1", "finished"),
    tab("i1", "input"),
    tab("p1", "permission"),
    tab("i2", "input"),
    tab("p2", "permission"),
  ];

  it("orders permission, then input, then finished, each in tab order", () => {
    const visited: (string | null)[] = [];
    let current: string | null = null;
    for (let step = 0; step < 6; step++) {
      current = nextNeedingYou(tabs, current);
      visited.push(current);
    }
    expect(visited).toEqual(["p1", "p2", "i1", "i2", "f1", "p1"]);
  });

  it("starts from the top when the current tab does not need you", () => {
    expect(nextNeedingYou(tabs, "w1")).toBe("p1");
  });

  it("ignores input-styled tabs that do not need you yet", () => {
    expect(nextNeedingYou([tab("s", "input", false), tab("w", "working")], null)).toBeNull();
  });

  it("stays on the only tab that needs you", () => {
    expect(nextNeedingYou([tab("p", "permission")], "p")).toBe("p");
  });
});

describe("cycle", () => {
  it("wraps both ways", () => {
    expect(cycle(2, 3, 1)).toBe(0);
    expect(cycle(0, 3, -1)).toBe(2);
  });
});
