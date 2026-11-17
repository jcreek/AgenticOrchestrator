import { useCallback, useEffect, useState } from "react";
import { commands, type Blocker } from "../bindings";
import { describeBlockers, hasUncommitted } from "../state/blockers";
import { errorKind, errorMessage } from "../state/errors";
import type { SessionTab } from "../state/model";
import { WarningIcon } from "../shell/StatusIcon";
import { Dialog } from "./Dialog";

interface Props {
  tab: SessionTab;
  /** What Claude is doing, when it is running and will be stopped. */
  runningLabel: string | null;
  /** The session's editor has unsaved edits. */
  unsaved: boolean;
  onClosed: (id: string) => void;
  onCancel: () => void;
}

type Choice = "keep" | "commit" | "discard";

const RUNNING_PHRASE: Record<string, string> = {
  "Needs permission": "waiting for permission",
  "Check terminal": "waiting in its terminal",
  "Status unknown": "running",
};

export function CloseSessionDialog({ tab, runningLabel, unsaved, onClosed, onCancel }: Props) {
  const removable = tab.summary?.removable === true;
  const [choice, setChoice] = useState<Choice>("keep");
  const [blockers, setBlockers] = useState<Blocker[] | null>(null);
  const [message, setMessage] = useState(`Save work in progress on ${tab.branch}`);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [committed, setCommitted] = useState(false);

  const loadBlockers = useCallback(async () => {
    const found = await commands.worktreeRemovalBlockers(tab.id);
    setBlockers(found);
    return found;
  }, [tab.id]);

  useEffect(() => {
    if (removable) loadBlockers().catch((failure: unknown) => setError(errorMessage(failure)));
  }, [removable, loadBlockers]);

  const remove = async (accepted: Blocker[]) => {
    try {
      await commands.removeWorktree(tab.id, accepted);
      onClosed(tab.id);
    } catch (failure) {
      const kind = errorKind(failure);
      if (kind === "worktreeBlocked") {
        await loadBlockers();
        setNotice("The worktree changed since this was shown. Check what would be lost, then try again.");
      } else if (kind === "notRemovable") {
        setError(`The app cannot remove this worktree: ${errorMessage(failure)}. Keep it and remove it by hand.`);
      } else {
        throw failure;
      }
    }
  };

  const submit = async () => {
    const waiting = removable && blockers === null && error === null;
    const noMessage = removable && choice === "commit" && !committed && message.trim() === "";
    if (busy || waiting || noMessage) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (!removable || choice === "keep") {
        await commands.removeWorkspace(tab.id);
        onClosed(tab.id);
      } else if (choice === "discard" || committed) {
        if (blockers) await remove(blockers);
      } else {
        await commands.gitStageAll(tab.id);
        await commands.gitCommit(tab.id, message.trim());
        setCommitted(true);
        const left = await loadBlockers();
        if (left.length === 0) await remove(left);
        else setNotice(`Committed. Removing the worktree would still lose ${describeBlockers(left)}.`);
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  };

  const loading = removable && blockers === null && !error;
  const commitInvalid = choice === "commit" && !committed && message.trim() === "";
  const canCommit = blockers !== null && hasUncommitted(blockers);
  const changed = tab.summary?.changedFiles ?? 0;
  const primaryLabel = removable && choice !== "keep" ? (committed ? "Remove worktree" : "Close and remove") : "Close session";

  const footer = (
    <>
      <span className="dialog-footer-note">{runningLabel && `Claude is ${(RUNNING_PHRASE[runningLabel] ?? runningLabel).toLowerCase()} and will be stopped`}</span>
      <button type="button" className="btn" onClick={onCancel} data-autofocus>
        Cancel <span className="kbd">Esc</span>
      </button>
      <button
        type="button"
        className="btn btn-primary"
        disabled={busy || loading || commitInvalid}
        onClick={() => void submit()}
      >
        {busy ? "Working…" : primaryLabel}
      </button>
    </>
  );

  return (
    <Dialog title={`Close ${tab.branch}?`} onCancel={onCancel} footer={footer}>
      {unsaved && (
        <div className="notice notice-danger" role="alert">
          <WarningIcon color="var(--status-crashed)" />
          <span>Unsaved edits in the editor will be lost.</span>
        </div>
      )}
      {error && (
        <div className="notice notice-danger" role="alert">
          <WarningIcon color="var(--status-crashed)" />
          <span>{error}</span>
        </div>
      )}
      {notice && (
        <div className="notice" role="status">
          <WarningIcon color="var(--status-input)" />
          <span>{notice}</span>
        </div>
      )}
      {!removable && (
        <p className="choice-detail dialog-text">
          Claude Code and the terminals in this session will be stopped. The folder <span className="mono">{tab.path}</span>{" "}
          is not changed.
        </p>
      )}
      {removable && loading && <p className="choice-detail dialog-text">Checking the worktree…</p>}
      {removable && blockers && (
        <>
          {blockers.length > 0 && (
            <div className="notice">
              <WarningIcon color="var(--status-input)" />
              <span>
                {changed > 0 && (
                  <>
                    <strong>
                      {changed} {changed === 1 ? "file has" : "files have"} uncommitted changes
                    </strong>{" "}
                    <span className="mono muted">
                      +{tab.summary?.added ?? 0} −{tab.summary?.removed ?? 0}
                    </span>{" "}
                    in <span className="mono">{tab.path}</span>.{" "}
                  </>
                )}
                Removing the worktree loses {describeBlockers(blockers)}.
              </span>
            </div>
          )}
          <fieldset className="choices">
            <legend>What should happen to the worktree?</legend>
            <label className="choice">
              <input type="radio" name="cs" checked={choice === "keep"} onChange={() => setChoice("keep")} />
              <span className="choice-text">
                <span className="choice-title">Keep the worktree</span>
                <span className="choice-detail">
                  End the session only. The folder, branch and uncommitted changes stay as they are.
                </span>
              </span>
            </label>
            {(canCommit || committed) && (
              <label className="choice">
                <input type="radio" name="cs" checked={choice === "commit"} onChange={() => setChoice("commit")} />
                <span className="choice-text">
                  <span className="choice-title">Commit changes, then remove the worktree</span>
                  <span className="choice-detail">
                    Commits {changed > 0 ? `all ${changed} files` : "all changes"} to {tab.branch}, then deletes the
                    folder. The branch is kept.
                  </span>
                </span>
              </label>
            )}
            {choice === "commit" && !committed && (
              <div className="field">
                <label htmlFor="cs-message">Commit message</label>
                <input
                  id="cs-message"
                  className="input"
                  value={message}
                  onChange={(event) => setMessage(event.target.value)}
                />
              </div>
            )}
            <label className="choice">
              <input type="radio" name="cs" checked={choice === "discard"} onChange={() => setChoice("discard")} />
              <span className="choice-text">
                <span className="choice-title">
                  {blockers.length > 0 ? "Discard changes and remove the worktree" : "Remove the worktree"}
                </span>
                <span className="choice-detail">
                  {blockers.length > 0
                    ? `Deletes the folder and loses ${describeBlockers(blockers)}. This cannot be undone. The branch is kept.`
                    : "Deletes the folder. The branch is kept."}
                </span>
              </span>
            </label>
          </fieldset>
        </>
      )}
    </Dialog>
  );
}
