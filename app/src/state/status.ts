import { KILLED_EXIT_CODE, type WorkspaceStatus_Serialize } from "../bindings";

export type StatusKind = "working" | "input" | "permission" | "finished" | "crashed" | "idle" | "unknown";

export interface Presentation {
  kind: StatusKind;
  /** Full label, as on the Overview and in aria labels. */
  label: string;
  /** Short label for the tab status line. */
  tabLabel: string;
  colorVar: string;
  /** Flashes until the tab is opened, and counts towards "need you". */
  needsYou: boolean;
  /** Flashes once when entered. */
  flashOnce: boolean;
  /** Epoch ms when the state began; null when there is no session. */
  since: number | null;
}

export const STARTING_GRACE_MS = 10_000;

export const STATUS_COLOR: Record<StatusKind, string> = {
  working: "var(--status-working)",
  input: "var(--status-input)",
  permission: "var(--status-permission)",
  finished: "var(--status-finished)",
  crashed: "var(--status-crashed)",
  idle: "var(--status-idle)",
  unknown: "var(--status-unknown)",
};

function make(
  kind: StatusKind,
  label: string,
  tabLabel: string,
  since: number | null,
  needsYou = false,
  flashOnce = false,
): Presentation {
  return { kind, label, tabLabel, colorVar: STATUS_COLOR[kind], needsYou, flashOnce, since };
}

export function isCrash(exitCode: number | null): boolean {
  return exitCode !== null && exitCode !== 0 && exitCode !== KILLED_EXIT_CODE;
}

export function presentStatus(
  entry: WorkspaceStatus_Serialize | undefined,
  unavailable: boolean,
  now: number,
): Presentation {
  if (!entry) return make("idle", "Not started", "Not started", null);
  const { status, since, exitCode } = entry;
  if (status === "exited") {
    if (isCrash(exitCode)) return make("crashed", `Crashed · exit ${exitCode}`, "Crashed", since, false, true);
    return make("idle", "Exited", "Exited", since);
  }
  if (unavailable) return make("unknown", "Status unknown", "Unknown", since);
  switch (status) {
    case "working":
      return make("working", "Working", "Working", since);
    case "needsInput":
      return make("input", "Waiting for input", "Input", since, true);
    case "needsPermission":
      return make("permission", "Needs permission", "Permission", since, true);
    case "doneUnseen":
      return make("finished", "Finished", "Finished", since, false, true);
    case "idle":
      return make("idle", "Idle", "Idle", since);
    case "starting":
      // A new folder stops at Claude's trust prompt, which the status source cannot see.
      return now - since < STARTING_GRACE_MS
        ? make("input", "Starting", "Starting", since)
        : make("input", "Check terminal", "Check terminal", since, true);
  }
}

export function presentMissingFolder(): Presentation {
  return make("unknown", "Folder not found", "Unavailable", null);
}

export function formatInState(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h`;
}

export function formatAgo(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
}
