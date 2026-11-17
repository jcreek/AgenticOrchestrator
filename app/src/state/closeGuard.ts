import type { StatusChanged_Serialize } from "../bindings";

export interface CloseGuard {
  running: number;
  unsaved: number;
}

/** What closing the app would stop or lose; null when it can close at once. */
export function closeGuard(statuses: StatusChanged_Serialize["workspaces"], dirty: Record<string, boolean>): CloseGuard | null {
  const running = Object.values(statuses).filter((entry) => entry.status !== "exited").length;
  const unsaved = Object.values(dirty).filter(Boolean).length;
  return running > 0 || unsaved > 0 ? { running, unsaved } : null;
}
