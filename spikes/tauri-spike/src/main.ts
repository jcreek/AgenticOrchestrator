import { Channel, invoke } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/editor/editor.worker.js?worker";
import tsWorker from "monaco-editor/language/typescript/ts.worker.js?worker";

const SHELL = "pwsh.exe";
const SHELL_ARGS = ["-NoLogo", "-NoProfile"];
const MAIN_SHELL_ARGS = ["-NoLogo"];
const EXTRA_IDLE_TERMINALS = 18;
const FLOOD_MEGABYTES = 20;
const FLOOD_END_MARKER = "AO_FLOOD_END";
const CURSOR_POSITION_QUERY = "\x1b[6n";
const CURSOR_POSITION_REPLY = "\x1b[1;1R";
const CTRL_J = "\n";
const PROBE_TIMEOUT_MS = 5000;
const TAIL_CHARS = 4096;

type Waiter = { marker: string; resolve: () => void };

interface MainTerminal {
  id: number;
  terminal: Terminal;
  waiters: Waiter[];
}

const logElement = document.querySelector<HTMLPreElement>("#log")!;
const started = performance.now();

function log(message: string): void {
  const line = `[${Math.round(performance.now() - started)}ms] ${message}`;
  logElement.textContent += `${line}\n`;
  logElement.scrollTop = logElement.scrollHeight;
  void invoke("spike_log", { line });
}

function logHeap(stage: string): void {
  const memory = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory;
  if (memory) log(`JS heap (${stage}): ${Math.round(memory.usedJSHeapSize / (1024 * 1024))} MB`);
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function toBytes(message: unknown): Uint8Array {
  if (message instanceof ArrayBuffer) return new Uint8Array(message);
  if (message instanceof Uint8Array) return message;
  return new Uint8Array(message as number[]);
}

async function openPty(
  program: string,
  args: string[],
  cols: number,
  rows: number,
  onBytes: (bytes: Uint8Array) => void,
): Promise<number> {
  const onData = new Channel<unknown>();
  onData.onmessage = (message) => onBytes(toBytes(message));
  const onExit = new Channel<number>();
  onExit.onmessage = (code) => log(`pty exited: ${program} code=${code}`);
  return invoke<number>("pty_open", { program, args, cols, rows, onData, onExit });
}

function write(id: number, data: string): Promise<void> {
  return invoke("pty_write", { id, data });
}

function installKeyLogger(): void {
  window.addEventListener(
    "keydown",
    (event) => {
      const isFunctionKey = /^F\d+$/.test(event.key);
      if (!event.ctrlKey && !event.altKey && !isFunctionKey) return;
      if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;
      const parts = [
        event.ctrlKey ? "Ctrl" : "",
        event.altKey ? "Alt" : "",
        event.shiftKey ? "Shift" : "",
        event.key,
      ].filter(Boolean);
      log(`key reached page: ${parts.join("+")}`);
    },
    true,
  );
}

function installCspLogger(): void {
  document.addEventListener("securitypolicyviolation", (event) => {
    log(`CSP violation: ${event.violatedDirective} blocked ${event.blockedURI}`);
  });
}

async function startMainTerminal(): Promise<MainTerminal> {
  const terminal = new Terminal({ fontFamily: "Consolas, monospace", fontSize: 13 });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(document.querySelector<HTMLElement>("#term")!);
  fit.fit();

  try {
    terminal.loadAddon(new WebglAddon());
    log("xterm WebGL addon: loaded");
  } catch (error) {
    log(`xterm WebGL addon: FAILED ${String(error)}`);
  }

  const main: MainTerminal = { id: -1, terminal, waiters: [] };
  const decoder = new TextDecoder();
  let tail = "";
  let firstData = true;
  const opened = performance.now();

  main.id = await openPty(SHELL, MAIN_SHELL_ARGS, terminal.cols, terminal.rows, (bytes) => {
    if (firstData) {
      firstData = false;
      log(`main terminal first output after ${Math.round(performance.now() - opened)}ms`);
    }
    terminal.write(bytes);
    tail = (tail + decoder.decode(bytes, { stream: true })).slice(-TAIL_CHARS);
    main.waiters = main.waiters.filter((waiter) => {
      if (!tail.includes(waiter.marker)) return true;
      waiter.resolve();
      return false;
    });
  });

  terminal.onData((data) => void write(main.id, data));
  terminal.onResize(({ cols, rows }) => void invoke("pty_resize", { id: main.id, cols, rows }));
  terminal.attachCustomKeyEventHandler((event) => {
    if (event.type !== "keydown") return true;
    if (event.key === "Enter" && event.shiftKey) {
      void write(main.id, CTRL_J);
      return false;
    }
    // Returning false leaves Ctrl+V to the browser, whose paste event xterm turns into a bracketed paste.
    if (event.ctrlKey && event.key.toLowerCase() === "v") return false;
    if (event.ctrlKey && event.key.toLowerCase() === "c" && terminal.hasSelection()) {
      void navigator.clipboard.writeText(terminal.getSelection());
      terminal.clearSelection();
      return false;
    }
    return true;
  });
  terminal.element?.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    if (terminal.hasSelection()) {
      void navigator.clipboard.writeText(terminal.getSelection());
      terminal.clearSelection();
      return;
    }
    navigator.clipboard
      .readText()
      .then((text) => terminal.paste(text))
      .catch((error) => log(`right-click paste FAILED ${String(error)}`));
  });
  window.addEventListener("resize", () => fit.fit());
  return main;
}

