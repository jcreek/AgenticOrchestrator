import { type KeyboardEvent, useId, useState } from "react";
import { commands } from "../bindings";
import { toAppError } from "./errors";

const BUSY_TEXT = { commit: "Committing…", push: "Pushing…" };

interface Result {
  ok: boolean;
  text: string;
}

export function CommitBox({ workspaceId, hasStaged, onDone }: {
  workspaceId: string;
  hasStaged: boolean;
  onDone: () => void;
}) {
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<"commit" | "push" | null>(null);
  const [result, setResult] = useState<Result | null>(null);
  const reasonId = useId();

  let reason: string | null = null;
  if (!hasStaged) reason = "Stage files to commit";
  else if (!message.trim()) reason = "Write a commit message";

  async function run(kind: "commit" | "push", done: string) {
    setBusy(kind);
    setResult(null);
    try {
      if (kind === "commit") {
        await commands.gitCommit(workspaceId, message);
        setMessage("");
      } else {
        await commands.gitPush(workspaceId);
      }
      setResult({ ok: true, text: done });
    } catch (caught) {
      const error = toAppError(caught);
      const verb = kind === "commit" ? "Commit" : "Push";
      const text = error.kind === "timedOut" ? `${verb} timed out. ${error.message}` : error.message;
      setResult({ ok: false, text });
    } finally {
      setBusy(null);
      onDone();
    }
  }

  const commit = () => {
    if (!reason && !busy) void run("commit", "Committed.");
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && event.ctrlKey) {
      event.preventDefault();
      commit();
    }
  };

  return (
    <div className="ide-commit">
      <textarea
        className="ide-commit-message"
        aria-label="Commit message"
        placeholder="Commit message (Ctrl+Enter to commit)"
        rows={3}
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <div className="ide-commit-row">
        <span id={reasonId} className="ide-commit-reason">
          {busy ? BUSY_TEXT[busy] : reason}
        </span>
        <button type="button" className="ide-action" disabled={busy !== null} onClick={() => void run("push", "Pushed.")}>
          Push
        </button>
        <button
          type="button"
          className="ide-action ide-action--primary"
          disabled={reason !== null || busy !== null}
          aria-describedby={reasonId}
          aria-keyshortcuts="Control+Enter"
          onClick={commit}
        >
          Commit
        </button>
      </div>
      {result && (
        <div className={result.ok ? "ide-commit-result" : "ide-commit-result ide-commit-result--error"} role={result.ok ? "status" : "alert"}>
          {result.text}
        </div>
      )}
    </div>
  );
}
