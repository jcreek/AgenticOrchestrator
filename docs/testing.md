# Testing

Three layers, from fast and narrow to slow and whole.

| Layer | What it proves | Where | Runs in |
|---|---|---|---|
| Unit tests | One function or module behaves, including edge cases | `#[cfg(test)]` modules in `crates/core/src`, `*.test.ts` in `app/src` | `cargo test --workspace`, `pnpm -C app test` |
| `@core` scenarios | The product's rules, stated in the user's words, hold in the Rust core: git, worktrees, files, state, status, terminals | `features/*.feature`, steps in `crates/core/tests/bdd` | `cargo test --workspace` (about 35 s) |
| `@ui` scenarios | The real app window does what the user expects, end to end, with a stub Claude Code | `features/*.feature`, steps in `app/e2e` | `pnpm -C app test:bdd`, on demand |

Write a rule as a `@core` scenario when the core alone decides it (a save conflict, a removal blocker). Write a `@ui` scenario when it depends on the window: what a tab shows, a dialog, a key press, a reload.

## Run

```
cargo test --workspace                                # unit tests and @core scenarios
cargo test -p orchestrator-core --test bdd            # @core scenarios only
cargo test -p orchestrator-core --test bdd -- --name "junction"   # scenarios whose name matches
pnpm -C app test                                      # front-end unit tests
pnpm -C app test:bdd                                  # @ui scenarios
pnpm -C app test:bdd --name "Ctrl\+W"               # @ui scenarios whose name matches
```

`test:bdd` first runs `app/e2e/prepare.mjs`, which builds the stub Claude Code and a debug build of the app into `target\bdd`. The first build takes about four minutes; later ones are incremental (about a minute, most of it the front-end build). Set `AO_BDD_SKIP_BUILD=1` to skip it when nothing has changed. The `@ui` run takes 7 to 10 minutes for 45 scenarios, longer on a busy machine; most of it is waiting for the status poller.

## Writing a scenario

1. Add it to the feature for its capability in `features/`. Write what the user does and sees ("When I close the window"), not how ("click the third button"). Tag it `@core` or `@ui`.
2. Run the runner. An undefined step fails the run and cucumber prints the step it needs.
3. Add the step:
   - `@core`: a function in the matching file under `crates/core/tests/bdd`, using the `#[given]`, `#[when]` or `#[then]` attributes. Steps call the public core API on folders inside the scenario's temp dir (`world.dir`). Wrap anything that starts a process in `off_thread`, so other scenarios run while it waits. `world.new_repo` and `world.new_repo_with_remote` copy repositories built once per run.
   - `@ui`: a function in `app/e2e/steps`. Keep it to a few lines; helpers live in `app/e2e/support` (`ui.mjs` for finding elements, clicking, typing, reading terminals and the editor; `sessions.mjs` for opening sessions). Act with real input (`click`, `press`, `typeText`) and assert on what is shown, found by role, accessible name or label.
4. Make it fail: break the behaviour it describes and check the scenario fails with a message that says why. A scenario that still passes tests nothing.

Waits never use fixed sleeps: `until(what, check, timeout)` polls and, on timeout, says what it waited for and what it last saw. In cucumber-js, `Given`, `When` and `Then` are interchangeable, so define each step text once.

## The stub Claude Code

`@ui` scenarios never run the real Claude Code. `crates/claude-stub` builds `claude.exe`, and the run puts its folder first on the app's PATH. It reads and writes files in the folder named by `AO_STUB_CLAUDE_DIR`, a per-scenario temp folder:

| File | Written by | Meaning |
|---|---|---|
| `launches.log` | stub | One line per interactive launch: `new|resume <session id> <folder>` |
| `live\<session id>.json` | stub | A running session: id, folder, pid. Removed when it exits normally |
| `control\<session id>.json` | test | What that session reports, e.g. `{"status":"waiting","waitingFor":"question"}`, `{"listed":false}` (not reported at all) or `{"exit":1}` (exit with that code) |
| `control\default.json` | test | The same, for sessions without their own file |
| `agents-mode` | test | `fail` makes `claude agents --json` exit with an error; `hang` makes it hang |

