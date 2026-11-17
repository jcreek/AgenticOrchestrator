import { platform, type Platform } from "./platform";

export type Shortcut =
  | { type: "nextNeedingYou" }
  | { type: "tab"; index: number }
  | { type: "overview" }
  | { type: "newSession" }
  | { type: "closeSession" }
  | { type: "cycleLayout" }
  | { type: "cycleTab"; step: 1 | -1 };

export interface KeyLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** App shortcuts use the platform modifier; terminal Ctrl+W remains word deletion. */
export function matchShortcut(event: KeyLike, inTerminal = false, target: Platform = platform): Shortcut | null {
  if (event.altKey) return null;
  if (event.key === "Tab" && event.ctrlKey && !event.metaKey) return { type: "cycleTab", step: event.shiftKey ? -1 : 1 };
  if (!primaryModifier(event, target) || event.shiftKey) return null;
  const digit = /^Digit(\d)$/.exec(event.code)?.[1] ?? (/^\d$/.test(event.key) ? event.key : null);
  if (digit === "0") return { type: "overview" };
  if (digit) return { type: "tab", index: Number(digit) - 1 };
  if (event.code === "Backslash" || event.key === "\\") return { type: "cycleLayout" };
  switch (event.key.toLowerCase()) {
    case "j":
      return { type: "nextNeedingYou" };
    case "n":
      return { type: "newSession" };
    case "w":
      return inTerminal && target !== "macos" ? null : { type: "closeSession" };
    default:
      return null;
  }
}

/** Exactly one platform modifier, leaving control sequences intact on macOS. */
export function primaryModifier(event: Pick<KeyLike, "ctrlKey" | "metaKey" | "altKey">, target: Platform = platform): boolean {
  return !event.altKey && (target === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey);
}

export function submitShortcut(event: KeyLike, target: Platform = platform): boolean {
  return event.key === "Enter" && !event.shiftKey && primaryModifier(event, target);
}

export function shortcutLabel(key: string, target: Platform = platform): string {
  return `${target === "macos" && key !== "Tab" && key !== "Shift+Tab" ? "Cmd" : "Ctrl"}+${key}`;
}

export function shortcutHints(text: string, target: Platform = platform): string {
  return text.replace(/Ctrl\+([^ ·)]+)/g, (_, key: string) => shortcutLabel(key, target));
}

export function terminalClipboard(event: KeyLike, selection: boolean, target: Platform = platform): "copy" | "paste" | null {
  if (!primaryModifier(event, target)) return null;
  if (event.key.toLowerCase() === "v" && !event.shiftKey) return "paste";
  if (event.key.toLowerCase() === "c" && selection && (target !== "macos" || !event.shiftKey)) return "copy";
  return null;
}
