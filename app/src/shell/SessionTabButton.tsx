import type { CSSProperties } from "react";
import type { SessionTab } from "../state/model";
import { formatInState } from "../state/status";
import { describeTab, tabText } from "../state/tabStrip";
import { StatusIcon } from "./StatusIcon";

interface Props {
  tab: SessionTab;
  groupName: string;
  merged: boolean;
  active: boolean;
  now: number;
  dragging: boolean;
  dropTarget: boolean;
  onSelect: (id: string) => void;
}

export function SessionTabButton({ tab, groupName, merged, active, now, dragging, dropTarget, onSelect }: Props) {
  const { presentation: p } = tab;
  const time = p.since === null ? "" : formatInState(now - p.since);
  const flashing = p.needsYou && !active;
  const text = tabText(tab, groupName, merged);
  const description = describeTab(tab, groupName, p.label, time);
  const classes = ["tab", active && "tab-active", flashing && "tab-alert", dragging && "tab-dragging", dropTarget && "tab-drop-target"];
  return (
    <button
      type="button"
      className={classes.filter(Boolean).join(" ")}
      style={{ "--tab-color": p.colorVar } as CSSProperties}
      data-tab-id={tab.id}
      aria-current={active ? "page" : undefined}
      aria-label={description}
      title={description}
      onClick={() => onSelect(tab.id)}
    >
      {flashing && <span className={`flash flash-${p.kind}`} aria-hidden="true" />}
      {p.flashOnce && !active && <span key={p.since} className={`flash-once flash-${p.kind}`} aria-hidden="true" />}
      <StatusIcon kind={p.kind} />
      <span className="tab-text">
        <span className={merged ? "tab-name" : "tab-name tab-name-branch"}>{text.primary}</span>
        <span className="tab-status">
          {text.branch !== null && <span className="tab-status-branch">{text.branch}</span>}
          <span className="tab-status-state">
            {text.branch !== null && " · "}
            {p.tabLabel}
            {time && ` · ${time}`}
          </span>
        </span>
      </span>
    </button>
  );
}
