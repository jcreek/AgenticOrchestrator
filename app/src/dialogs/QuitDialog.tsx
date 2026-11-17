import type { CloseGuard } from "../state/closeGuard";
import { WarningIcon } from "../shell/StatusIcon";
import { Dialog } from "./Dialog";

interface Props {
  guard: CloseGuard;
  onQuit: () => void;
  onCancel: () => void;
}

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export function QuitDialog({ guard, onQuit, onCancel }: Props) {
  const footer = (
    <>
      <span className="dialog-footer-note" />
      <button type="button" className="btn" onClick={onCancel} data-autofocus>
        Cancel <span className="kbd">Esc</span>
      </button>
      <button type="button" className="btn btn-primary" onClick={onQuit}>
        Close app
      </button>
    </>
  );
  return (
    <Dialog title="Close Agentic Orchestrator?" onCancel={onCancel} footer={footer}>
      {guard.running > 0 && (
        <div className="notice">
          <WarningIcon color="var(--status-input)" />
          <span>
            {count(guard.running, "running Claude session", "running Claude sessions")} will be stopped. Their folders and
            changes are kept.
          </span>
        </div>
      )}
      {guard.unsaved > 0 && (
        <div className="notice notice-danger" role="alert">
          <WarningIcon color="var(--status-crashed)" />
          <span>{count(guard.unsaved, "file has", "files have")} unsaved edits that will be lost.</span>
        </div>
      )}
    </Dialog>
  );
}
