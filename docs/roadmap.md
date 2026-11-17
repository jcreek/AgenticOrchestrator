# Cross-platform roadmap: macOS first, then Linux

The app runs on Windows only. This is the plan for macOS and then Linux. It comes from an audit of the code in November 2026.

Items marked *(inferred)* come from knowledge of the platforms, not from running anything. They must be confirmed on a real Mac.

## Where we start

- **Nothing builds off Windows.** `crates/core`, `crates/claude-stub` and `app/src-tauri` all use Windows APIs unconditionally.
- **The terminal layer is mostly portable.** It uses `portable-pty`, not ConPTY directly. The Windows-only parts are process-tree killing (Job objects), font discovery (registry), the state folder (`%LOCALAPPDATA%`) and a set of path rules.
- **The `@ui` tests cannot run on macOS.** The harness drives WebView2 over the Chrome DevTools Protocol. WKWebView offers neither that nor a WebDriver route that Tauri supports.
- **There is no CI and no signing.**

## Decisions

1. **`@ui` scenarios stay Windows-only for the macOS port.** macOS is covered by `cargo test --workspace` (which includes the `@core` BDD scenarios), the front-end unit tests and a written manual smoke checklist. Linux can get `@ui` later through tauri-driver.
2. **Signing and notarisation wait until the app is handed to another Mac user.** A locally built app runs on the Mac that built it. Distribution needs a paid Apple Developer account.
3. **No CI is added as part of the port.** Checks are run by hand on a Mac, as they are on Windows. A CI matrix is optional in Phase 4.
4. **Windows behaviour does not change.** Every phase ends with all five checks passing on Windows.

Linux would be the cheaper platform to test (free CI runners, and a WebDriver route for `@ui`). macOS first means the first port relies on manual UI testing. Phase 2 is shared Unix work, so Linux gets most of its core port either way.

## Phase 1: Make the workspace compile everywhere (small)

No behaviour change. Windows-only code is gated with `#[cfg(windows)]`, and other platforms get a stub that returns an error or an empty result.

- `crates/core/Cargo.toml`, `crates/claude-stub/Cargo.toml`: `windows` becomes a Windows-only dependency.
- `crates/core/src/terminal.rs`: Job-object code and the `TerminalError::Job` variant.
- `crates/core/src/fonts.rs`: registry lookup.
- `app/src-tauri/build.rs`: the MSVC manifest flags.
- `app/src-tauri/src/lib.rs`: the WebView2 `SetIsVisible` call.
- `app/src-tauri/src/misc.rs`, `status.rs`: `creation_flags`.
- `crates/claude-stub`: Win32 console and process calls.
- Test code that uses Win32: `crates/core/tests/perf.rs`, `crates/core/tests/bdd/terminals.rs`, and the unit-test modules in `terminal.rs`, `state.rs` and `files.rs`.

**Exit:** the five checks pass on Windows; `cargo clippy --workspace --all-targets -- -D warnings` passes on a Mac.

## Phase 2: Unix implementation in core (the largest phase)

Shared by macOS and Linux. All logic goes in `crates/core` with tests.

1. **Process trees** (`terminal.rs`). A process group and `killpg` in place of Job objects. `ProcessTree::adopt` takes an already-spawned child, but on Unix the group must be set before spawn, so its shape changes; the one outside caller is `app/src-tauri/src/status.rs`. A signalled process does not return `KILLED_EXIT_CODE`, so "killed by us" needs recording separately. Known limit: a process that starts its own session escapes the kill, which a Job object prevents.
2. **Login-shell environment** *(inferred)*. A Mac app started from Finder gets a minimal `PATH`, so `claude`, Homebrew `git` and `code` are not found. Capture the login shell's environment once at startup and feed it to `child_environment` in place of the app's own environment. The existing filter still runs on the captured set.
3. **Default shell**: `$SHELL`, falling back to `/bin/zsh` on macOS and `/bin/bash` on Linux.
4. **ConPTY cursor-reply workaround** (`read_output` in `terminal.rs`): Windows only. Left on, it would answer a real program's query wrongly on Unix.
5. **`resolve_program`**: on Unix, look for an extensionless executable.
6. **State folder** (`state.rs`): `~/Library/Application Support/AgenticOrchestrator` on macOS, the XDG folder on Linux. Add an override variable at the same time; the tests need it for isolation on every platform.
7. **Fonts** (`fonts.rs`): list fonts on macOS and Linux. `nerd_font_family` already works on plain names.
8. **Path rules.** Make these Windows-only: reserved names and invalid characters (`files.rs`), the trailing dot/space check (`state.rs`), and backslash-to-slash conversion (`git.rs`, `worktrees.rs`). Lowercased comparisons (`state.rs`, `status.rs`) stay for Windows and macOS and are revisited for Linux in Phase 5.
9. **Worktree removal** (`unlink_directory_links` in `worktrees.rs`): `remove_dir` fails on a Unix symlink *(inferred)*; use `remove_file` there.
10. **Tests.** Unix fixtures for the core tests and `@core` BDD steps: `sh -c` for `cmd.exe /c`, `sleep` for `ping -n`, a symlink for `mklink /J`, and a per-platform absolute path in `features/changes.feature`. One shared cross-platform process lister for `ao-proc` and the BDD steps. A `termios` raw mode for the stub so the Ctrl+W scenario passes.

