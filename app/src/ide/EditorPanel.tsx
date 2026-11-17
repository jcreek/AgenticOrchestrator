import { useCallback, useRef, useState } from "react";
import { type AppError, commands } from "../bindings";
import { useSettings } from "../state/settings";
import { DiffView } from "./DiffView";
import { EditView } from "./EditView";
import { toAppError } from "./errors";
import { useMonaco } from "./hooks";
import { Message } from "./parts";
import type { ChangeRow } from "./status";

export type ViewMode = "diff" | "edit";

export interface OpenFile {
  path: string;
  view: ViewMode;
}

export function EditorPanel({ workspaceId, open, row, base, visible, onView, onDirty, onSaved }: {
  workspaceId: string;
  open: OpenFile | null;
  /** The open file's row in the current changes list, if it has changes. */
  row: ChangeRow | null;
  base: string | null;
  visible: boolean;
  onView: (view: ViewMode) => void;
  onDirty: (dirty: boolean) => void;
  onSaved: () => void;
}) {
  const { monaco, failed } = useMonaco(visible && open !== null);
  const { settings, update } = useSettings();
  const { diffLayout } = settings;
  const [dirty, setDirty] = useState(false);
  const [vscodeError, setVscodeError] = useState<AppError | null>(null);
  const cursorLine = useRef<number | null>(null);

  const reportDirty = useCallback(
    (value: boolean) => {
      setDirty(value);
      onDirty(value);
    },
    [onDirty],
  );

  const onCursorLine = useCallback((line: number) => {
    cursorLine.current = line;
  }, []);

  const openInVscode = useCallback(() => {
    if (!open) return;
    setVscodeError(null);
    const line = open.view === "edit" ? cursorLine.current : null;
    commands.openInVscode(workspaceId, open.path, line).catch((caught: unknown) => setVscodeError(toAppError(caught)));
  }, [workspaceId, open]);

  if (!open) {
    return (
      <div className="ide-main">
        <div className="ide-head" />
        <div className="ide-body">
          <Message title="No file selected" detail="Pick a changed file or browse Files." />
        </div>
      </div>
    );
  }

  const deleted = row?.kind === "deleted";
  let label = "Editing";
  if (open.view === "diff") label = base ? `Diff against ${base}` : "Diff against HEAD";

  let body;
  if (failed) body = <Message alert title="The editor failed to load" detail={failed} />;
  else if (!monaco) body = <Message title="Loading editor…" />;
  else if (open.view === "diff")
    body = (
      <DiffView
        key={`${open.path}\n${base ?? ""}`}
        m={monaco}
        workspaceId={workspaceId}
        path={open.path}
        originalPath={row?.originalPath ?? null}
        deleted={deleted}
        base={base}
        revision={row ? `${row.kind}:${row.added}:${row.removed}:${row.staged}:${row.unstaged}` : "clean"}
        visible={visible}
        onOpenInVscode={openInVscode}
      />
    );
  else
    body = (
      <EditView
        key={open.path}
        m={monaco}
        workspaceId={workspaceId}
        path={open.path}
        visible={visible}
        onDirty={reportDirty}
        onCursorLine={onCursorLine}
        onSaved={onSaved}
        onOpenInVscode={openInVscode}
      />
    );

  return (
    <div className="ide-main">
      <div className="ide-head">
        <span className="ide-file-path" title={open.path}>
          <bdi>{open.path}</bdi>
        </span>
        {open.view === "edit" && dirty && (
          <span className="ide-dirty" role="img" aria-label="Unsaved changes" title="Unsaved changes">
            ●
          </span>
        )}
        <span className="ide-muted ide-nowrap">{label}</span>
        <span className="ide-spacer" />
        {vscodeError && (
          <span className="ide-head-error" role="alert" title={vscodeError.message}>
            {vscodeError.message}
          </span>
        )}
        {open.view === "diff" && (
          <div role="group" aria-label="Diff view" className="ide-segmented">
            <button type="button" aria-pressed={diffLayout === "sideBySide"} onClick={() => update({ diffLayout: "sideBySide" })}>
              Side by side
            </button>
            <button type="button" aria-pressed={diffLayout === "inline"} onClick={() => update({ diffLayout: "inline" })}>
              Inline
            </button>
          </div>
        )}
        {open.view === "diff" ? (
          <button type="button" className="ide-button" disabled={deleted} title={deleted ? "Deleted files cannot be edited" : undefined} onClick={() => onView("edit")}>
            Edit file
          </button>
        ) : (
          <button type="button" className="ide-button" onClick={() => onView("diff")}>
            View diff
          </button>
        )}
        <button type="button" className="ide-button" disabled={deleted} onClick={openInVscode}>
          Open in VS Code
        </button>
      </div>
      <div className="ide-body">{body}</div>
    </div>
  );
}
