import { createQuitController } from "./state/quit";
import { platform } from "./state/platform";
import { shortcutHints } from "./state/shortcuts";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import {
  commands,
  events,
  type SessionStatus,
  type Settings_Serialize,
  type StatusChanged_Serialize,
  type TerminalInfo_Serialize,
  type ToolCheck,
  type Update,
} from "./bindings";
import { CloseSessionDialog } from "./dialogs/CloseSessionDialog";
import { NewSessionDialog } from "./dialogs/NewSessionDialog";
import { QuitDialog } from "./dialogs/QuitDialog";
import { Toasts } from "./notifications/Toasts";
import { Empty } from "./overview/Empty";
import { Overview } from "./overview/Overview";
import { SessionView } from "./session/SessionView";
import { SessionCounts, StatusBar } from "./shell/StatusBar";
import { WarningIcon } from "./shell/StatusIcon";
import { TitleBar } from "./shell/TitleBar";
import { errorKind, errorMessage } from "./state/errors";
import { closeGuard } from "./state/closeGuard";
import { useNow, usePageVisible, useWindowFocus } from "./state/hooks";
import { useSummaries } from "./state/useSummaries";
import { DEFAULT_LAYOUT, forgetLayout, loadLayout, nextMode, saveLayout, type Layout, type LayoutMode } from "./state/layout";
import { SettingsContext } from "./state/settings";
import { SettingsDialog } from "./dialogs/SettingsDialog";
import type { ClaudeLaunch, SessionTab, Workspace } from "./state/model";
import { cycle, groupTabs, moveTab, nextNeedingYou } from "./state/order";
import { groupFolder } from "./state/paths";
import { matchShortcut, type Shortcut } from "./state/shortcuts";
import { presentMissingFolder, presentStatus } from "./state/status";
import { batchReducer, initialBatch, stickyToasts } from "./state/toasts";
import "./App.css";

const SNOOZE_MS = 10 * 60_000;
const SESSION_HINTS = shortcutHints("Ctrl+J next needing you · Ctrl+1–9 tab · Ctrl+0 overview · Ctrl+\\ layout");
const OVERVIEW_HINTS = shortcutHints("Arrows move · Enter open tab · Ctrl+J next needing you · Ctrl+1–9 tab · Ctrl+W close session · Ctrl+N new");
const NO_SHELLS: TerminalInfo_Serialize[] = [];

type Boot = { state: "loading" } | { state: "ready" } | { state: "failed"; unreadable: boolean; message: string };
type DialogState = null | { type: "new" } | { type: "close"; id: string } | { type: "quit" } | { type: "settings" };

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

