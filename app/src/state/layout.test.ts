import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadLayout } from "./layout";

describe("loading a tab's layout", () => {
  const saved = new Map<string, string>();

  beforeEach(() => {
    saved.clear();
    vi.stubGlobal("localStorage", { getItem: (key: string) => saved.get(key) ?? null });
  });

  it("uses the default mode when the tab has no saved layout", () => {
    expect(loadLayout("w1", "ide").mode).toBe("ide");
  });

  it("keeps the tab's saved mode over the default", () => {
    saved.set("ao.layout.w1", JSON.stringify({ mode: "terminal", terminalShare: 0.5 }));
    expect(loadLayout("w1", "ide").mode).toBe("terminal");
  });
});
