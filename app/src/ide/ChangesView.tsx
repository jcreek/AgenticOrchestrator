import { type KeyboardEvent, useRef, useState } from "react";
import { type AppError, commands } from "../bindings";
import { CommitBox } from "./CommitBox";
import { describeError, toAppError } from "./errors";
import { useSettings } from "../state/settings";
import { Message } from "./parts";
import {
  type ChangeRow,
  changeTree,
  flatItems,
  formatCounts,
  hasStaged,
  rowFor,
  rowKey,
  stageState,
  stageToggle,
  statusLetter,
  statusName,
  statusTone,
} from "./status";
import type { Changes } from "./useChanges";

export function ChangesView({ workspaceId, changes, selectedPath, onSelect }: {
  workspaceId: string;
  changes: Changes;
  selectedPath: string | null;
  onSelect: (row: ChangeRow) => void;
}) {
  const [actionError, setActionError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [collapsed, setCollapsed] = useState<string[]>([]);
  const listRef = useRef<HTMLUListElement>(null);
  const { settings, update } = useSettings();
  const asTree = settings.changesLayout === "tree";
  const { mode } = changes;
  const items = asTree ? changeTree(changes.rows, collapsed) : flatItems(changes.rows);
  const rows = items.flatMap((item) => (item.type === "file" ? [item.row] : []));
  const stageable = mode === "uncommitted";
  const selectedRow = rowFor(rows, selectedPath);
  const selectedIndex = selectedRow ? rows.indexOf(selectedRow) : -1;

  const toggleFolder = (path: string) =>
    setCollapsed((current) => (current.includes(path) ? current.filter((folder) => folder !== path) : [...current, path]));

  async function act(work: () => Promise<unknown>) {
    setBusy(true);
    setActionError(null);
    try {
      await work();
    } catch (caught) {
      setActionError(toAppError(caught));
    } finally {
      setBusy(false);
      void changes.refresh();
    }
  }

  const toggleStage = (row: ChangeRow) => {
    const { action, paths } = stageToggle(row);
    void act(() => (action === "stage" ? commands.gitStage(workspaceId, paths) : commands.gitUnstage(workspaceId, paths)));
  };

  const selectAt = (index: number) => {
    const row = rows[index];
    if (!row) return;
    onSelect(row);
    listRef.current?.querySelectorAll<HTMLButtonElement>(".ide-row-main")[index]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      selectAt(index + (event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === " " && stageable && !busy) {
      event.preventDefault();
      toggleStage(rows[index]);
    }
  };

  const anyUnstaged = changes.statusRows.some((row) => row.unstaged);

  return (
    <div className="ide-changes">
      <div className="ide-toolbar">
        {changes.base && (
          <div role="group" aria-label="Compare" className="ide-segmented">
            <button type="button" aria-pressed={mode === "uncommitted"} onClick={() => changes.setMode("uncommitted")}>
              Uncommitted
            </button>
            <button type="button" aria-pressed={mode === "branch"} onClick={() => changes.setMode("branch")} title={`Compared with ${changes.base}`}>
              This branch
            </button>
          </div>
        )}
        <span className="ide-spacer" />
        <div role="group" aria-label="Changes layout" className="ide-segmented">
          <button type="button" aria-pressed={asTree} onClick={() => update({ changesLayout: "tree" })}>
            Tree
          </button>
          <button type="button" aria-pressed={!asTree} onClick={() => update({ changesLayout: "list" })}>
            List
          </button>
        </div>
        {stageable && (
          <button type="button" className="ide-button" disabled={busy || !anyUnstaged} onClick={() => void act(() => commands.gitStageAll(workspaceId))}>
            Stage all
          </button>
        )}
      </div>
      {changes.error && <Message alert title={describeError(changes.error).title} detail={changes.error.message} />}
      {actionError && (
        <div className="ide-inline-error" role="alert">
          {actionError.message}
        </div>
      )}
      {!changes.error && changes.loaded && items.length === 0 && (
        <Message title="No changes" detail={stageable ? "The working tree matches HEAD." : `Nothing differs from ${changes.base}.`} />
      )}
      {items.length > 0 && (
        <ul ref={listRef} className="ide-list" aria-label="Changed files">
          {items.map((item) => {
            const indent = { paddingLeft: 12 + item.depth * 12 };
            if (item.type === "folder") {
              return (
                <li key={`folder:${item.path}`} className="ide-row" style={indent}>
                  <button type="button" className="ide-folder" aria-expanded={item.expanded} title={item.path} onClick={() => toggleFolder(item.path)}>
                    <span className="ide-twisty" aria-hidden="true">
                      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" className={item.expanded ? "is-open" : undefined}>
                        <path d="M3.5 2l3 3-3 3" />
                      </svg>
                    </span>
                    {item.name}
                  </button>
                </li>
              );
            }
            const { row } = item;
            const index = rows.indexOf(row);
            const selected = index === selectedIndex;
            const tabbable = selected || (selectedIndex < 0 && index === 0);
            const counts = formatCounts(row.added, row.removed);
            return (
              <li key={rowKey(row)} className={selected ? "ide-row is-selected" : "ide-row"} style={indent} data-path={row.path}>
                {stageable && <StageMarker row={row} disabled={busy} onToggle={() => toggleStage(row)} />}
                <button
                  type="button"
                  className="ide-row-main"
                  tabIndex={tabbable ? 0 : -1}
                  aria-current={selected || undefined}
                  aria-keyshortcuts={stageable ? "Space" : undefined}
                  title={row.originalPath ? `${row.path}\nRenamed from ${row.originalPath}` : row.path}
                  onClick={() => onSelect(row)}
                  onKeyDown={(event) => onKeyDown(event, index)}
                >
                  <span className={`ide-letter ide-tone-${statusTone(row.kind)}`} aria-hidden="true">
                    {statusLetter(row.kind)}
                  </span>
                  <span className="ide-visually-hidden">{statusName(row.kind)}</span>
                  <span className="ide-path">
                    <bdi>{item.name}</bdi>
                  </span>
                  {counts && <span className="ide-counts">{counts}</span>}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <span className="ide-grow" />
      <CommitBox workspaceId={workspaceId} hasStaged={hasStaged(changes.statusRows)} onDone={() => void changes.refresh()} />
    </div>
  );
}

function StageMarker({ row, disabled, onToggle }: { row: ChangeRow; disabled: boolean; onToggle: () => void }) {
  const state = stageState(row);
  const checked = state === "mixed" ? "mixed" : state === "checked";
  return (
    <button
      type="button"
      role="checkbox"
      tabIndex={-1}
      aria-checked={checked}
      aria-label={`Staged: ${row.path}`}
      title={state === "checked" ? "Unstage" : "Stage"}
      className="ide-stage"
      disabled={disabled}
      onClick={onToggle}
    >
      {state === "checked" && (
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M2 5.2l2 2 4-4.4" />
        </svg>
      )}
      {state === "mixed" && (
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
          <path d="M2.5 5h5" />
        </svg>
      )}
    </button>
  );
}
