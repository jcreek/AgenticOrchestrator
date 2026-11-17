import { platform } from "../state/platform";
import { shortcutHints } from "../state/shortcuts";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type PointerEvent, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { SessionTab } from "../state/model";
import type { TabGroup } from "../state/order";
import { hiddenTabs, isMerged, revealScroll } from "../state/tabStrip";
import { OverflowMenu } from "./OverflowMenu";
import { SessionTabButton } from "./SessionTabButton";
import "./TitleBar.css";

interface Props {
  groups: TabGroup<SessionTab>[];
  activeId: string | null;
  needYou: number;
  now: number;
  showTabs?: boolean;
  /** Tabs ignore input while a dialog is open; the window controls keep working. */
  inert?: boolean;
  onOverview: () => void;
  onSelect: (id: string) => void;
  onReorder: (draggedId: string, targetId: string) => void;
  onNewSession: () => void;
  onSettings: () => void;
}

interface TabDrag {
  id: string;
  overId: string | null;
}

/** Pointer travel, in pixels, before a press on a tab becomes a drag rather than a click. */
const DRAG_START = 5;

const tabIdAt = (target: EventTarget | null) =>
  target instanceof Element ? (target.closest<HTMLElement>("[data-tab-id]")?.dataset.tabId ?? null) : null;

interface StripView {
  hidden: string[];
  fadeStart: boolean;
  fadeEnd: boolean;
}

/** Matches the fade width in TitleBar.css, so a revealed tab is never under the fade. */
const FADE = 24;
const NO_OVERFLOW: StripView = { hidden: [], fadeStart: false, fadeEnd: false };

const sameView = (a: StripView, b: StripView) =>
  a.fadeStart === b.fadeStart && a.fadeEnd === b.fadeEnd && a.hidden.join() === b.hidden.join();

function OverviewIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <rect x="1.5" y="1.5" width="4.5" height="4.5" />
      <rect x="8" y="1.5" width="4.5" height="4.5" />
      <rect x="1.5" y="8" width="4.5" height="4.5" />
      <rect x="8" y="8" width="4.5" height="4.5" />
    </svg>
  );
}

