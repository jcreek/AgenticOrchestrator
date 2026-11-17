export type LayoutMode = "terminal" | "both" | "ide";

export interface Layout {
  mode: LayoutMode;
  stacked: boolean;
  swapped: boolean;
  /** Share of the split given to the terminal, 0.15 to 0.85. */
  terminalShare: number;
}

export const DEFAULT_LAYOUT: Layout = { mode: "both", stacked: false, swapped: false, terminalShare: 0.45 };
export const MIN_SHARE = 0.15;
export const MAX_SHARE = 0.85;

const MODES: LayoutMode[] = ["terminal", "both", "ide"];

export function nextMode(mode: LayoutMode): LayoutMode {
  return MODES[(MODES.indexOf(mode) + 1) % MODES.length];
}

export function clampShare(share: number): number {
  return Math.min(MAX_SHARE, Math.max(MIN_SHARE, share));
}

const key = (workspaceId: string) => `ao.layout.${workspaceId}`;

export function loadLayout(workspaceId: string, defaultMode: LayoutMode): Layout {
  const unsaved = { ...DEFAULT_LAYOUT, mode: defaultMode };
  try {
    const saved = JSON.parse(localStorage.getItem(key(workspaceId)) ?? "null") as Partial<Layout> | null;
    if (!saved || !MODES.includes(saved.mode as LayoutMode)) return unsaved;
    return {
      mode: saved.mode as LayoutMode,
      stacked: saved.stacked === true,
      swapped: saved.swapped === true,
      terminalShare: clampShare(Number(saved.terminalShare) || DEFAULT_LAYOUT.terminalShare),
    };
  } catch {
    return unsaved;
  }
}

export function saveLayout(workspaceId: string, layout: Layout): void {
  try {
    localStorage.setItem(key(workspaceId), JSON.stringify(layout));
  } catch {
    // Storage can be unavailable; the layout then lasts for this run only.
  }
}

export function forgetLayout(workspaceId: string): void {
  try {
    localStorage.removeItem(key(workspaceId));
  } catch {
    // Nothing to clean up when storage is unavailable.
  }
}
