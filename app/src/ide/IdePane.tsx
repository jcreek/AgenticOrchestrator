import { type KeyboardEvent, useCallback, useEffect, useId, useRef, useState } from "react";
import { ChangesView } from "./ChangesView";
import { EditorPanel, type OpenFile, type ViewMode } from "./EditorPanel";
import { FileTree } from "./FileTree";
import { useVisible } from "./hooks";
import { type ChangeRow, formatCounts, rowFor, totals } from "./status";
import { useChanges } from "./useChanges";
import "./ide.css";

export interface IdePaneProps {
  workspaceId: string;
  onDirtyChange?: (dirty: boolean) => void;
}

type Tab = "changes" | "files";

export function IdePane({ workspaceId, onDirtyChange }: IdePaneProps) {
  const dirtyChange = useRef(onDirtyChange);
  dirtyChange.current = onDirtyChange;
  const rootRef = useRef<HTMLElement>(null);
  const visible = useVisible(rootRef);
  const changes = useChanges(workspaceId, visible);
  const [tab, setTab] = useState<Tab>("changes");
  const [open, setOpen] = useState<OpenFile | null>(null);
  const [treeSignal, setTreeSignal] = useState(0);
  const dirty = useRef(false);
  const ids = useId();

  const openFile = (path: string, view: ViewMode) => {
    if (open?.path === path && open.view === view) return;
    if (dirty.current && !window.confirm("Discard unsaved changes?")) return;
    setOpen({ path, view });
  };

  const firstRow = changes.rows[0];
  useEffect(() => {
    if (!open && firstRow) setOpen({ path: firstRow.path, view: "diff" });
  }, [open, firstRow]);

  const onDirty = useCallback((value: boolean) => {
    dirty.current = value;
    dirtyChange.current?.(value);
  }, []);

  const onSelectChange = (row: ChangeRow) => {
    if (open?.path === row.path) return;
    openFile(row.path, "diff");
  };

  const onTabKey = (event: KeyboardEvent) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const next: Tab = tab === "changes" ? "files" : "changes";
    setTab(next);
    document.getElementById(`${ids}-${next}-tab`)?.focus();
  };

  const { refresh } = changes;
  const onSaved = useCallback(() => void refresh(), [refresh]);
  const row = rowFor(changes.rows, open?.path ?? null);
  const { added, removed } = totals(changes.rows);
  const showTotals = tab === "changes" && changes.mode === "uncommitted" && (added > 0 || removed > 0);

  return (
    <section ref={rootRef} aria-label="IDE" className="ide" data-workspace-id={workspaceId}>
      <div className="ide-sidebar">
        <div className="ide-head ide-tabs-head">
          <div role="tablist" aria-label="Sidebar" className="ide-tabs">
            <TabButton id={ids} tab="changes" current={tab} onSelect={setTab} onKeyDown={onTabKey}>
              {changes.loaded ? `Changes ${changes.rows.length}` : "Changes"}
            </TabButton>
            <TabButton id={ids} tab="files" current={tab} onSelect={setTab} onKeyDown={onTabKey}>
              Files
            </TabButton>
          </div>
          <span className="ide-spacer" />
          {showTotals && (
            <span className="ide-totals">
              <span className="ide-added">{formatCounts(added, 0)}</span> <span className="ide-removed">{formatCounts(0, removed)}</span>
            </span>
          )}
          {tab === "files" && (
            <button type="button" className="ide-icon-button" aria-label="Refresh files" title="Refresh" onClick={() => setTreeSignal((n) => n + 1)}>
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M10 6a4 4 0 1 1-1.2-2.85M10 1.5v2.5H7.5" />
              </svg>
            </button>
          )}
        </div>
        <div role="tabpanel" id={`${ids}-changes-panel`} aria-labelledby={`${ids}-changes-tab`} className="ide-panel" hidden={tab !== "changes"}>
          <ChangesView workspaceId={workspaceId} changes={changes} selectedPath={open?.path ?? null} onSelect={onSelectChange} />
        </div>
        <div role="tabpanel" id={`${ids}-files-panel`} aria-labelledby={`${ids}-files-tab`} className="ide-panel" hidden={tab !== "files"}>
          <FileTree
            workspaceId={workspaceId}
            active={visible && tab === "files"}
            refreshSignal={treeSignal}
            selectedPath={open?.path ?? null}
            onOpen={(path) => openFile(path, "edit")}
          />
        </div>
      </div>
      <EditorPanel
        workspaceId={workspaceId}
        open={open}
        row={row}
        base={changes.mode === "branch" ? changes.base : null}
        visible={visible}
        onView={(view) => open && openFile(open.path, view)}
        onDirty={onDirty}
        onSaved={onSaved}
      />
    </section>
  );
}

function TabButton({ id, tab, current, onSelect, onKeyDown, children }: {
  id: string;
  tab: Tab;
  current: Tab;
  onSelect: (tab: Tab) => void;
  onKeyDown: (event: KeyboardEvent) => void;
  children: string;
}) {
  const selected = tab === current;
  return (
    <button
      type="button"
      role="tab"
      id={`${id}-${tab}-tab`}
      aria-selected={selected}
      aria-controls={`${id}-${tab}-panel`}
      tabIndex={selected ? 0 : -1}
      className="ide-tab"
      onClick={() => onSelect(tab)}
      onKeyDown={onKeyDown}
    >
      {children}
    </button>
  );
}
