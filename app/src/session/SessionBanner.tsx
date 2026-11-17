import { KILLED_EXIT_CODE, type WorkspaceStatus_Serialize } from "../bindings";
import { useNow } from "../state/hooks";
import type { ClaudeLaunch, Summary, Workspace } from "../state/model";
import { formatAgo, isCrash } from "../state/status";
import { StatusIcon, WarningIcon } from "../shell/StatusIcon";

interface Props {
  workspace: Workspace;
  summary: Summary | undefined;
  status: WorkspaceStatus_Serialize | undefined;
  unavailable: boolean;
  launch: ClaudeLaunch | undefined;
  onStart: (resume: boolean) => void;
  onRemove: () => void;
  onRetryStatus: () => void;
}

function Ago({ since }: { since: number }) {
  const now = useNow();
  return <>{formatAgo(now - since)}</>;
}

function StartButtons({ canResume, onStart }: { canResume: boolean; onStart: (resume: boolean) => void }) {
  return (
    <>
      {canResume && (
        <button type="button" className="btn btn-sm btn-primary" onClick={() => onStart(true)}>
          Resume conversation
        </button>
      )}
      <button type="button" className={`btn btn-sm${canResume ? "" : " btn-primary"}`} onClick={() => onStart(false)}>
        Start fresh
      </button>
    </>
  );
}

export function SessionBanner({ workspace, summary, status, unavailable, launch, onStart, onRemove, onRetryStatus }: Props) {
  const canResume = workspace.claudeSessionId !== null;
  const place = summary?.kind === "worktree" ? "worktree" : "folder";

  if (summary?.folderMissing) {
    return (
      <div className="banner banner-danger" role="alert">
        <WarningIcon color="var(--status-crashed)" />
        <span className="banner-text">
          <span className="banner-title">Folder not found: {workspace.path}</span>
          <span className="banner-detail">
            The folder was moved, renamed or deleted. This session cannot start until it is back.
          </span>
        </span>
        <button type="button" className="btn btn-sm" onClick={onRemove}>
          Remove from list
        </button>
      </div>
    );
  }

  if (!launch) {
    return (
      <div className="banner" role="status">
        <StatusIcon kind="idle" />
        <span className="banner-text">
          <span className="banner-title">Claude Code is not running</span>
          <span className="banner-detail">
            {canResume ? "Resume the last conversation, or start a new one." : `Start Claude Code in this ${place}.`}
          </span>
        </span>
        <StartButtons canResume={canResume} onStart={onStart} />
      </div>
    );
  }

  const exitedThisLaunch = status?.status === "exited" && status.since >= launch.startedAt;
  if (exitedThisLaunch && isCrash(status.exitCode)) {
    return (
      <div className="banner banner-danger" role="alert">
        <WarningIcon color="var(--status-crashed)" />
        <span className="banner-text">
          <span className="banner-title">Claude Code exited unexpectedly</span>
          <span className="banner-detail">
            Exit code {status.exitCode} · <Ago since={status.since} /> · the {place} and its changes are untouched
          </span>
        </span>
        <StartButtons canResume={canResume} onStart={onStart} />
      </div>
    );
  }

  if (exitedThisLaunch) {
    const stopped = status.exitCode === KILLED_EXIT_CODE;
    return (
      <div className="banner" role="status">
        <StatusIcon kind="idle" />
        <span className="banner-text">
          <span className="banner-title">{stopped ? "Claude Code was stopped" : "Claude Code has exited"}</span>
          <span className="banner-detail">
            {stopped || status.exitCode === null ? "" : `Exit code ${status.exitCode} · `}
            <Ago since={status.since} />
          </span>
        </span>
        <StartButtons canResume={canResume} onStart={onStart} />
      </div>
    );
  }

  if (unavailable && status) {
    return (
      <div className="banner" role="alert">
        <StatusIcon kind="unknown" />
        <span className="banner-text">
          <span className="banner-title">Cannot read this session’s state</span>
          <span className="banner-detail">
            The terminal still works. This tab will not flash or notify until the state can be read again.
          </span>
        </span>
        <button type="button" className="btn btn-sm" onClick={onRetryStatus}>
          Retry
        </button>
      </div>
    );
  }

  return null;
}
