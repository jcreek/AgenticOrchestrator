import { useRef, useState } from "react";
import { commands, type TerminalInfo_Serialize } from "../bindings";
import type { ClaudeLaunch } from "../state/model";
import { WarningIcon } from "../shell/StatusIcon";
import { closeLaunch, XTerm, type TerminalSource } from "./XTerm";
import "@xterm/xterm/css/xterm.css";
import "./TerminalPane.css";

interface Props {
  workspaceId: string;
  launch: ClaudeLaunch | undefined;
  initialShells: TerminalInfo_Serialize[];
  visible: boolean;
  onClaudeStarted: () => void;
}

interface Shell {
  key: string;
  number: number;
  source: TerminalSource;
  id: number | null;
  exited: boolean;
  error: string | null;
}

const CLAUDE = "claude";

export function TerminalPane({ workspaceId, launch, initialShells, visible, onClaudeStarted }: Props) {
  const [shells, setShells] = useState<Shell[]>(() =>
    initialShells.map((info, index) => ({
      key: `attached-${info.id}`,
      number: index + 1,
      source: { attach: info.id },
      id: info.id,
      exited: !info.running,
      error: null,
    })),
  );
  const [active, setActive] = useState(CLAUDE);
  const [claudeError, setClaudeError] = useState<{ generation: number; message: string } | null>(null);
  const counter = useRef(initialShells.length);

  const update = (key: string, change: Partial<Shell>) =>
    setShells((current) => current.map((shell) => (shell.key === key ? { ...shell, ...change } : shell)));

  const addShell = () => {
    counter.current += 1;
    const key = `new-${counter.current}`;
    setShells((current) => [
      ...current,
      { key, number: counter.current, source: { resume: false, prompt: null }, id: null, exited: false, error: null },
    ]);
    setActive(key);
  };

  const closeShell = (shell: Shell) => {
    if (shell.id !== null) commands.terminalClose(shell.id).catch(() => undefined);
    else closeLaunch(`${workspaceId}:shell:${shell.key}`);
    setShells((current) => current.filter((entry) => entry.key !== shell.key));
    if (active === shell.key) setActive(CLAUDE);
  };

  const error = claudeError && launch && claudeError.generation === launch.generation ? claudeError.message : null;

  return (
    <div className="terminal-pane">
      <div className="term-tabs" role="toolbar" aria-label="Terminals">
        <button
          type="button"
          className="term-tab"
          aria-pressed={active === CLAUDE}
          onClick={() => setActive(CLAUDE)}
        >
          Claude
        </button>
        {shells.map((shell) => (
          <span key={shell.key} className="term-tab-wrap">
            <button
              type="button"
              className="term-tab"
              aria-pressed={active === shell.key}
              onClick={() => setActive(shell.key)}
            >
              Shell {shell.number}
              {shell.exited && <span className="muted"> · exited</span>}
            </button>
            <button
              type="button"
              className="term-tab-close"
              aria-label={`Close shell ${shell.number}`}
              onClick={() => closeShell(shell)}
            >
              <svg width="8" height="8" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
                <path d="M0 0l10 10M10 0L0 10" />
              </svg>
            </button>
          </span>
        ))}
        <button type="button" className="term-tab-add" aria-label="New shell" title="New shell" onClick={addShell}>
          <svg width="10" height="10" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
            <path d="M6 1v10M1 6h10" />
          </svg>
        </button>
      </div>
      <div className="term-stack">
        <div className="term-slot" hidden={active !== CLAUDE}>
          {error && (
            <div className="banner banner-danger" role="alert">
              <WarningIcon color="var(--status-crashed)" />
              <span className="banner-text">
                <span className="banner-title">Could not start Claude Code</span>
                <span className="banner-detail">{error}</span>
              </span>
            </div>
          )}
          {launch && !error && (
            <XTerm
              key={launch.generation}
              workspaceId={workspaceId}
              kind="claude"
              launchKey={`${workspaceId}:claude:${launch.generation}`}
              source={launch.source}
              visible={visible && active === CLAUDE}
              onStarted={onClaudeStarted}
              onError={(message) => setClaudeError({ generation: launch.generation, message })}
            />
          )}
        </div>
        {shells.map((shell) => (
          <div key={shell.key} className="term-slot" hidden={active !== shell.key}>
            {shell.error ? (
              <div className="banner banner-danger" role="alert">
                <WarningIcon color="var(--status-crashed)" />
                <span className="banner-text">
                  <span className="banner-title">Could not start the shell</span>
                  <span className="banner-detail">{shell.error}</span>
                </span>
              </div>
            ) : (
              <XTerm
                workspaceId={workspaceId}
                kind="shell"
                launchKey={`${workspaceId}:shell:${shell.key}`}
                source={shell.source}
                visible={visible && active === shell.key}
                onStarted={(id) => update(shell.key, { id })}
                onExited={() => update(shell.key, { exited: true })}
                onError={(message) => update(shell.key, { error: message })}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
