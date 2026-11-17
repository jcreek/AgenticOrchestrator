import { describe, expect, it } from "vitest";
import { matchShortcut, type KeyLike } from "./shortcuts";

const key = (key: string, code: string, extra: Partial<KeyLike> = {}): KeyLike => ({
  key,
  code,
  ctrlKey: true,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...extra,
});

describe("matchShortcut", () => {
  it("maps the design's shortcuts", () => {
    expect(matchShortcut(key("j", "KeyJ"))).toEqual({ type: "nextNeedingYou" });
    expect(matchShortcut(key("3", "Digit3"))).toEqual({ type: "tab", index: 2 });
    expect(matchShortcut(key("0", "Digit0"))).toEqual({ type: "overview" });
    expect(matchShortcut(key("n", "KeyN"))).toEqual({ type: "newSession" });
    expect(matchShortcut(key("w", "KeyW"))).toEqual({ type: "closeSession" });
    expect(matchShortcut(key("\\", "Backslash"))).toEqual({ type: "cycleLayout" });
    expect(matchShortcut(key("Tab", "Tab"))).toEqual({ type: "cycleTab", step: 1 });
    expect(matchShortcut(key("Tab", "Tab", { shiftKey: true }))).toEqual({ type: "cycleTab", step: -1 });
  });

  it("leaves terminal keys alone", () => {
    expect(matchShortcut(key("c", "KeyC"))).toBeNull();
    expect(matchShortcut(key("v", "KeyV"))).toBeNull();
    expect(matchShortcut(key("j", "KeyJ", { ctrlKey: false }))).toBeNull();
    expect(matchShortcut(key("j", "KeyJ", { altKey: true }))).toBeNull();
    expect(matchShortcut(key("N", "KeyN", { shiftKey: true }))).toBeNull();
  });

  it("passes Ctrl+W (delete word) to a focused terminal and keeps the other shortcuts there", () => {
    expect(matchShortcut(key("w", "KeyW"), true)).toBeNull();
    expect(matchShortcut(key("j", "KeyJ"), true)).toEqual({ type: "nextNeedingYou" });
    expect(matchShortcut(key("n", "KeyN"), true)).toEqual({ type: "newSession" });
    expect(matchShortcut(key("\\", "Backslash"), true)).toEqual({ type: "cycleLayout" });
    expect(matchShortcut(key("1", "Digit1"), true)).toEqual({ type: "tab", index: 0 });
    expect(matchShortcut(key("Tab", "Tab"), true)).toEqual({ type: "cycleTab", step: 1 });
  });
});
