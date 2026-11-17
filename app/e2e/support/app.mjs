import { execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { APP_EXE, PROC_EXE, STUB_EXE } from "./paths.mjs";
import { sleep, until } from "./wait.mjs";

const STRIPPED = /^(NO_COLOR|CLAUDECODE|CLAUDE_PID|CLAUDE_CODE_.*)$/i;

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/** The app's environment: ours minus Claude session variables, isolated folders, and the stub Claude first on PATH. */
function appEnvironment({ localAppData, webviewData, stubDir, port }) {
  const env = {};
  for (const [name, value] of Object.entries(process.env)) if (!STRIPPED.test(name)) env[name] = value;
  const pathKey = Object.keys(env).find((name) => name.toUpperCase() === "PATH") ?? "PATH";
  env[pathKey] = `${dirname(STUB_EXE)};${env[pathKey]}`;
  env.LOCALAPPDATA = localAppData;
  env.WEBVIEW2_USER_DATA_FOLDER = webviewData;
  env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--remote-debugging-port=${port}`;
  env.AO_STUB_CLAUDE_DIR = stubDir;
  return env;
}

const WINDOW_STATE_FILE = join(process.env.APPDATA, "dev.agenticorchestrator.bdd", ".window-state.json");
const PARKED = { width: 1400, height: 900, x: -1380, y: -880 };

// The window-state plugin restores a position only if a corner is on a monitor,
// and focuses the window unless its saved state says hidden.
function parkWindowOffScreen() {
  const state = { ...PARKED, prev_x: PARKED.x, prev_y: PARKED.y, maximized: false, visible: false, decorated: false, fullscreen: false };
  mkdirSync(dirname(WINDOW_STATE_FILE), { recursive: true });
  writeFileSync(WINDOW_STATE_FILE, JSON.stringify({ main: state }));
}

/** Every running process as { pid, ppid, exe, created }; with `listed`, also exited ones something still holds open. */
export function processes({ listed = false } = {}) {
  return JSON.parse(execFileSync(PROC_EXE, { encoding: "utf8" })).filter((entry) => listed || entry.running);
}

/** Processes started, directly or not, by `rootPid`; a "child" older than its parent only shares a reused pid. */
export function descendants(rootPid) {
  const all = processes({ listed: true });
  const found = [];
  const queue = all.filter((entry) => entry.pid === rootPid);
  while (queue.length) {
    const parent = queue.shift();
    for (const child of all.filter((entry) => entry.ppid === parent.pid && entry.pid !== parent.pid && entry.created >= parent.created)) {
      found.push(child);
      queue.push(child);
    }
  }
  return found;
}

/** Waits until none of `entries` is running, matching pid, exe and start time so a reused pid is not mistaken. */
export async function assertStopped(entries, when) {
  const identity = (entry) => `${entry.pid}|${entry.exe}|${entry.created}`;
  const stillRunning = () => {
    const now = new Set(processes().map(identity));
    return entries.filter((entry) => now.has(identity(entry)));
  };
  try {
    await until(`every process to stop ${when}`, () => stillRunning().length === 0, 20_000);
  } catch {
    const left = stillRunning();
    throw new Error(`processes still running ${when}: ${left.map((entry) => `${entry.exe} (${entry.pid})`).join(", ")}`);
  }
}

async function cdpConnect(url, onEvent) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = () => reject(new Error(`could not connect to the page at ${url}`));
  });
  let nextId = 0;
  const pending = new Map();
  socket.onmessage = (message) => {
    const data = JSON.parse(message.data);
    if (data.id && pending.has(data.id)) {
      const { resolve, reject } = pending.get(data.id);
      pending.delete(data.id);
      if (data.error) reject(new Error(`${data.error.message} (${JSON.stringify(data.error)})`));
      else resolve(data.result);
    } else if (data.method) onEvent(data.method, data.params);
  };
  socket.onclose = () => {
    for (const { reject } of pending.values()) reject(new Error("the page connection closed"));
    pending.clear();
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      if (socket.readyState !== WebSocket.OPEN) return reject(new Error("the page connection is closed"));
      const id = ++nextId;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  return { send, close: () => socket.close() };
}

/** One running copy of the app, driven over the Chrome DevTools Protocol. */
export class App {
  static async launch({ root, stubDir, launchNumber }) {
    const port = await freePort();
    const localAppData = join(root, "appdata");
    mkdirSync(localAppData, { recursive: true });
    const env = appEnvironment({ localAppData, webviewData: join(root, `webview-${launchNumber}`), stubDir, port });
    parkWindowOffScreen();
    const app = new App(spawn(APP_EXE, [], { env, stdio: "ignore" }));
    const target = await until(`the app window to accept a debugger on port ${port}`, async () => {
      if (app.exited) throw new Error("the app exited during start-up");
      const list = await fetch(`http://127.0.0.1:${port}/json`).then((response) => response.json(), () => []);
      return list.find((entry) => entry.type === "page" && entry.url.startsWith("http://tauri.localhost"));
    }, 60_000);
    app.cdp = await cdpConnect(target.webSocketDebuggerUrl, (method, params) => app.onEvent(method, params));
    await app.cdp.send("Runtime.enable");
    await app.cdp.send("Log.enable");
    await app.cdp.send("Page.enable");
    // During a run the window is rarely in the foreground, and the app counts a tab as viewed only while focused.
    await app.cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
    await app.waitUntilLoaded();
    app.remember();
    return app;
  }

  constructor(child) {
    this.child = child;
    this.pid = child.pid;
    this.exited = false;
    this.errors = [];
    this.known = new Map();
    child.on("exit", () => (this.exited = true));
  }

  onEvent(method, params) {
    if (method === "Runtime.exceptionThrown") {
      const details = params.exceptionDetails;
      this.errors.push(`exception: ${details.exception?.description ?? details.text}`);
    } else if (method === "Runtime.consoleAPICalled" && params.type === "error") {
      this.errors.push(`console.error: ${params.args.map((arg) => arg.value ?? arg.description ?? arg.type).join(" ")}`);
    } else if (method === "Log.entryAdded" && params.entry.level === "error") {
      this.errors.push(`log: ${params.entry.text} ${params.entry.url ?? ""}`.trim());
    }
  }

  async evaluate(expression) {
    const result = await this.cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (result.exceptionDetails) {
      throw new Error(`page script failed: ${result.exceptionDetails.exception?.description ?? result.exceptionDetails.text}`);
    }
    return result.result.value;
  }

  async waitUntilLoaded() {
    await until("the app to finish loading", () =>
      this.evaluate(`document.readyState === 'complete' && !!document.querySelector('.overview-tab, .fatal')`),
    );
    await this.evaluate(`window.dispatchEvent(new FocusEvent('focus'))`);
  }

  async reload() {
    await this.cdp.send("Page.reload");
    await sleep(200);
    await this.waitUntilLoaded();
  }

  /** Calls a backend command the way the page does; resolves to { ok } or { error }. */
  invoke(command, args = {}) {
    return this.evaluate(
      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)}).then((ok) => ({ ok }), (error) => ({ error }))`,
    );
  }

  async screenshot() {
    const { data } = await this.cdp.send("Page.captureScreenshot", { format: "png" });
    return Buffer.from(data, "base64");
  }

  /** Records the processes the app has started so far, so they can be checked after it stops. */
  remember() {
    if (this.exited) return [];
    const found = descendants(this.pid);
    for (const entry of found) this.known.set(entry.pid, entry);
    return found;
  }

  /** Stops the app and everything it started; fails if any of those processes is left running. */
  async stop() {
    this.remember();
    this.cdp?.close();
    if (!this.exited) {
      // taskkill exits non-zero when part of the tree has already gone; the check below is what counts.
      spawnSync("taskkill", ["/PID", String(this.pid), "/T", "/F"], { stdio: "ignore" });
    }
    await until("the app to exit", () => this.exited);
    await assertStopped([...this.known.values()], "after the app stopped");
  }
}
