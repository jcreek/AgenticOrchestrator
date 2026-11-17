import { shortcutHints } from "../state/shortcuts";
import type { CSSProperties } from "react";
import type { SessionTab } from "../state/model";
import { StatusIcon } from "../shell/StatusIcon";
import "./Toasts.css";

interface Props {
  sticky: SessionTab[];
  finished: SessionTab[];
  /** Where Ctrl+J goes; only that toast shows the hint. */
  jumpId: string | null;
  onGoTo: (id: string) => void;
  onSnooze: (id: string) => void;
  onDismiss: (id: string) => void;
  onShowOverview: () => void;
  onDismissFinished: () => void;
}

const MAX_STICKY = 3;

function title(tab: SessionTab): string {
  return `${tab.group?.name ?? tab.name} / ${tab.branch}`;
}

function DismissButton({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="toast-dismiss" aria-label="Dismiss" onClick={onClick}>
      <svg width="8" height="8" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
        <path d="M0 0l10 10M10 0L0 10" />
      </svg>
    </button>
  );
}

export function Toasts({ sticky, finished, jumpId, onGoTo, onSnooze, onDismiss, onShowOverview, onDismissFinished }: Props) {
  if (sticky.length === 0 && finished.length === 0) return null;
  return (
    <section className="toasts" aria-label="Notifications">
      {finished.length > 0 && (
        <div className="toast" role="status" style={{ "--toast-color": "var(--status-finished)" } as CSSProperties}>
          <div className="toast-head">
            <StatusIcon kind="finished" />
            <span className="toast-title">
              {finished.length === 1 ? "1 session finished" : `${finished.length} sessions finished`}
            </span>
            <DismissButton onClick={onDismissFinished} />
          </div>
          {finished.map((tab) => (
            <span key={tab.id} className="mono">
              {title(tab)}
            </span>
          ))}
          <div className="toast-actions">
            <button type="button" className="btn btn-sm" onClick={onShowOverview}>
              Show in overview <span className="kbd">{shortcutHints("Ctrl+0")}</span>
            </button>
          </div>
        </div>
      )}
      {sticky.slice(0, MAX_STICKY).map((tab) => (
        <div
          key={tab.id}
          className="toast"
          role="status"
          style={{ "--toast-color": tab.presentation.colorVar } as CSSProperties}
        >
          <div className="toast-head">
            <StatusIcon kind={tab.presentation.kind} />
            <span className="toast-title">{tab.presentation.label}</span>
            <DismissButton onClick={() => onDismiss(tab.id)} />
          </div>
          <span className="mono">{title(tab)}</span>
          <div className="toast-actions">
            <button type="button" className="btn btn-sm btn-primary" onClick={() => onGoTo(tab.id)}>
              Go to tab{tab.id === jumpId && <span className="kbd">{shortcutHints(" Ctrl+J")}</span>}
            </button>
            <button type="button" className="btn btn-sm" onClick={() => onSnooze(tab.id)}>
              Snooze 10 min
            </button>
          </div>
        </div>
      ))}
    </section>
  );
}
