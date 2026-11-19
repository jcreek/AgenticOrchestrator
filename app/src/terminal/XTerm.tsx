import { terminalClipboard } from "../state/shortcuts";
import { Channel } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import { useEffect, useRef } from "react";
import { commands, events, type TerminalInfo_Serialize, type TerminalKind } from "../bindings";
import type { ClaudeSource } from "../state/model";
import { errorMessage } from "../state/errors";
import { useSettings } from "../state/settings";
import { matchShortcut } from "../state/shortcuts";
import { chunkText } from "./chunk";
import { terminalFontFamily } from "./font";
import { OSC52, osc52Text } from "./osc52";
import { onThemeChange, terminalTheme } from "./theme";

export type TerminalSource = ClaudeSource;

interface Props {
  workspaceId: string;
  kind: TerminalKind;
  /** Unique per launch, so a remount attaches to the terminal instead of opening another. */
  launchKey: string;
  source: TerminalSource;
  visible: boolean;
  onStarted?: (id: number) => void;
  onExited?: (exitCode: number) => void;
  onError?: (message: string) => void;
}

const HIDDEN_COLS = 120;
const HIDDEN_ROWS = 30;
const RESIZE_DEBOUNCE_MS = 50;

const launches = new Map<string, Promise<TerminalInfo_Serialize>>();
const attaches = new Map<number, Promise<unknown>>();

/** Attaches in call order per terminal, since the backend may run concurrent attaches in any order; a disposed view is skipped. */
function attachInOrder(id: number, channel: Channel<ArrayBuffer>, live: () => boolean): Promise<unknown> {
  const attached = (attaches.get(id) ?? Promise.resolve())
    .catch(() => undefined)
    .then(() => (live() ? commands.terminalAttach(id, channel) : undefined));
  attaches.set(id, attached);
  return attached;
}

/** Closes the terminal a launch opens, once it has opened; for terminals closed while still starting. */
export function closeLaunch(launchKey: string): void {
  launches.get(launchKey)?.then((info) => commands.terminalClose(info.id)).catch(() => undefined);
}

// Clearing first would make the browser's copy event for Ctrl+C put an empty selection on the clipboard.
function copy(term: Terminal): void {
  navigator.clipboard.writeText(term.getSelection()).then(() => term.clearSelection(), () => undefined);
}

