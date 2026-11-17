import { shortcutHints } from "../state/shortcuts";
import { memo, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { TerminalInfo_Serialize, WorkspaceStatus_Serialize } from "../bindings";
import { IdePane } from "../ide/IdePane";
import { clampShare, MAX_SHARE, MIN_SHARE, type Layout, type LayoutMode } from "../state/layout";
import { KIND_LABEL, type ClaudeLaunch, type Summary, type Workspace } from "../state/model";
import { TerminalPane } from "../terminal/TerminalPane";
import { SessionBanner } from "./SessionBanner";
import "./SessionView.css";

interface Props {
  workspace: Workspace;
  summary: Summary | undefined;
  status: WorkspaceStatus_Serialize | undefined;
  unavailable: boolean;
  launch: ClaudeLaunch | undefined;
  initialShells: TerminalInfo_Serialize[];
  layout: Layout;
  visible: boolean;
  onLayout: (id: string, layout: Layout) => void;
  onStart: (id: string, resume: boolean) => void;
  onClaudeStarted: () => void;
  onClose: (id: string) => void;
  onRemove: (id: string) => void;
  onRetryStatus: () => void;
  onDirtyChange: (id: string, dirty: boolean) => void;
}

const MODES: [LayoutMode, string][] = [
  ["terminal", "Terminal"],
  ["both", "Both"],
  ["ide", "IDE"],
];
const KEY_STEP = 0.02;

export const SessionView = memo(function SessionView(props: Props) {
  const { workspace, summary, visible } = props;
  const id = workspace.id;
  const onLayout = (next: Layout) => props.onLayout(id, next);
  const onClose = () => props.onClose(id);
  const [dragShare, setDragShare] = useState<number | null>(null);
  const layout = dragShare === null ? props.layout : { ...props.layout, terminalShare: dragShare };
  const bodyRef = useRef<HTMLDivElement>(null);
  const [ideMounted, setIdeMounted] = useState(false);
  const both = layout.mode === "both";
  const terminalFirst = layout.stacked ? layout.swapped : !layout.swapped;
  const firstShare = terminalFirst ? layout.terminalShare : 1 - layout.terminalShare;

  useEffect(() => {
    if (visible && layout.mode !== "terminal") setIdeMounted(true);
  }, [visible, layout.mode]);

  const toTerminalShare = (share: number) => clampShare(terminalFirst ? share : 1 - share);
  const setFirstShare = (share: number) => onLayout({ ...layout, terminalShare: toTerminalShare(share) });

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const body = bodyRef.current;
    if (!body || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const rect = body.getBoundingClientRect();
    setDragShare(
      toTerminalShare(layout.stacked ? (event.clientY - rect.top) / rect.height : (event.clientX - rect.left) / rect.width),
    );
  };
  const onDragEnd = () => {
    if (dragShare === null) return;
    onLayout(layout);
    setDragShare(null);
  };
  const onSeparatorKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const shrink = layout.stacked ? "ArrowUp" : "ArrowLeft";
    const grow = layout.stacked ? "ArrowDown" : "ArrowRight";
    const targets: Record<string, number> = {
      [shrink]: firstShare - KEY_STEP,
      [grow]: firstShare + KEY_STEP,
      Home: MIN_SHARE,
      End: MAX_SHARE,
    };
    if (!(event.key in targets)) return;
    event.preventDefault();
    setFirstShare(targets[event.key]);
  };

  return (
    <div className="session" hidden={!visible}>
      <div className="session-toolbar">
        <span className="session-path mono" title={workspace.path}>
          {workspace.path}
        </span>
        {summary && <span className="session-kind">{KIND_LABEL[summary.kind]}</span>}
        <span className="session-space" />
        <div className="segmented" role="group" aria-label="Layout">
          {MODES.map(([mode, label]) => (
            <button
              key={mode}
              type="button"
              aria-pressed={layout.mode === mode}
              onClick={() => onLayout({ ...layout, mode })}
            >
              {label}
            </button>
          ))}
        </div>
        {both && (
          <>
            <button type="button" className="btn-plain" onClick={() => onLayout({ ...layout, swapped: !layout.swapped })}>
              {layout.stacked ? "Swap" : "Swap sides"}
            </button>
            <button type="button" className="btn-plain" onClick={() => onLayout({ ...layout, stacked: !layout.stacked })}>
              {layout.stacked ? "Side by side" : "Stack"}
            </button>
            <button type="button" className="btn-plain" title={shortcutHints("Close session (Ctrl+W)")} onClick={onClose}>
              Close session
            </button>
          </>
        )}
        {!both && (
          <button type="button" className="btn btn-sm" onClick={onClose}>
            Close session <span className="kbd">{shortcutHints("Ctrl+W")}</span>
          </button>
        )}
      </div>
      <SessionBanner
        workspace={workspace}
        summary={summary}
        status={props.status}
        unavailable={props.unavailable}
        launch={props.launch}
        onStart={(resume) => props.onStart(id, resume)}
        onRemove={() => props.onRemove(id)}
        onRetryStatus={props.onRetryStatus}
      />
      <div className={`session-body${layout.stacked ? " session-stacked" : ""}`} ref={bodyRef}>
        <section
          aria-label="Terminal"
          className="pane pane-terminal"
          hidden={layout.mode === "ide"}
          style={{ order: terminalFirst ? 0 : 2, flex: both ? `0 0 calc(${layout.terminalShare * 100}% - 2px)` : undefined }}
        >
          <TerminalPane
            workspaceId={id}
            launch={props.launch}
            initialShells={props.initialShells}
            visible={visible && layout.mode !== "ide"}
            onClaudeStarted={props.onClaudeStarted}
          />
        </section>
        {both && (
          <div
            role="separator"
            className="separator"
            tabIndex={0}
            aria-label="Resize terminal and IDE"
            aria-orientation={layout.stacked ? "horizontal" : "vertical"}
            aria-valuemin={MIN_SHARE * 100}
            aria-valuemax={MAX_SHARE * 100}
            aria-valuenow={Math.round(firstShare * 100)}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onLostPointerCapture={onDragEnd}
            onKeyDown={onSeparatorKey}
          />
        )}
        <section aria-label="IDE" className="pane pane-ide" hidden={layout.mode === "terminal"} style={{ order: terminalFirst ? 2 : 0 }}>
          {ideMounted && <IdePane workspaceId={id} onDirtyChange={(dirty) => props.onDirtyChange(id, dirty)} />}
        </section>
      </div>
    </div>
  );
});
