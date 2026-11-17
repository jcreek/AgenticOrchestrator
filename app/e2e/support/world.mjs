import { After, AfterAll, Before, BeforeAll, Status, World, setDefaultTimeout, setWorldConstructor } from "@cucumber/cucumber";
import { existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { App, assertStopped } from "./app.mjs";
import { APP_EXE, ARTEFACTS, PROC_EXE, RUN_ROOT, STUB_EXE } from "./paths.mjs";
import { sleep } from "./wait.mjs";

setDefaultTimeout(120_000);

const DEFAULT_SETTINGS = { shellProgram: "pwsh.exe", shellArgs: ["-NoLogo", "-NoProfile"] };
const everyApp = [];
let scenarioCount = 0;
let failures = 0;

class AppWorld extends World {
  constructor(options) {
    super(options);
    this.app = null;
    this.launches = 0;
    this.places = new Map();
    this.settings = { ...DEFAULT_SETTINGS };
  }

  get stubDir() {
    return join(this.root, "stub");
  }

  get statePath() {
    return join(this.root, "appdata", "AgenticOrchestrator", "state.json");
  }

  /** The running app, starting it first if needed; a missing state file is seeded with the test settings. */
  async open() {
    if (!this.app) {
      if (!existsSync(this.statePath)) this.writeState({ workspaces: [] });
      this.app = await App.launch({ root: this.root, stubDir: this.stubDir, launchNumber: ++this.launches });
      everyApp.push(this.app);
    }
    return this.app;
  }

  async restart() {
    await this.app.stop();
    this.app = null;
    return this.open();
  }

  writeState(state) {
    mkdirSync(join(this.root, "appdata", "AgenticOrchestrator"), { recursive: true });
    writeFileSync(this.statePath, JSON.stringify({ settings: this.settings, ...state }, null, 2));
  }

  readState() {
    return JSON.parse(readFileSync(this.statePath, "utf8"));
  }

  place(name) {
    const path = this.places.get(name);
    if (!path) throw new Error(`no repository or worktree named "${name}" in this scenario`);
    return path;
  }
}

setWorldConstructor(AppWorld);

BeforeAll(function () {
  for (const exe of [APP_EXE, STUB_EXE, PROC_EXE]) {
    if (!existsSync(exe)) throw new Error(`${exe} is missing; run "pnpm -C app test:bdd", which builds it first`);
  }
  mkdirSync(ARTEFACTS, { recursive: true });
});

Before(function ({ pickle }) {
  const slug = pickle.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 40);
  const folder = join(RUN_ROOT, `${String(++scenarioCount).padStart(2, "0")}-${slug}`);
  mkdirSync(join(folder, "stub"), { recursive: true });
  this.root = realpathSync.native(folder);
  this.slug = folder.split("\\").pop();
});

After(async function ({ result }) {
  const failed = result.status !== Status.PASSED;
  if (failed) failures += 1;
  if (failed && this.app && !this.app.exited) {
    try {
      const png = await this.app.screenshot();
      writeFileSync(join(ARTEFACTS, `${this.slug}.png`), png);
      this.attach(png, "image/png");
    } catch (error) {
      this.attach(`no screenshot: ${error.message}`, "text/plain");
    }
  }
  if (failed && this.app?.errors.length) {
    const errors = this.app.errors.join("\n");
    writeFileSync(join(ARTEFACTS, `${this.slug}.console.txt`), errors);
    this.attach(`Page errors:\n${errors}`, "text/plain");
  }
  if (this.app) await this.app.stop();
  if (!failed) await removeFolder(this.root);
});

AfterAll(async function () {
  await assertStopped(everyApp.flatMap((app) => [...app.known.values()]), "at the end of the run");
  if (failures > 0) console.log(`\nScreenshots and page errors of failed scenarios: ${ARTEFACTS}`);
  console.log(`Scenario folders of failed scenarios are kept under: ${RUN_ROOT}`);
});

async function removeFolder(folder) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    try {
      rmSync(folder, { recursive: true, force: true });
      return;
    } catch {
      await sleep(300);
    }
  }
}