let probeCount = 0;

function probe(main: MainTerminal): Promise<number> {
  const marker = `AO_PROBE_${probeCount++}`;
  const sent = performance.now();
  return new Promise((resolve) => {
    const timeout = setTimeout(() => resolve(-1), PROBE_TIMEOUT_MS);
    main.waiters.push({
      marker,
      resolve: () => {
        clearTimeout(timeout);
        resolve(Math.round(performance.now() - sent));
      },
    });
    void write(main.id, `echo ${marker}\r`);
  });
}

async function probeSeries(main: MainTerminal, label: string, count: number): Promise<void> {
  const results: number[] = [];
  for (let index = 0; index < count; index++) {
    results.push(await probe(main));
    await sleep(150);
  }
  log(`echo latency (${label}) ms: ${results.join(", ")}`);
}

async function checkMonaco(): Promise<void> {
  try {
    self.MonacoEnvironment = {
      getWorker: (_workerId: string, label: string) =>
        label === "typescript" || label === "javascript" ? new tsWorker() : new editorWorker(),
    };
    const before = "fn main() {\n    println!(\"one\");\n}\n";
    const after = "fn main() {\n    println!(\"two\");\n    println!(\"three\");\n}\n";
    const languages = ["rust", "csharp", "typescript", "json", "powershell"];
    const registered = new Set(monaco.languages.getLanguages().map((language) => language.id));
    log(`monaco languages registered: ${languages.filter((id) => registered.has(id)).join(", ")}`);

    const diffEditor = monaco.editor.createDiffEditor(document.querySelector<HTMLElement>("#editor")!, {
      theme: "vs-dark",
      automaticLayout: true,
    });
    const diffComputed = new Promise<number>((resolve, reject) => {
      diffEditor.onDidUpdateDiff(() => resolve(diffEditor.getLineChanges()?.length ?? 0));
      setTimeout(() => reject(new Error("diff not computed within 10s")), 10_000);
    });
    diffEditor.setModel({
      original: monaco.editor.createModel(before, "rust"),
      modified: monaco.editor.createModel(after, "rust"),
    });
    log(`monaco diff editor: ${await diffComputed} change block(s) computed`);

    await sleep(1000);
    const tokenClasses = new Set(
      [...document.querySelectorAll("#editor .view-line span span")].map((span) => span.className),
    );
    log(`monaco syntax highlighting: ${tokenClasses.size} distinct token style(s) rendered`);
  } catch (error) {
    log(`monaco: FAILED ${String(error)}`);
  }
}

