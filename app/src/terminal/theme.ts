import type { ITheme } from "@xterm/xterm";

export function terminalTheme(): ITheme {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string) => style.getPropertyValue(name).trim();
  return {
    background: token("--color-terminal"),
    foreground: token("--color-text"),
    cursor: token("--color-text"),
    cursorAccent: token("--color-terminal"),
    selectionBackground: token("--color-selected"),
    red: token("--status-crashed"),
    green: token("--status-finished"),
    yellow: token("--status-input"),
    blue: token("--status-working"),
    magenta: token("--status-permission"),
    cyan: token("--color-accent"),
    brightBlack: token("--color-text-muted"),
  };
}

export function onThemeChange(listener: () => void): () => void {
  const query = window.matchMedia("(prefers-color-scheme: light)");
  query.addEventListener("change", listener);
  return () => query.removeEventListener("change", listener);
}