export function XTerm(props: Props) {
  const mountRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const propsRef = useRef(props);
  propsRef.current = props;
  const fontFamily = terminalFontFamily(useSettings().terminalFont);
  const fontFamilyRef = useRef(fontFamily);
  fontFamilyRef.current = fontFamily;

  useEffect(() => {
    const { workspaceId, kind, launchKey, source } = propsRef.current;
    const mount = mountRef.current;
    if (!mount) return;
    const term = new Terminal({
      fontFamily: fontFamilyRef.current,
      fontSize: 12,
      lineHeight: 1.5,
      allowProposedApi: true,
      scrollback: 10_000,
      theme: terminalTheme(),
      linkHandler: { activate: () => undefined },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";
    term.open(mount);
    termRef.current = term;
    fitRef.current = fit;

    let id: number | null = null;
    let disposed = false;
    let writes: Promise<unknown> = Promise.resolve();
    const send = (data: string) => {
      const target = id;
      if (target === null) return;
      for (const part of chunkText(data)) {
        writes = writes.then(() => commands.terminalWrite(target, part)).catch(() => undefined);
      }
    };
    const resize = (cols: number, rows: number) => {
      if (id !== null) commands.terminalResize(id, cols, rows).catch(() => undefined);
    };
    const fitIfShown = () => {
      if (!disposed && mount.clientWidth > 0 && mount.clientHeight > 0) fit.fit();
    };
    fitIfShown();
    if (mount.clientWidth === 0) term.resize(HIDDEN_COLS, HIDDEN_ROWS);

    term.onData(send);
    term.onResize(({ cols, rows }) => resize(cols, rows));
    // Claude Code copies its own selection by sending OSC 52, expecting the terminal to set the clipboard.
    term.parser.registerOscHandler(OSC52, (data) => {
      const text = osc52Text(data);
      if (text !== null) navigator.clipboard.writeText(text).catch(() => undefined);
      return true;
    });
    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      if (matchShortcut(event, true)) return false;
      if (event.key === "Enter" && event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey) {
        send("\n");
        return false;
      }
      // Returning false leaves Ctrl+V to the browser, whose paste event xterm turns into a bracketed paste.
      if (terminalClipboard(event, term.hasSelection()) === "paste") return false;
      if (terminalClipboard(event, term.hasSelection()) === "copy") {
        copy(term);
        return false;
      }
      return true;
    });
    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      if (term.hasSelection()) {
        copy(term);
        return;
      }
      navigator.clipboard.readText().then((text) => term.paste(text), () => undefined);
    };
    mount.addEventListener("contextmenu", onContextMenu);

    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (buffer) => {
      if (!disposed) term.write(new Uint8Array(buffer));
    };

    const connect = async (): Promise<number> => {
      if ("attach" in source) {
        await attachInOrder(source.attach, channel, () => !disposed);
        if (kind === "claude") {
          // Claude's TUI only redraws after a real size change; the later resize restores the size.
          await commands.terminalResize(source.attach, term.cols, Math.max(1, term.rows - 1)).catch(() => undefined);
        }
        return source.attach;
      }
      let pending = launches.get(launchKey);
      const reused = pending !== undefined;
      if (!pending) {
        pending = commands.terminalOpen(workspaceId, kind, term.cols, term.rows, source.resume, source.prompt, channel);
        launches.set(launchKey, pending);
        pending.catch(() => launches.delete(launchKey));
      }
      const info = await pending;
      if (reused) await attachInOrder(info.id, channel, () => !disposed);
      return info.id;
    };
    connect().then(
      (connected) => {
        if (disposed) return;
        id = connected;
        resize(term.cols, term.rows);
        propsRef.current.onStarted?.(connected);
        const early = earlyExits.get(connected);
        if (early !== undefined) propsRef.current.onExited?.(early);
      },
      (error: unknown) => {
        if (!disposed) propsRef.current.onError?.(errorMessage(error));
      },
    );

    const earlyExits = new Map<number, number>();
    const unlisten = events.terminalExited.listen((event) => {
      if (disposed) return;
      if (id === null) earlyExits.set(event.payload.id, event.payload.exitCode);
      else if (event.payload.id === id) propsRef.current.onExited?.(event.payload.exitCode);
    });
    const stopTheme = onThemeChange(() => {
      term.options.theme = terminalTheme();
    });
    let timer: number | undefined;
    const observer = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(fitIfShown, RESIZE_DEBOUNCE_MS);
    });
    observer.observe(mount);

    return () => {
      disposed = true;
      window.clearTimeout(timer);
      observer.disconnect();
      stopTheme();
      unlisten.then((stop) => stop(), () => undefined);
      mount.removeEventListener("contextmenu", onContextMenu);
      termRef.current = null;
      fitRef.current = null;
      term.dispose();
    };
  }, []);

  useEffect(() => {
    const term = termRef.current;
    if (!term || term.options.fontFamily === fontFamily) return;
    term.options.fontFamily = fontFamily;
    if (mountRef.current && mountRef.current.clientWidth > 0) fitRef.current?.fit();
  }, [fontFamily]);

  useEffect(() => {
    const term = termRef.current;
    const mount = mountRef.current;
    if (!props.visible || !term || !mount) return;
    let webgl: WebglAddon | null = null;
    try {
      webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl?.dispose();
        webgl = null;
      });
      term.loadAddon(webgl);
    } catch {
      webgl = null;
    }
    if (mount.clientWidth > 0) fitRef.current?.fit();
    term.focus();
    return () => {
      try {
        webgl?.dispose();
      } catch {
        // The terminal may already be disposed, taking the addon with it.
      }
    };
  }, [props.visible]);

  return <div className="xterm-mount" ref={mountRef} />;
}