Run as `claude --session-id <id>` or `claude --resume <id>`, it prints `Stub Claude Code` and `new|resume session <id>`, echoes what is typed (Ctrl+W as `<Ctrl+W>`), prints a line whenever its control file changes (as the real CLI redraws, which wakes the app's status poller), and exits when told to. Run as `claude agents --json`, it prints one entry per live session in the shape the real CLI uses for interactive sessions: `pid`, `cwd`, `kind: "interactive"`, `startedAt`, `sessionId`, `name`, `status` (`busy`, `idle` or `waiting`) and, when waiting, `waitingFor`.

Steps control it in the user's words: "When Claude in "alpha" asks for permission" writes `alpha`'s control file. A status change can take up to the poller's quiet interval (15 s) plus a query to show, so status steps wait up to 30 s.

The shape was last checked against the real CLI by hand on 2026-11-16, with Claude Code 2.1.326: interactive entries had `cwd`, `kind`, `name`, `pid`, `sessionId`, `startedAt` and `status`. No session was waiting at the time, so `waitingFor` was not seen live; its form comes from the fixture in `crates/core/src/status.rs`. To check again, run `claude agents --json` while a session waits for permission and compare.

`crates/claude-stub` also builds `ao-proc.exe`, which lists every process with its parent, start time and whether it is still running, so the run can find what the app started and tell it from a later process that reused its id.

## Isolation

Each `@ui` scenario gets a fresh folder under `%TEMP%\ao-bdd\run-<time>\` and a freshly started app. A restart costs about two seconds; resetting a running app could not clear its terminals, statuses and page state as reliably. For each start the app gets:

- `LOCALAPPDATA` pointing into the scenario folder, so `state.json` is a throwaway copy, seeded with `pwsh.exe -NoLogo -NoProfile` as the shell.
- `WEBVIEW2_USER_DATA_FOLDER` pointing into the scenario folder, so the WebView2 profile is new too.
- The bundle identifier `dev.agenticorchestrator.bdd` (from `app/e2e/tauri.bdd.json`, passed to `tauri build --config`), so it does not collide with a copy of the app you have open.
- WebView2 remote debugging on a free port, which the run drives over the Chrome DevTools Protocol.
- The environment of the run minus `NO_COLOR`, `CLAUDECODE`, `CLAUDE_PID` and `CLAUDE_CODE_*`, with the stub first on PATH.

Every repository is created per scenario inside the scenario folder, with a local git identity and signing off. After each scenario the app is stopped by its process id with everything it started, and the run fails if any process the app started is still running. The scenario folder is deleted when the scenario passed.

## When a scenario fails

The report names the step and the reason, e.g. `Timed out after 30s waiting for the "alpha" tab to show "Finished". Last seen: the tab reads "alpha · main · Idle · 3s"`. A screenshot of the window and any page errors are attached to the HTML report at `%TEMP%\ao-bdd\ui-report.html` and saved under `%TEMP%\ao-bdd\run-<time>\artefacts`; the run prints that folder. The failed scenario's folder (repositories, `state.json`, stub files) is kept beside it.

## Known limits

- The `@ui` run needs real windows, so it cannot run in a headless session. To stay out of your way each test window cannot take keyboard focus, stays off the taskbar, and parked above and left of the primary monitor with a 20-pixel corner showing. It flashes at a normal position for a moment at start-up before it is parked. Windows may still show a notification when a session needs you.
- The parked window is 1400 by 900 physical pixels, so on a display scaled above 100% the page is narrower than in the real app.
- The window is driven with keyboard and mouse events sent to the page. Window-manager behaviour (dragging, snapping, the real minimise and maximise) is not exercised; the "Many sessions" widths are set with page emulation, not by resizing the window.
- The folder picker is native and cannot be driven. Scenarios type the folder into the New session dialog instead, which the dialog supports.
- Terminal text is read from xterm.js's buffer through React's internals, because the WebGL renderer draws to a canvas.
- The run writes the test build's saved window state to `%APPDATA%\dev.agenticorchestrator.bdd\.window-state.json` before each start; that is how the window is parked and kept unfocused. The real app's saved state is under a different identifier and is not touched.
- Debug builds regenerate `app/src/bindings.ts` at start-up; the content is unchanged unless commands changed.
