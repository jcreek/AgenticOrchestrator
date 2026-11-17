import { useCallback, useEffect, useRef, useState } from "react";
import { type AppError, commands } from "../bindings";
import { toAppError } from "./errors";
import { usePoll } from "./hooks";
import { type ChangeRow, fromBranch, fromStatus } from "./status";

export type ChangeMode = "uncommitted" | "branch";

const POLL_MS = 3000;
// Git changes made outside the folder, as in a linked worktree, move no revision.
const UNCHANGED_REFRESH_MS = 30_000;

export function useChanges(workspaceId: string, visible: boolean) {
  const [chosenMode, setMode] = useState<ChangeMode>("uncommitted");
  const [base, setBase] = useState<string | null | undefined>(undefined);
  const [statusRows, setStatusRows] = useState<ChangeRow[] | null>(null);
  const [branchRows, setBranchRows] = useState<ChangeRow[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const sequence = useRef(0);

  const mode: ChangeMode = chosenMode === "branch" && base ? "branch" : "uncommitted";
  const branchBase = mode === "branch" ? base : null;

  useEffect(() => {
    if (!visible || base !== undefined) return;
    commands.gitDefaultBase(workspaceId).then(setBase, () => setBase(null));
  }, [visible, base, workspaceId]);

  const refresh = useCallback(async () => {
    const id = ++sequence.current;
    try {
      const [status, branch] = await Promise.all([
        commands.gitStatus(workspaceId),
        branchBase ? commands.gitBranchChanges(workspaceId, branchBase) : null,
      ]);
      if (id !== sequence.current) return;
      setStatusRows(status.map(fromStatus));
      setBranchRows(branch ? branch.map(fromBranch) : null);
      setError(null);
    } catch (caught) {
      if (id === sequence.current) setError(toAppError(caught));
    }
  }, [workspaceId, branchBase]);

  const last = useRef<{ refresh: typeof refresh; revision: number | undefined; at: number } | null>(null);
  const refreshIfChanged = useCallback(async () => {
    const revisions = await commands.workspaceRevisions().catch(() => ({}) as Partial<Record<string, number>>);
    const revision = revisions[workspaceId];
    const previous = last.current;
    const unchanged = revision !== undefined && previous?.refresh === refresh && previous.revision === revision;
    if (unchanged && Date.now() - previous.at < UNCHANGED_REFRESH_MS) return;
    last.current = { refresh, revision, at: Date.now() };
    await refresh();
  }, [workspaceId, refresh]);

  usePoll(refreshIfChanged, POLL_MS, visible);

  const rows = (mode === "branch" ? branchRows : statusRows) ?? [];
  const loaded = mode === "branch" ? branchRows !== null : statusRows !== null;
  return { mode, setMode, base: base ?? null, rows, statusRows: statusRows ?? [], loaded, error, refresh };
}

export type Changes = ReturnType<typeof useChanges>;
