import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";
import { type AppError, commands } from "../bindings";
import { describeError, toAppError } from "./errors";
import { Message } from "./parts";
import { emptyTree, foldersToRefresh, mergeListing, ROOT, setExpanded, treeKey, type TreeState, visibleRows } from "./tree";

export function FileTree({ workspaceId, active, refreshSignal, selectedPath, onOpen }: {
  workspaceId: string;
  active: boolean;
  refreshSignal: number;
  selectedPath: string | null;
  onOpen: (path: string) => void;
}) {
  const [tree, setTree] = useState<TreeState>(emptyTree);
  const [error, setError] = useState<AppError | null>(null);
  const [focusIndex, setFocusIndex] = useState(0);
  const treeRef = useRef(tree);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    treeRef.current = tree;
  }, [tree]);

  const load = useCallback(
    async (folder: string) => {
      try {
        const entries = await commands.listDir(workspaceId, folder);
        setTree((current) => mergeListing(current, folder, entries));
        if (folder === ROOT) setError(null);
      } catch (caught) {
        if (folder === ROOT) setError(toAppError(caught));
        else setTree((current) => setExpanded(current, folder, false));
      }
    },
    [workspaceId],
  );

  const refresh = useCallback(async () => {
    await Promise.all(foldersToRefresh(treeRef.current).map(load));
  }, [load]);

  useEffect(() => {
    if (active) void refresh();
  }, [active, refresh, refreshSignal]);

  useEffect(() => {
    if (!active) return;
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, refresh]);

  const rows = visibleRows(tree);
  const focus = Math.min(focusIndex, Math.max(rows.length - 1, 0));

  const toggle = (path: string, open: boolean) => {
    setTree((current) => setExpanded(current, path, open));
    if (open && !tree.listings[path]) void load(path);
  };

  const focusRow = (index: number) => {
    setFocusIndex(index);
    containerRef.current?.querySelectorAll<HTMLElement>("[role=treeitem]")[index]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    const result = treeKey(rows, index, event.key);
    if (!result) return;
    event.preventDefault();
    if (result.type === "focus") focusRow(result.index);
    else if (result.type === "expand") toggle(result.path, true);
    else if (result.type === "collapse") toggle(result.path, false);
    else onOpen(result.path);
  };

  if (error) return <Message alert title={describeError(error).title} detail={error.message} actions={[{ label: "Retry", onClick: () => void refresh() }]} />;

  if (tree.listings[ROOT]?.length === 0) return <Message title="Empty folder" />;

  return (
    <div ref={containerRef} role="tree" aria-label="Files" className="ide-tree">
      {rows.map((row, index) => {
        const { entry } = row;
        return (
          <div
            key={entry.relPath}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-expanded={entry.isDir ? row.expanded : undefined}
            aria-selected={entry.relPath === selectedPath}
            tabIndex={index === focus ? 0 : -1}
            className={entry.relPath === selectedPath ? "ide-tree-row is-selected" : "ide-tree-row"}
            style={{ paddingLeft: 12 + row.depth * 12 }}
            title={entry.relPath}
            onFocus={() => setFocusIndex(index)}
            onKeyDown={(event) => onKeyDown(event, index)}
            onClick={() => (entry.isDir ? toggle(entry.relPath, !row.expanded) : onOpen(entry.relPath))}
          >
            <span className="ide-twisty" aria-hidden="true">
              {entry.isDir && (
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" className={row.expanded ? "is-open" : undefined}>
                  <path d="M3.5 2l3 3-3 3" />
                </svg>
              )}
            </span>
            <span className={entry.isDir ? "ide-tree-name ide-tree-name--dir" : "ide-tree-name"}>{entry.name}</span>
          </div>
        );
      })}
    </div>
  );
}
