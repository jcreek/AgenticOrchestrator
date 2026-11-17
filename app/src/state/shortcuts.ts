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

/** App-level shortcuts; these win over the terminal, except Ctrl+W, which shells and Claude use to delete a word. */
export function matchShortcut(event: KeyLike, inTerminal = false): Shortcut | null {
  if (!event.ctrlKey || event.altKey || event.metaKey) return null;
  if (event.key === "Tab") return { type: "cycleTab", step: event.shiftKey ? -1 : 1 };
  if (event.shiftKey) return null;
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
      return inTerminal ? null : { type: "closeSession" };
    default:
      return null;
  }
}
