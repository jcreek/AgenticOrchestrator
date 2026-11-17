import "./Overview.css";

export function Empty({ onNewSession }: { onNewSession: () => void }) {
  return (
    <div className="empty">
      <svg width="40" height="40" viewBox="0 0 40 40" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <rect x="4" y="7" width="32" height="26" rx="3" />
        <path d="M11 16l5 4-5 4M20 24h8" />
      </svg>
      <h1>No sessions yet</h1>
      <p>
        Start Claude Code in a repo folder, or in a new git worktree on its own branch. Each session opens as a tab, and
        the tab shows when it needs you.
      </p>
      <button type="button" className="btn btn-primary" onClick={onNewSession} autoFocus>
        New session <span className="kbd">Ctrl+N</span>
      </button>
      <div className="empty-keys">
        <span className="mono">Ctrl+J</span>
        <span>Jump to the next session that needs you</span>
        <span className="mono">Ctrl+1–9</span>
        <span>Switch to a session tab</span>
        <span className="mono">Ctrl+0</span>
        <span>Back to this overview</span>
      </div>
    </div>
  );
}