**Exit:** `cargo test --workspace` and clippy pass on macOS and on Windows.

## Phase 3: macOS app behaviour (medium)

1. **Window**: a `tauri.macos.conf.json` with an overlay title bar; hide the HTML window buttons in `TitleBar.tsx` on macOS and leave room for the traffic lights.
2. **Quit guard** (`app/src-tauri/src/lib.rs`) *(inferred)*: Cmd+Q and Dock > Quit bypass the close confirmation. Handle `ExitRequested` and route it through the existing guard in `App.tsx`.
3. **Shortcuts** (`app/src/state/shortcuts.ts`): Cmd as the modifier on macOS. One label helper in place of the hard-coded "Ctrl+…" text in about ten files. Also terminal copy/paste in `XTerm.tsx`, and Ctrl+Enter in `CommitBox.tsx` and `NewSessionDialog.tsx`.
4. **Paths in the UI** (`app/src/state/paths.ts`): join with the separator the repo path uses.
5. **Fonts** (`theme.css`, `terminal/font.ts`): add `-apple-system`, `ui-monospace`, "SF Mono" and Menlo to the stacks.
6. **WebKit checks** *(all inferred)*: clipboard paste on right-click, `document.hidden` when minimised, Monaco workers, `scrollbar-width`, notifications.
7. **Bundle identifier**: `dev.agenticorchestrator.app` ends in `.app`, which clashes on macOS. Renaming it resets saved window positions on Windows, so it needs a decision.

**Exit:** `pnpm -C app tauri dev` runs on a Mac, and a manual smoke checklist passes. The checklist is the `@ui` feature files walked by hand, and goes in `docs/testing.md`.

## Phase 4: macOS packaging and docs (small)

- `tauri.macos.conf.json`: `app` and `dmg` bundle targets.
- A shell equivalent of `Start Agentic Orchestrator.cmd`.
- README and `docs/development.md`: macOS prerequisites, POSIX command forms, upgrade steps and the macOS paths.
- The Environment notes in `CLAUDE.md`.
- Optional: signing and notarisation; a CI matrix running the cargo and unit-test checks on Windows and macOS.

**Exit:** a `.dmg` built on the Mac installs and runs on that Mac.

## Phase 5: Linux (medium, mostly testing)

- `tauri.linux.conf.json`: `deb` and `AppImage` targets.
- Case-sensitive path comparison in `state.rs`, `status.rs` and `app/src/state/paths.ts`, and the "spelled in upper case" scenario in `features/sessions.feature`.
- Folder watching: recursive inotify can hit the watch limit on large repos. Confirm the existing fallback to polling in `watch.rs` covers it.
- WebKitGTK testing: WebGL terminal renderer, window decorations, CSS support.
- `@ui` on Linux: a WebDriver backend behind the existing `App` and `ui.mjs` interface in `app/e2e/support/`, where all the CDP calls already sit. Focus and viewport emulation need replacements.
- Docs: Linux prerequisites.

## Checks per phase

- After every phase, on Windows: all five checks from `CLAUDE.md`.
- From Phase 1, on the Mac: `cargo clippy --workspace --all-targets -- -D warnings`.
- From Phase 2, on the Mac: `cargo test --workspace`.
- From Phase 3, on the Mac: `pnpm -C app build`, `pnpm -C app test`, and the manual smoke checklist.
