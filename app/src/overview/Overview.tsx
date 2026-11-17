import { shortcutHints } from "../state/shortcuts";
import { useEffect, useRef, type CSSProperties, type KeyboardEvent } from "react";
import { KIND_LABEL, type SessionTab } from "../state/model";
import type { TabGroup } from "../state/order";
import { groupFolder } from "../state/paths";
import { formatInState, type StatusKind } from "../state/status";
import { StatusIcon } from "../shell/StatusIcon";
import "./Overview.css";

interface Props {
  groups: TabGroup<SessionTab>[];
  focusId: string | null;
  now: number;
  canResumeAll: boolean;
  onOpen: (id: string) => void;
  onNextNeedingYou: () => void;
  onNewSession: () => void;
  onResumeAll: () => void;
}

const SUMMARY: [StatusKind, string][] = [
  ["input", "waiting for input"],
  ["permission", "needs permission"],
  ["crashed", "crashed"],
  ["working", "working"],
  ["finished", "finished"],
  ["idle", "idle"],
  ["unknown", "unknown"],
];

const HIGHLIGHT: Partial<Record<StatusKind, string>> = {
  input: "var(--flash-input)",
  permission: "var(--flash-color)",
  crashed: "var(--status-crashed)",
};

function groupPath(group: TabGroup<SessionTab>): string {
  return group.key.startsWith("folder:") ? group.items[0].path : groupFolder(group.key);
}

export function Overview(props: Props) {
  const { groups, focusId, now, canResumeAll, onOpen, onNextNeedingYou, onNewSession, onResumeAll } = props;
  const listRef = useRef<HTMLDivElement>(null);
  const tabs = groups.flatMap((group) => group.items);
  const anyNeedsYou = tabs.some((tab) => tab.presentation.needsYou || tab.presentation.kind === "finished");
  const counts = new Map<StatusKind, number>();
  for (const tab of tabs) counts.set(tab.presentation.kind, (counts.get(tab.presentation.kind) ?? 0) + 1);

  useEffect(() => {
    const rows = listRef.current?.querySelectorAll<HTMLButtonElement>(".ov-row");
    if (!rows || rows.length === 0) return;
    const target = [...rows].find((row) => row.dataset.workspaceId === focusId) ?? rows[0];
    target.focus();
  }, [focusId]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const rows = [...(listRef.current?.querySelectorAll<HTMLButtonElement>(".ov-row") ?? [])];
    const at = rows.indexOf(document.activeElement as HTMLButtonElement);
    const moves: Record<string, number> = {
      ArrowDown: Math.min(rows.length - 1, at + 1),
      ArrowUp: Math.max(0, at - 1),
      Home: 0,
      End: rows.length - 1,
    };
    if (!(event.key in moves) || rows.length === 0) return;
    event.preventDefault();
    rows[moves[event.key]].focus();
  };

  return (
    <div className="overview">
      <div className="ov-summary">
        <h1>
          {tabs.length} {tabs.length === 1 ? "session" : "sessions"}
        </h1>
        {SUMMARY.filter(([kind]) => counts.has(kind)).map(([kind, text]) => (
          <span key={kind} className="ov-count">
            <StatusIcon kind={kind} />
            <span>
              <span className="ov-count-number">{counts.get(kind)}</span> {text}
            </span>
          </span>
        ))}
        <span className="ov-summary-space" />
        {canResumeAll && (
          <button type="button" className="btn" onClick={onResumeAll}>
            Resume all
          </button>
        )}
        <button type="button" className="btn btn-attention" disabled={!anyNeedsYou} onClick={onNextNeedingYou}>
          Next needing you <span className="kbd">{shortcutHints("Ctrl+J")}</span>
        </button>
        <button type="button" className="btn" onClick={onNewSession}>
          New session <span className="kbd kbd-dim">{shortcutHints("Ctrl+N")}</span>
        </button>
      </div>
      <div className="ov-grid ov-head" aria-hidden="true">
        <span>KEY</span>
        <span>STATE</span>
        <span>BRANCH</span>
        <span>FOLDER</span>
        <span>KIND</span>
        <span className="ov-right">IN STATE</span>
      </div>
      <div className="ov-list" ref={listRef} onKeyDown={onKeyDown}>
        {groups.map((group) => (
          <section key={group.key} aria-label={group.name}>
            <div className="ov-group">
              <span className="ov-group-name">{group.name}</span>
              <span className="mono muted">{groupPath(group)}</span>
              <span className="ov-summary-space" />
              <span className="muted">
                {group.items.length} {group.items.length === 1 ? "session" : "sessions"}
              </span>
            </div>
            {group.items.map((tab) => (
              <OverviewRow key={tab.id} tab={tab} now={now} onOpen={onOpen} />
            ))}
          </section>
        ))}
      </div>
    </div>
  );
}

function OverviewRow({ tab, now, onOpen }: { tab: SessionTab; now: number; onOpen: (id: string) => void }) {
  const p = tab.presentation;
  const highlight = p.needsYou || p.kind === "crashed" ? HIGHLIGHT[p.kind] : undefined;
  const time = p.since === null ? "" : formatInState(now - p.since);
  const style = highlight
    ? ({ "--row-highlight": `color-mix(in srgb, ${highlight} var(--highlight-amount), transparent)` } as CSSProperties)
    : undefined;
  return (
    <button
      type="button"
      className={`ov-grid ov-row${highlight ? " ov-row-highlight" : ""}`}
      style={style}
      data-workspace-id={tab.id}
      aria-label={`${tab.branch}, ${p.label}${time && `, ${time}`}, ${tab.path}`}
      onClick={() => onOpen(tab.id)}
    >
      <span className="mono muted">{tab.number ?? ""}</span>
      <span className="ov-state-cell">
        <span className={`ov-state${p.needsYou ? " ov-state-alert" : ""}`} style={{ color: p.needsYou ? undefined : p.colorVar }}>
          {p.needsYou && <span className={`flash flash-${p.kind}`} aria-hidden="true" />}
          <StatusIcon kind={p.kind} />
          <span className="ov-state-label">{p.label}</span>
        </span>
      </span>
      <span className="mono ov-ellipsis">{tab.branch}</span>
      <span className="mono muted ov-ellipsis">{tab.path}</span>
      <span className="muted">{tab.summary ? KIND_LABEL[tab.summary.kind] : ""}</span>
      <span className="mono ov-right">{time}</span>
    </button>
  );
}
