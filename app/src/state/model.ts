import type { Workspace_Serialize, WorkspaceSummary_Serialize } from "../bindings";
import type { Presentation } from "./status";

export type Workspace = Workspace_Serialize;
export type Summary = WorkspaceSummary_Serialize;

export interface SessionTab {
  id: string;
  name: string;
  path: string;
  group: { key: string; name: string } | null;
  workspace: Workspace;
  summary: Summary | undefined;
  branch: string;
  presentation: Presentation;
  /** 1 to 9 for the Ctrl+number shortcut; null past the ninth tab. */
  number: number | null;
}

export type ClaudeSource = { attach: number } | { resume: boolean; prompt: string | null };

export interface ClaudeLaunch {
  generation: number;
  source: ClaudeSource;
  /** Epoch ms; an exit reported from before this belongs to the previous launch. */
  startedAt: number;
}

export const KIND_LABEL: Record<Summary["kind"], string> = {
  folder: "Folder",
  repo: "Repo folder",
  worktree: "Worktree",
};