export function TitleBar({ groups, activeId, needYou, now, showTabs = true, inert = false, onOverview, onSelect, onReorder, onNewSession, onSettings }: Props) {
  const stripRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<StripView>(NO_OVERFLOW);
  const pressed = useRef<{ id: string; x: number; y: number } | null>(null);
  const [drag, setDrag] = useState<TabDrag | null>(null);

  const endDrag = () => {
    pressed.current = null;
    setDrag(null);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const press = pressed.current;
    if (!press) return;
    if (event.buttons === 0) return endDrag();
    if (!drag) {
      if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_START) return;
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    const under = tabIdAt(document.elementFromPoint(event.clientX, event.clientY));
    const overId = under === press.id ? null : under;
    setDrag((current) => (current?.overId === overId ? current : { id: press.id, overId }));
  };

  const dragGroup = drag && groups.find((group) => group.items.some((tab) => tab.id === drag.id));
  const movesGroup = !!drag?.overId && !!dragGroup && !dragGroup.items.some((tab) => tab.id === drag.overId);
  const isDragging = (group: TabGroup<SessionTab>, id: string) => !!drag && (id === drag.id || (movesGroup && group === dragGroup));
  const layoutKey = groups.map((group) => `${group.key}:${group.items.map((tab) => tab.id).join()}`).join("|");

  const measure = useCallback(() => {
    const strip = stripRef.current;
    if (!strip) return;
    const spans = [...strip.querySelectorAll<HTMLElement>("[data-tab-id]")].map((tab) => ({
      id: tab.dataset.tabId!,
      start: tab.offsetLeft,
      end: tab.offsetLeft + tab.offsetWidth,
    }));
    const { scrollLeft, clientWidth, scrollWidth } = strip;
    const next = {
      hidden: hiddenTabs(spans, scrollLeft, scrollLeft + clientWidth),
      fadeStart: scrollLeft > 1,
      fadeEnd: scrollLeft + clientWidth < scrollWidth - 1,
    };
    setView((current) => (sameView(current, next) ? current : next));
  }, []);

  const revealActive = useCallback(() => {
    measure();
    const strip = stripRef.current;
    const tab = strip?.querySelector<HTMLElement>('[aria-current="page"]');
    if (!strip || !tab) return;
    const left = revealScroll(tab.offsetLeft, tab.offsetLeft + tab.offsetWidth, strip.scrollLeft, strip.clientWidth, FADE);
    if (left === null) return;
    // Long smooth scrolls are slow to land; only glide over short distances.
    const smooth = Math.abs(left - strip.scrollLeft) <= strip.clientWidth && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    strip.scrollTo({ left, behavior: smooth ? "smooth" : "instant" });
  }, [measure]);

  useEffect(() => {
    const strip = stripRef.current;
    if (!strip) return;
    const observer = new ResizeObserver(revealActive);
    observer.observe(strip);
    return () => observer.disconnect();
  }, [revealActive, showTabs]);

  useLayoutEffect(() => revealActive(), [activeId, layoutKey, revealActive]);

  const stripClass = ["tab-strip", view.fadeStart && "tab-strip-fade-start", view.fadeEnd && "tab-strip-fade-end", drag && "tab-strip-dragging"];
  return (
    <header className={platform === "macos" ? "title-bar title-bar-macos" : "title-bar"} data-tauri-drag-region>
      {showTabs && (
        <>
          <button
            type="button"
            className="overview-tab"
            inert={inert}
            aria-current={activeId === null ? "page" : undefined}
            aria-label={needYou > 0 ? `Overview, ${needYou} need you` : "Overview"}
            onClick={onOverview}
          >
            <OverviewIcon />
            <span>Overview</span>
            {needYou > 0 && (
              <span className="attention-badge" aria-hidden="true">
                {needYou}
              </span>
            )}
          </button>
          <div
            ref={stripRef}
            className={stripClass.filter(Boolean).join(" ")}
            inert={inert}
            data-tauri-drag-region
            onScroll={measure}
            onPointerDown={(event) => {
              const id = event.button === 0 ? tabIdAt(event.target) : null;
              pressed.current = id ? { id, x: event.clientX, y: event.clientY } : null;
            }}
            onPointerMove={onPointerMove}
            onPointerUp={() => {
              if (drag?.overId) onReorder(drag.id, drag.overId);
              endDrag();
            }}
            onPointerCancel={endDrag}
            onLostPointerCapture={endDrag}
            onWheel={(event) => {
              if (event.deltaX === 0) event.currentTarget.scrollLeft += event.deltaY;
            }}
          >
            {groups.map((group) => {
              const merged = isMerged(group);
              const tabs = group.items.map((tab) => (
                <SessionTabButton
                  key={tab.id}
                  tab={tab}
                  groupName={group.name}
                  merged={merged}
                  active={tab.id === activeId}
                  now={now}
                  dragging={isDragging(group, tab.id)}
                  dropTarget={tab.id === drag?.overId}
                  onSelect={onSelect}
                />
              ));
              if (merged) return tabs[0];
              return (
                <div key={group.key} className="tab-group" role="group" aria-label={group.name}>
                  <span className="tab-group-label" title={group.name} data-tauri-drag-region>
                    {group.name}
                  </span>
                  {tabs}
                </div>
              );
            })}
          </div>
          <OverflowMenu groups={groups} activeId={activeId} hidden={view.hidden} now={now} inert={inert} onSelect={onSelect} />
          <button type="button" className="new-session" inert={inert} aria-label="New session" title={shortcutHints("New session (Ctrl+N)")} onClick={onNewSession}>
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
              <path d="M6 1v10M1 6h10" />
            </svg>
          </button>
          <button type="button" className="new-session" inert={inert} aria-label="Settings" title="Settings" onClick={onSettings}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
              <circle cx="8" cy="8" r="2.5" />
              <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
            </svg>
          </button>
        </>
      )}
      <span className="title-bar-space" data-tauri-drag-region />
      {platform !== "macos" && <WindowControls />}
    </header>
  );
}

function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    const refresh = () => {
      appWindow.isMaximized().then(setMaximized, () => undefined);
    };
    refresh();
    window.addEventListener("resize", refresh);
    return () => window.removeEventListener("resize", refresh);
  }, []);

  const appWindow = () => getCurrentWindow();
  return (
    <div className="window-controls">
      <button type="button" aria-label="Minimise" onClick={() => void appWindow().minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden="true">
          <path d="M0 5h10" />
        </svg>
      </button>
      <button
        type="button"
        aria-label={maximized ? "Restore" : "Maximise"}
        onClick={() => void appWindow().toggleMaximize()}
      >
        {maximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden="true">
            <rect x=".5" y="2.5" width="7" height="7" />
            <path d="M2.5 2.5V.5h7v7h-2" />
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden="true">
            <rect x=".5" y=".5" width="9" height="9" />
          </svg>
        )}
      </button>
      <button type="button" className="window-close" aria-label="Close" onClick={() => void appWindow().close()}>
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden="true">
          <path d="M0 0l10 10M10 0L0 10" />
        </svg>
      </button>
    </div>
  );
}
