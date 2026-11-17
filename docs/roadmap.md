# Cross-platform roadmap: macOS first, then Linux

Windows is the established desktop platform. Unix core support, macOS app behaviour and macOS packaging configuration are implemented; native validation remains tracked below before claiming full macOS support. It comes from an audit of the code in November 2026.

Items marked *(inferred)* come from knowledge of the platforms, not from running anything. They must be confirmed on a real Mac.

## Current status — 2026-11-17

- **Phases 1 and 2 are implemented.** Windows-only APIs are gated and Unix core behaviour is present. Native macOS workspace tests (118 core unit tests, 8 app unit tests and 47 core scenarios / 215 steps), strict clippy, frontend build and 141 frontend unit tests pass.
- **Phase 3 is implemented, with the full native smoke checklist pending.** The bundled app can now be inspected through computer use. Its overview, Cmd shortcut labels, disposable repository session startup and native Quit have been observed; this does not establish all window, clipboard, WebKit or quit-confirmation behaviour.
- **Phase 4 is complete for local Apple Silicon installation.** The app and DMG build; a copy installed from the mounted DMG runs after the image is ejected. The repository launcher and macOS installation/upgrade documentation are in place.
- **Native Windows regression verification remains pending.** Cross-compilation and Mac checks do not replace the five Windows checks.
- **Phase 5 has not started.** Linux core code cross-compiles, but native Linux execution, desktop packaging and UI automation remain outstanding.
- **No CI, Developer ID signing or notarisation.** Users can manually choose Open Anyway for a trusted downloaded copy.

### Remaining work

1. Complete the macOS manual smoke checklist in [Testing](testing.md), including running-session/unsaved-edit quit confirmation, clipboard, minimisation, Monaco workers and notifications. The bundled app is accessible to computer use; the earlier attachment failure affected the unbundled development executable.
2. Validate a browser-downloaded DMG and Open Anyway on another matching-architecture Mac, plus upgrade/persistence behaviour. Intel builds have not been validated.
3. Run all five checks on native Windows and record the results.
4. Implement and validate Phase 5 on native Linux.

## Decisions

1. **`@ui` scenarios stay Windows-only for the macOS port.** macOS is covered by `cargo test --workspace` (which includes the `@core` BDD scenarios), the front-end unit tests and a written manual smoke checklist. Linux can get `@ui` later through tauri-driver.
2. **Developer ID signing and notarisation are deliberately excluded.** Users can install a shared DMG and manually choose Open Anyway if Gatekeeper blocks first launch. This workflow does not require a paid Apple Developer account.
3. **No CI is added as part of the port.** Checks are run by hand on a Mac, as they are on Windows. A CI matrix remains deferred.
4. **Windows behaviour does not change.** Every phase ends with all five checks passing on Windows.

Linux would be the cheaper platform to test (free CI runners, and a WebDriver route for `@ui`). macOS first means the first port relies on manual UI testing. Phase 2 is shared Unix work, so Linux gets most of its core port either way.

## Phase 1: Make the workspace compile everywhere (implemented)

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

## Phase 2: Unix implementation in core (implemented; Windows verification pending)

Shared by macOS and Linux. All logic goes in `crates/core` with tests. The implementation includes Unix PTY/process groups, a cached filtered login-shell environment, platform shell/program resolution, platform state paths with `AGENTIC_ORCHESTRATOR_STATE_DIR`, CoreText/fontconfig font discovery, Unix path handling and portable test fixtures. The macOS Rust workspace tests, core BDD scenarios and strict clippy pass, as do the frontend build and unit tests. Core and stub code (including tests) cross-compiles for Windows and Linux targets. Linux execution and all five Windows checks still need native verification.

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

## Phase 3: macOS app behaviour (implemented; native smoke and Windows verification pending)

The implementation adds the Mac overlay title bar and traffic-light space, a native
quit-to-React confirmation bridge, platform shortcut matching and labels, POSIX
worktree previews and Apple font fallbacks. Cmd+Tab remains reserved by macOS;
Ctrl+Tab and Ctrl+Shift+Tab cycle session tabs. Cmd+W closes the selected session,
while terminal Ctrl+W remains word deletion. The native menu does not reserve
Cmd+W for closing the window. macOS alone uses
`dev.agenticorchestrator.desktop`, preserving the Windows identifier and window
positions. The Rust workspace/core BDD checks, strict clippy and frontend build
and unit tests pass on macOS. The dev executable launches. Phase 4 also verified
the installed bundle’s overview, disposable session startup and native Quit
through computer use; the full native smoke checklist remains pending.
See the manual checklist and dated results in `docs/testing.md`. Phase 4 local
packaging validation is complete; Windows still needs all five native checks.

1. **Window**: a `tauri.macos.conf.json` with an overlay title bar; hide the HTML window buttons in `TitleBar.tsx` on macOS and leave room for the traffic lights.
2. **Quit guard** (`app/src-tauri/src/lib.rs`) *(inferred)*: Cmd+Q and Dock > Quit bypass the close confirmation. Handle `ExitRequested` and route it through the existing guard in `App.tsx`.
3. **Shortcuts** (`app/src/state/shortcuts.ts`): Cmd as the modifier on macOS. One label helper in place of the hard-coded "Ctrl+…" text in about ten files. Also terminal copy/paste in `XTerm.tsx`, and Ctrl+Enter in `CommitBox.tsx` and `NewSessionDialog.tsx`.
4. **Paths in the UI** (`app/src/state/paths.ts`): join with the separator the repo path uses.
5. **Fonts** (`theme.css`, `terminal/font.ts`): add `-apple-system`, `ui-monospace`, "SF Mono" and Menlo to the stacks.
6. **WebKit checks** *(all inferred)*: clipboard paste on right-click, `document.hidden` when minimised, Monaco workers, `scrollbar-width`, notifications.
7. **Bundle identifier**: macOS overrides it with `dev.agenticorchestrator.desktop`; Windows retains `dev.agenticorchestrator.app` and its saved window positions.

**Exit:** `pnpm -C app tauri dev` runs on a Mac, and a manual smoke checklist passes. The checklist is the `@ui` feature files walked by hand, and goes in `docs/testing.md`.

## Phase 4: macOS packaging and docs (implemented; local installation verified)

- `tauri.macos.conf.json`: `app` and `dmg` bundle targets.
- Executable `Start Agentic Orchestrator.command` mirrors the Windows launcher’s commit-stamped build and launch workflow.
- README and `docs/development.md`: macOS prerequisites, POSIX command forms, upgrade steps and the macOS paths.
- The Environment notes in `CLAUDE.md`.
- Developer ID signing and notarisation deliberately excluded; CI remains deferred.
- Validation results and remaining installation/smoke checks are recorded in `docs/testing.md`.

**Exit met on Apple Silicon (2026-11-17):** the built DMG was mounted, its app copied out and launched after ejecting the image; a disposable repository session reached Claude Code’s trust prompt and native Quit left no test processes. Downloaded-app Gatekeeper behaviour, the phase 3 checklist and native Windows checks remain pending.

## Phase 5: Linux (not started; medium, mostly testing)

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