async function spawnIdleTerminals(count: number): Promise<void> {
  const began = performance.now();
  let answered = 0;
  for (let index = 0; index < count; index++) {
    const decoder = new TextDecoder();
    let replied = false;
    const id = await openPty(SHELL, SHELL_ARGS, 120, 30, (bytes) => {
      if (replied || !decoder.decode(bytes, { stream: true }).includes(CURSOR_POSITION_QUERY)) return;
      replied = true;
      answered++;
      void write(id, CURSOR_POSITION_REPLY);
    });
  }
  await sleep(3000);
  log(
    `spawned ${count} extra shells in ${Math.round(performance.now() - began - 3000)}ms; ` +
      `${answered} sent the startup cursor query`,
  );
}

async function floodTest(main: MainTerminal): Promise<void> {
  const path = await invoke<string>("make_flood_file", { megabytes: FLOOD_MEGABYTES });
  const terminal = new Terminal({ cols: 120, rows: 30, fontFamily: "Consolas, monospace", fontSize: 11 });
  terminal.open(document.querySelector<HTMLElement>("#flood")!);

  const decoder = new TextDecoder();
  let tail = "";
  let received = 0;
  let parsed = 0;
  let chunks = 0;
  let sawEnd = false;
  let finish: () => void = () => {};
  const finished = new Promise<void>((resolve) => (finish = resolve));
  const began = performance.now();

  const id = await openPty("cmd.exe", ["/c", "type", path], 120, 30, (bytes) => {
    received += bytes.length;
    chunks++;
    tail = (tail + decoder.decode(bytes, { stream: true })).slice(-64);
    if (!sawEnd && tail.includes(FLOOD_END_MARKER)) {
      sawEnd = true;
      log(`flood: last byte over IPC after ${Math.round(performance.now() - began)}ms`);
    }
    terminal.write(bytes, () => {
      parsed += bytes.length;
      if (sawEnd && parsed === received) finish();
    });
  });
  terminal.onData((data) => void write(id, data));

  const probing = (async () => {
    while (!sawEnd) {
      await probeSeries(main, "during flood", 3);
    }
  })();

  await Promise.race([finished, sleep(120_000)]);
  const elapsed = performance.now() - began;
  await probing;
  const megabytes = received / (1024 * 1024);
  log(
    `flood: ${FLOOD_MEGABYTES}MB file -> ${megabytes.toFixed(1)}MB received in ${chunks} chunks, ` +
      `parsed by xterm after ${Math.round(elapsed)}ms (${(megabytes / (elapsed / 1000)).toFixed(1)} MB/s), ` +
      `complete=${sawEnd && parsed === received}`,
  );
}

async function selfTest(): Promise<void> {
  installKeyLogger();
  installCspLogger();
  log(`user agent: ${navigator.userAgent}`);

  const main = await startMainTerminal();
  document.querySelector("#open-claude")!.addEventListener("click", () => {
    void write(main.id, "claude\r");
    main.terminal.focus();
  });

  main.terminal.focus();
  await sleep(4000);
  await probeSeries(main, "idle", 8);
  if (!(await invoke<boolean>("selftest_requested"))) {
    log("ready (stress self-test skipped)");
    return;
  }
  logHeap("one terminal");
  await checkMonaco();
  logHeap("with monaco");
  await spawnIdleTerminals(EXTRA_IDLE_TERMINALS);
  await probeSeries(main, "20 terminals open", 5);
  logHeap("20 terminals");
  await floodTest(main);
  await probeSeries(main, "after flood", 5);
  logHeap("after flood");
  log("SELFTEST DONE");
  main.terminal.focus();
}

selfTest().catch((error) => log(`SELFTEST FAILED ${String(error)}`));
