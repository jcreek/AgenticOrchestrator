import { describe, expect, it } from "vitest";
import { matchShortcut, shortcutLabel, shortcutHints, submitShortcut, terminalClipboard, type KeyLike } from "./shortcuts";

const key = (key: string, code: string, extra: Partial<KeyLike> = {}): KeyLike => ({
  key,
  code,
  ctrlKey: true,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...extra,
});

const windowsShortcut = (event: KeyLike, inTerminal = false) => matchShortcut(event, inTerminal, "other");

describe("matchShortcut", () => {
  it("maps the design's shortcuts", () => {
    expect(windowsShortcut(key("j", "KeyJ"))).toEqual({ type: "nextNeedingYou" });
    expect(windowsShortcut(key("3", "Digit3"))).toEqual({ type: "tab", index: 2 });
    expect(windowsShortcut(key("0", "Digit0"))).toEqual({ type: "overview" });
    expect(windowsShortcut(key("n", "KeyN"))).toEqual({ type: "newSession" });
    expect(windowsShortcut(key("w", "KeyW"))).toEqual({ type: "closeSession" });
    expect(windowsShortcut(key("\\", "Backslash"))).toEqual({ type: "cycleLayout" });
    expect(windowsShortcut(key("Tab", "Tab"))).toEqual({ type: "cycleTab", step: 1 });
    expect(windowsShortcut(key("Tab", "Tab", { shiftKey: true }))).toEqual({ type: "cycleTab", step: -1 });
  });

  it("leaves terminal keys alone", () => {
    expect(windowsShortcut(key("c", "KeyC"))).toBeNull();
    expect(windowsShortcut(key("v", "KeyV"))).toBeNull();
    expect(windowsShortcut(key("j", "KeyJ", { ctrlKey: false }))).toBeNull();
    expect(windowsShortcut(key("j", "KeyJ", { altKey: true }))).toBeNull();
    expect(windowsShortcut(key("N", "KeyN", { shiftKey: true }))).toBeNull();
  });

  it("passes Ctrl+W (delete word) to a focused terminal and keeps the other shortcuts there", () => {
    expect(windowsShortcut(key("w", "KeyW"), true)).toBeNull();
    expect(windowsShortcut(key("j", "KeyJ"), true)).toEqual({ type: "nextNeedingYou" });
    expect(windowsShortcut(key("n", "KeyN"), true)).toEqual({ type: "newSession" });
    expect(windowsShortcut(key("\\", "Backslash"), true)).toEqual({ type: "cycleLayout" });
    expect(windowsShortcut(key("1", "Digit1"), true)).toEqual({ type: "tab", index: 0 });
    expect(windowsShortcut(key("Tab", "Tab"), true)).toEqual({ type: "cycleTab", step: 1 });
  });
});

const cmd = (value: string, code = "") => key(value, code, { ctrlKey: false, metaKey: true });

it("uses Cmd on Mac while retaining Ctrl+Tab and terminal control sequences", () => {
  expect(matchShortcut(cmd("j"), false, "macos")).toEqual({ type: "nextNeedingYou" });
  expect(matchShortcut(cmd("3", "Digit3"), false, "macos")).toEqual({ type: "tab", index: 2 });
  expect(matchShortcut(cmd("w"), true, "macos")).toEqual({ type: "closeSession" });
  expect(matchShortcut(key("w", "KeyW"), true, "macos")).toBeNull();
  expect(matchShortcut(key("j", "KeyJ"), false, "macos")).toBeNull();
  expect(matchShortcut(cmd("Tab"), false, "macos")).toBeNull();
  expect(matchShortcut(key("Tab", "Tab"), true, "macos")).toEqual({ type: "cycleTab", step: 1 });
  expect(matchShortcut(key("Tab", "Tab", { shiftKey: true }), true, "macos")).toEqual({ type: "cycleTab", step: -1 });
  expect(matchShortcut(cmd("w"), false, "other")).toBeNull();
  expect(matchShortcut({ ...cmd("j"), ctrlKey: true }, false, "macos")).toBeNull();
});

it("formats platform labels and submits only with the platform modifier", () => {
  expect(shortcutLabel("Enter", "macos")).toBe("Cmd+Enter");
  expect(shortcutHints("Ctrl+J next · Ctrl+1–9 tab", "macos")).toBe("Cmd+J next · Cmd+1–9 tab");
  expect(shortcutLabel("Tab", "macos")).toBe("Ctrl+Tab");
  expect(shortcutLabel("Shift+Tab", "macos")).toBe("Ctrl+Shift+Tab");
  expect(shortcutLabel("N", "other")).toBe("Ctrl+N");
  expect(submitShortcut(cmd("Enter"), "macos")).toBe(true);
  expect(submitShortcut(key("Enter", "Enter"), "macos")).toBe(false);
  expect(submitShortcut(key("Enter", "Enter"), "other")).toBe(true);
  expect(submitShortcut({ ...cmd("Enter"), shiftKey: true }, "macos")).toBe(false);
  expect(submitShortcut({ ...cmd("Enter"), altKey: true }, "macos")).toBe(false);
});

it("keeps copy, paste and terminal interrupts distinct on each platform", () => {
  for (const [target, event] of [["macos", cmd("c")], ["other", key("c", "KeyC")]] as const) {
    expect(terminalClipboard(event, true, target)).toBe("copy");
    expect(terminalClipboard(event, false, target)).toBeNull();
    expect(terminalClipboard({ ...event, key: "v" }, false, target)).toBe("paste");
  }
  expect(terminalClipboard(key("c", "KeyC"), true, "macos")).toBeNull();
  expect(terminalClipboard(cmd("v"), false, "other")).toBeNull();
  expect(terminalClipboard(key("C", "KeyC", { shiftKey: true }), true, "other")).toBe("copy");
});