export default function App() {
  const [boot, setBoot] = useState<Boot>({ state: "loading" });
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [settings, setSettings] = useState<Settings_Serialize | null>(null);
  const [statuses, setStatuses] = useState<StatusChanged_Serialize>({ workspaces: {}, unavailable: false });
  const [launches, setLaunches] = useState<Record<string, ClaudeLaunch>>({});
  const [initialShells, setInitialShells] = useState<Record<string, TerminalInfo_Serialize[]>>({});
  const [layouts, setLayouts] = useState<Record<string, Layout>>({});
  const [activeId, setActiveId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [tools, setTools] = useState<ToolCheck | null>(null);
  const [nerdFont, setNerdFont] = useState<string | null>(null);
  const [update, setUpdate] = useState<Update | null>(null);
  const [toolsDismissed, setToolsDismissed] = useState(false);
  const [appError, setAppError] = useState<string | null>(null);
  const [batch, dispatchBatch] = useReducer(batchReducer, initialBatch);
  const [dismissed, setDismissed] = useState<Record<string, number>>({});
  const [snoozed, setSnoozed] = useState<Record<string, number>>({});
  const [dirty, setDirty] = useState<Record<string, boolean>>({});
  const now = useNow();
  const focused = useWindowFocus();
  const pageVisible = usePageVisible();
  const listSeq = useRef(0);
  const guardRef = useRef<ReturnType<typeof closeGuard>>(null);
  guardRef.current = closeGuard(statuses.workspaces, dirty);
  const activeRef = useRef(activeId);
  activeRef.current = activeId;
  const lastSessionRef = useRef<string | null>(null);
  const previousStatus = useRef<Record<string, SessionStatus> | null>(null);

  const defaultMode = settings?.defaultLayout ?? DEFAULT_LAYOUT.mode;

  const adopt = useCallback((added: Workspace[], mode: LayoutMode) => {
    setLayouts((current) => {
      const next = { ...current };
      for (const workspace of added) next[workspace.id] ??= loadLayout(workspace.id, mode);
      return next;
    });
  }, []);

  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const updateSettings = useCallback((changes: Partial<Settings_Serialize>) => {
    if (!settingsRef.current) return;
    const next = { ...settingsRef.current, ...changes };
    settingsRef.current = next;
    setSettings(next);
    commands.updateSettings(next).catch((error: unknown) => setAppError(errorMessage(error)));
  }, []);
  const settingsAccess = useMemo(
    () => settings && { settings, terminalFont: settings.terminalFont || nerdFont || "", update: updateSettings },
    [settings, nerdFont, updateSettings],
  );

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const state = await commands.getState();
        const terminals = await Promise.all(
          state.workspaces.map((workspace) => commands.listTerminals(workspace.id).catch(() => [])),
        );
        if (cancelled) return;
        const restored: Record<string, ClaudeLaunch> = {};
        const shells: Record<string, TerminalInfo_Serialize[]> = {};
        state.workspaces.forEach((workspace, index) => {
          const claude = terminals[index].filter((info) => info.kind === "claude");
          const newest = claude[claude.length - 1];
          if (newest) restored[workspace.id] = { generation: 1, source: { attach: newest.id }, startedAt: 0 };
          shells[workspace.id] = terminals[index].filter((info) => info.kind === "shell");
        });
        setWorkspaces(state.workspaces);
        setSettings(state.settings);
        setLaunches(restored);
        setInitialShells(shells);
        adopt(state.workspaces, state.settings.defaultLayout);
        setBoot({ state: "ready" });
      } catch (error) {
        if (!cancelled) {
          setBoot({ state: "failed", unreadable: errorKind(error) === "stateUnreadable", message: errorMessage(error) });
        }
      }
    };
    void load();
    commands.checkTools().then(setTools, () => undefined);
    commands.detectNerdFont().then(setNerdFont, () => undefined);
    commands.checkForUpdate().then(setUpdate, () => undefined);
    commands.getStatuses().then(setStatuses, () => undefined);
    const unlisten = events.statusChanged.listen((event) => setStatuses(event.payload));
    return () => {
      cancelled = true;
      unlisten.then((stop) => stop(), () => undefined);
    };
  }, [adopt]);

  const { summaries, forgetSummary } = useSummaries(
    workspaces.map((workspace) => workspace.id),
    activeId,
    pageVisible,
    focused,
    statuses,
  );

  useEffect(() => {
    commands.setViewedWorkspace(focused ? activeId : null).catch(() => undefined);
  }, [activeId, focused]);

  useEffect(() => {
    if (activeId) dispatchBatch({ type: "forget", id: activeId });
  }, [activeId]);

  const quitController = useMemo(() => createQuitController({
    needsConfirmation: () => Boolean(guardRef.current && readyRef.current),
    showConfirmation: () => setDialog({ type: "quit" }),
    hideConfirmation: () => setDialog(null),
    close: () => platform === "macos" ? commands.approveQuit() : getCurrentWindow().destroy(),
    onError: (error) => setAppError(errorMessage(error)),
  }), []);

  useEffect(() => {
    let disposed = false;
    const quitListener = events.quitRequested.listen(() => quitController.request());
    void quitListener.then(async () => {
      if (disposed) return;
      await commands.quitGuardReady(true);
    }).catch((error: unknown) => setAppError(errorMessage(error)));
    const stop = getCurrentWindow().onCloseRequested((event) => quitController.request(event));
    return () => {
      disposed = true;
      void commands.quitGuardReady(false).catch(() => undefined);
      void quitListener.then((unlisten) => unlisten(), () => undefined);
      stop.then((unlisten) => unlisten(), () => undefined);
    };
  }, [quitController]);

  useEffect(() => {
    const before = previousStatus.current;
    if (before && !statuses.unavailable) {
      for (const [id, entry] of Object.entries(statuses.workspaces)) {
        if (entry.status === "doneUnseen" && before[id] && before[id] !== "doneUnseen" && id !== activeRef.current) {
          dispatchBatch({ type: "finished", id, now: Date.now() });
        }
      }
    }
    previousStatus.current = Object.fromEntries(
      Object.entries(statuses.workspaces).map(([id, entry]) => [id, entry.status]),
    );
  }, [statuses]);

  useEffect(() => dispatchBatch({ type: "tick", now }), [now]);

  const tabs: SessionTab[] = workspaces.map((workspace) => {
    const summary = summaries[workspace.id];
    return {
      id: workspace.id,
      name: workspace.name,
      path: workspace.path,
      group: summary?.group ?? null,
      workspace,
      summary,
      branch: summary?.branch ?? workspace.name,
      presentation: summary?.folderMissing
        ? presentMissingFolder()
        : presentStatus(statuses.workspaces[workspace.id], statuses.unavailable, now),
      number: null,
    };
  });
  const groups = groupTabs(tabs);
  const ordered = groups.flatMap((group) => group.items);
  ordered.forEach((tab, index) => {
    tab.number = index < 9 ? index + 1 : null;
  });
  const byId = new Map(tabs.map((tab) => [tab.id, tab]));
  const activeTab = activeId ? byId.get(activeId) : undefined;
  const activeSince = activeTab?.presentation.since ?? null;

  useEffect(() => {
    if (activeId && activeSince !== null) setDismissed((current) => ({ ...current, [activeId]: activeSince }));
  }, [activeId, activeSince]);

  const select = (id: string | null) => {
    if (id === null && activeRef.current) lastSessionRef.current = activeRef.current;
    setActiveId(id);
  };

  const startClaude = useCallback(
    (id: string, resume: boolean, prompt: string | null = null) =>
      setLaunches((current) => ({
        ...current,
        [id]: { generation: (current[id]?.generation ?? 0) + 1, source: { resume, prompt }, startedAt: Date.now() },
      })),
    [],
  );

  const refreshWorkspaces = useCallback(() => {
    const seq = ++listSeq.current;
    commands.getState().then((state) => {
      if (seq === listSeq.current) setWorkspaces(state.workspaces);
    }, () => undefined);
  }, []);

  const forget = useCallback((id: string) => {
    listSeq.current += 1;
    setWorkspaces((current) => current.filter((workspace) => workspace.id !== id));
    const without = <T,>(record: Record<string, T>) => {
      const next = { ...record };
      delete next[id];
      return next;
    };
    setLaunches(without);
    setInitialShells(without);
    forgetSummary(id);
    setLayouts(without);
    setDirty(without);
    forgetLayout(id);
    dispatchBatch({ type: "forget", id });
    if (activeRef.current === id) setActiveId(null);
    if (lastSessionRef.current === id) lastSessionRef.current = null;
  }, [forgetSummary]);

  const reorder = (draggedId: string, targetId: string) => {
    const ids = moveTab(groups, draggedId, targetId);
    listSeq.current += 1;
    setWorkspaces((current) => [...current].sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id)));
    select(draggedId);
    commands.reorderWorkspaces(ids).then(refreshWorkspaces, refreshWorkspaces);
  };

  const removeMissing = useCallback(
    (id: string) => {
      commands.removeWorkspace(id).then(
        () => forget(id),
        (error: unknown) => setAppError(errorMessage(error)),
      );
    },
    [forget],
  );

  const setLayout = useCallback((id: string, layout: Layout) => {
    setLayouts((current) => ({ ...current, [id]: layout }));
    saveLayout(id, layout);
  }, []);

  const closeSession = useCallback((id: string) => setDialog({ type: "close", id }), []);
  const retryStatus = useCallback(() => commands.getStatuses().then(setStatuses, () => undefined), []);
  const setDirtyFor = useCallback(
    (id: string, value: boolean) => setDirty((current) => (!!current[id] === value ? current : { ...current, [id]: value })),
    [],
  );

  const notStarted = ordered.filter((tab) => !launches[tab.id] && !tab.summary?.folderMissing);
  const resumeAll = () => {
    for (const tab of notStarted) startClaude(tab.id, tab.workspace.claudeSessionId !== null);
  };

  const jumpId = nextNeedingYou(
    ordered.map((tab) => ({ id: tab.id, kind: tab.presentation.kind, needsYou: tab.presentation.needsYou })),
    activeId,
  );
  const goNextNeedingYou = () => {
    if (jumpId) select(jumpId);
  };

  const runShortcut = (shortcut: Shortcut) => {
    switch (shortcut.type) {
      case "nextNeedingYou":
        goNextNeedingYou();
        return;
      case "tab": {
        const tab = ordered[shortcut.index];
        if (tab) select(tab.id);
        return;
      }
      case "overview":
        select(null);
        return;
      case "newSession":
        setDialog({ type: "new" });
        return;
      case "closeSession": {
        const focusedRow = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>("[data-workspace-id]");
        const id = activeId ?? focusedRow?.dataset.workspaceId;
        if (id && byId.has(id)) setDialog({ type: "close", id });
        return;
      }
      case "cycleLayout":
        if (activeId) {
          const layout = layouts[activeId] ?? loadLayout(activeId, defaultMode);
          setLayout(activeId, { ...layout, mode: nextMode(layout.mode) });
        }
        return;
      case "cycleTab": {
        const positions = [null, ...ordered.map((tab) => tab.id)];
        select(positions[cycle(positions.indexOf(activeId), positions.length, shortcut.step)]);
        return;
      }
    }
  };
  const shortcutRef = useRef(runShortcut);
  shortcutRef.current = runShortcut;
  const dialogRef = useRef(dialog);
  dialogRef.current = dialog;
  const readyRef = useRef(false);
  readyRef.current = boot.state === "ready";

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const shortcut = matchShortcut(event, event.target instanceof Element && event.target.closest(".xterm") !== null);
      if (!shortcut) return;
      event.preventDefault();
      event.stopPropagation();
      if (!dialogRef.current && readyRef.current) shortcutRef.current(shortcut);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  if (boot.state !== "ready" || !settings || !settingsAccess) {
    return (
      <div className="app">
        <TitleBar groups={[]} activeId={null} needYou={0} now={now} showTabs={false} onOverview={() => undefined} onSelect={() => undefined} onReorder={() => undefined} onNewSession={() => undefined} onSettings={() => undefined} />
        {boot.state === "failed" && (
          <div className="fatal" role="alert">
            <WarningIcon color="var(--status-crashed)" />
            <h1>{boot.unreadable ? "Saved sessions could not be read" : "The app could not start"}</h1>
            <pre>{boot.message}</pre>
            <p>
              {boot.unreadable
                ? "Nothing has been changed or overwritten. Fix or move the state file, then restart the app."
                : "Restart the app to try again."}
            </p>
          </div>
        )}
      </div>
    );
  }

  const needYou = ordered.filter((tab) => tab.presentation.needsYou).length;
  const countOf = (...kinds: string[]) => ordered.filter((tab) => kinds.includes(tab.presentation.kind)).length;
  const missingTools = tools ? (["claude", "git", "code"] as const).filter((tool) => !tools[tool]) : [];
  const knownRepos = [
    ...new Set(Object.values(summaries).flatMap((summary) => (summary.group ? [groupFolder(summary.group.key)] : []))),
  ];
  const initialRepo = activeTab?.group ? groupFolder(activeTab.group.key) : (activeTab?.path ?? knownRepos[0] ?? "");
  const sticky = stickyToasts(
    ordered.map((tab) => ({ ...tab, kind: tab.presentation.kind, needsYou: tab.presentation.needsYou, since: tab.presentation.since })),
    { activeId, dismissed, snoozedUntil: snoozed, now },
  ).map((entry) => byId.get(entry.id)!);
  const finishedTabs = (batch.shown?.ids ?? []).flatMap((id) => byId.get(id) ?? []);
  const closing = dialog?.type === "close" ? byId.get(dialog.id) : undefined;
  const closingStatus = closing ? statuses.workspaces[closing.id] : undefined;
  const dialogOpen = dialog !== null && (dialog.type !== "close" || closing !== undefined);
  const repoCount = groups.filter((group) => !group.key.startsWith("folder:")).length;
  const worktreeCount = ordered.filter((tab) => tab.summary?.kind === "worktree").length;

  return (
    <SettingsContext.Provider value={settingsAccess}>
    <div className="app">
      <TitleBar
        groups={groups}
        activeId={activeId}
        needYou={needYou}
        now={now}
        inert={dialogOpen}
        onOverview={() => select(null)}
        onSelect={select}
        onReorder={reorder}
        onNewSession={() => setDialog({ type: "new" })}
        onSettings={() => setDialog({ type: "settings" })}
        updateAvailable={update !== null}
      />
      {missingTools.length > 0 && !toolsDismissed && (
        <div className="banner banner-warning" role="alert">
          <WarningIcon color="var(--status-input)" />
          <span className="banner-text">
            <span className="banner-title">Not found on PATH: {missingTools.join(", ")}</span>
            <span className="banner-detail">
              Sessions need Claude Code (claude) and git. VS Code (code) is only used to open files.
            </span>
          </span>
          <button type="button" className="btn btn-sm" onClick={() => setToolsDismissed(true)}>
            Dismiss
          </button>
        </div>
      )}
      {appError && (
        <div className="banner banner-danger" role="alert">
          <WarningIcon color="var(--status-crashed)" />
          <span className="banner-text">
            <span className="banner-title">Something went wrong</span>
            <span className="banner-detail">{appError}</span>
          </span>
          <button type="button" className="btn btn-sm" onClick={() => setAppError(null)}>
            Dismiss
          </button>
        </div>
      )}
      <main className="app-main" inert={dialogOpen}>
        {activeId === null &&
          (workspaces.length === 0 ? (
            <Empty onNewSession={() => setDialog({ type: "new" })} />
          ) : (
            <Overview
              groups={groups}
              focusId={lastSessionRef.current}
              now={now}
              canResumeAll={notStarted.length > 0}
              onOpen={select}
              onNextNeedingYou={goNextNeedingYou}
              onNewSession={() => setDialog({ type: "new" })}
              onResumeAll={resumeAll}
            />
          ))}
        {tabs.map((tab) => (
          <SessionView
            key={tab.id}
            workspace={tab.workspace}
            summary={tab.summary}
            status={statuses.workspaces[tab.id]}
            unavailable={statuses.unavailable}
            launch={launches[tab.id]}
            initialShells={initialShells[tab.id] ?? NO_SHELLS}
            layout={layouts[tab.id] ?? loadLayout(tab.id, defaultMode)}
            visible={tab.id === activeId}
            onLayout={setLayout}
            onStart={startClaude}
            onClaudeStarted={refreshWorkspaces}
            onClose={closeSession}
            onRemove={removeMissing}
            onRetryStatus={retryStatus}
            onDirtyChange={setDirtyFor}
            onNotesSaved={refreshWorkspaces}
          />
        ))}
      </main>
      {activeId !== null ? (
        <StatusBar hints={SESSION_HINTS}>
          <SessionCounts
            counts={{ needYou, working: countOf("working"), finished: countOf("finished"), idle: countOf("idle") }}
          />
        </StatusBar>
      ) : workspaces.length === 0 ? (
        <StatusBar hints={shortcutHints("Ctrl+N new session")}>
          <span>0 sessions</span>
        </StatusBar>
      ) : (
        <StatusBar hints={OVERVIEW_HINTS}>
          <span>{plural(repoCount, "repo", "repos")}</span>
          <span>{plural(worktreeCount, "worktree", "worktrees")}</span>
        </StatusBar>
      )}
      <Toasts
        sticky={sticky}
        finished={finishedTabs}
        jumpId={jumpId}
        onGoTo={select}
        onSnooze={(id) => setSnoozed((current) => ({ ...current, [id]: Date.now() + SNOOZE_MS }))}
        onDismiss={(id) => {
          const since = byId.get(id)?.presentation.since;
          if (since != null) setDismissed((current) => ({ ...current, [id]: since }));
        }}
        onShowOverview={() => {
          dispatchBatch({ type: "dismiss" });
          select(null);
        }}
        onDismissFinished={() => dispatchBatch({ type: "dismiss" })}
      />
      {dialog?.type === "new" && (
        <NewSessionDialog
          workspaces={workspaces}
          knownRepos={knownRepos}
          initialRepo={initialRepo}
          worktreeRoot={settings.worktreeRoot}
          defaultWhere={settings.defaultWhere}
          onCreated={(workspace, prompt) => {
            listSeq.current += 1;
            setWorkspaces((current) => [...current, workspace]);
            setInitialShells((current) => ({ ...current, [workspace.id]: [] }));
            adopt([workspace], defaultMode);
            startClaude(workspace.id, false, prompt);
            setActiveId(workspace.id);
          }}
          onClose={() => setDialog(null)}
        />
      )}
      {closing && (
        <CloseSessionDialog
          tab={closing}
          runningLabel={closingStatus && closingStatus.status !== "exited" ? closing.presentation.label : null}
          unsaved={dirty[closing.id] === true}
          onClosed={(id) => {
            setDialog(null);
            forget(id);
          }}
          onCancel={() => setDialog(null)}
        />
      )}
      {dialog?.type === "quit" && (
        <QuitDialog
          guard={guardRef.current ?? { running: 0, unsaved: 0 }}
          onQuit={() => void quitController.approve()}
          onCancel={() => quitController.cancel()}
        />
      )}
      {dialog?.type === "settings" && (
        <SettingsDialog settings={settings} nerdFont={nerdFont} update={update} onSave={updateSettings} onClose={() => setDialog(null)} />
      )}
    </div>
    </SettingsContext.Provider>
  );
}
