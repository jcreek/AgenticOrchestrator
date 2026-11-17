import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { SessionTab } from "../state/model";
import type { TabGroup } from "../state/order";
import { formatInState } from "../state/status";
import { isMerged, overflowCounts } from "../state/tabStrip";
import { StatusIcon } from "./StatusIcon";

interface Props {
  groups: TabGroup<SessionTab>[];
  activeId: string | null;
  hidden: string[];
  now: number;
  inert: boolean;
  onSelect: (id: string) => void;
}

const MENU_ID = "session-overflow-menu";

export function OverflowMenu({ groups, activeId, hidden, now, inert, onSelect }: Props) {
  const [anchor, setAnchor] = useState<{ top: number; right: number } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const open = anchor !== null;
  const counts = overflowCounts(
    hidden,
    groups.flatMap((group) => group.items.map((tab) => ({ id: tab.id, needsYou: tab.presentation.needsYou }))),
  );

  const close = (returnFocus: boolean) => {
    setAnchor(null);
    if (returnFocus) buttonRef.current?.focus();
  };

  useEffect(() => setAnchor(null), [activeId, inert]);

  useEffect(() => {
    if (!open) return;
    const items = [...(menuRef.current?.querySelectorAll<HTMLElement>("[role=menuitemradio]") ?? [])];
    (items.find((item) => item.getAttribute("aria-checked") === "true") ?? items[0])?.focus();
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menuRef.current?.contains(target) && !buttonRef.current?.contains(target)) setAnchor(null);
    };
    const dismiss = () => setAnchor(null);
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
    };
  }, [open]);

  if (counts.hidden === 0 && !open) return null;

  const toggle = () => {
    if (open) return close(false);
    const rect = buttonRef.current!.getBoundingClientRect();
    setAnchor({ top: rect.bottom + 4, right: window.innerWidth - rect.right });
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" || event.key === "Tab") {
      event.preventDefault();
      close(true);
      return;
    }
    const items = [...(menuRef.current?.querySelectorAll<HTMLElement>("[role=menuitemradio]") ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    const moves: Record<string, number> = {
      ArrowDown: (at + 1) % items.length,
      ArrowUp: (at - 1 + items.length) % items.length,
      Home: 0,
      End: items.length - 1,
    };
    if (!(event.key in moves)) return;
    event.preventDefault();
    items[moves[event.key]].focus();
  };

  const label = `${counts.hidden} more ${counts.hidden === 1 ? "session" : "sessions"}${counts.needYou > 0 ? `, ${counts.needYou} need you` : ""}`;
  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="tab-overflow"
        inert={inert}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? MENU_ID : undefined}
        title="All sessions"
        onClick={toggle}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
          <path d="M2.5 4.5L6 8l3.5-3.5" />
        </svg>
        <span aria-hidden="true">{counts.hidden}</span>
        {counts.needYou > 0 && (
          <span className="attention-badge" aria-hidden="true">
            {counts.needYou}
          </span>
        )}
      </button>
      {open && (
        <div
          ref={menuRef}
          id={MENU_ID}
          className="session-menu"
          role="menu"
          aria-label="All sessions"
          style={anchor}
          onKeyDown={onKeyDown}
        >
          {groups.map((group) => {
            const merged = isMerged(group);
            const rows = group.items.map((tab) => {
              const p = tab.presentation;
              const time = p.since === null ? "" : formatInState(now - p.since);
              return (
                <button
                  key={tab.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={tab.id === activeId}
                  tabIndex={-1}
                  className="session-menu-row"
                  onPointerEnter={(event) => event.currentTarget.focus()}
                  onClick={() => {
                    onSelect(tab.id);
                    close(true);
                  }}
                >
                  <StatusIcon kind={p.kind} />
                  <span className="session-menu-ellipsis">{merged ? group.name : tab.name}</span>
                  <span className="session-menu-ellipsis mono muted">{tab.group ? tab.branch : ""}</span>
                  <span className="session-menu-ellipsis" style={{ color: p.needsYou ? undefined : p.colorVar }}>
                    {p.label}
                  </span>
                  <span className="mono muted session-menu-time">{time}</span>
                </button>
              );
            });
            if (merged) return rows[0];
            return (
              <div key={group.key} role="group" aria-label={group.name}>
                <div className="session-menu-group" aria-hidden="true">
                  {group.name}
                </div>
                {rows}
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
