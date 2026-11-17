# Cross-platform roadmap: remaining work

Only outstanding work is listed here. Validation records and the macOS manual
smoke checklist live in [Testing](testing.md).

## Decisions

- **macOS UI validation is manual.** The existing `@ui` harness is Windows-only
  because it drives WebView2 over CDP. Linux UI automation will use tauri-driver.
- **Developer ID signing and notarisation are excluded.** Users can manually
  choose Open Anyway for a trusted downloaded copy.
- **CI remains deferred.** Run checks by hand on each native platform.
- **Preserve Windows behaviour.** Cross-compilation and Mac checks do not replace
  the five native Windows checks in `CLAUDE.md`.

## macOS validation

- Complete the manual smoke checklist in [Testing](testing.md): native window
  controls, running-session and unsaved-edit quit confirmation, shortcuts,
  clipboard, minimisation, Monaco workers, scrollbars, fonts and notifications.
  Use the bundled app for computer use; the unbundled development executable
  could not be attached to previously.
- Validate a browser-downloaded DMG and Open Anyway on another Mac matching
  the build architecture.
- Validate upgrades and persistence of sessions, settings and pane layouts.
- Build and validate on Intel macOS.
- Verify the repository launcher by double-clicking it in Finder.

## Windows regression validation

Run all five checks from `CLAUDE.md` on native Windows and record the results:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm -C app build
pnpm -C app test
pnpm -C app test:bdd
```

## Phase 5: Linux

- Run the Rust workspace tests, core BDD scenarios, strict clippy, frontend build
  and frontend unit tests on native Linux.
- Add `tauri.linux.conf.json` with `deb` and `AppImage` targets; verify installation
  and startup on native Linux.
- Make path comparisons case-sensitive in `state.rs`, `status.rs` and
  `app/src/state/paths.ts`; adapt the "spelled in upper case" scenario in
  `features/sessions.feature`.
- Verify folder watching and the fallback to polling in `watch.rs` when recursive
  inotify reaches the watch limit on large repositories.
- Validate WebKitGTK behaviour: WebGL terminal rendering, window decorations,
  CSS support and fonts.
- Add a tauri-driver WebDriver backend behind the existing `App` and `ui.mjs`
  interface in `app/e2e/support/`. Replace CDP-specific focus and viewport
  emulation, then run the `@ui` scenarios on Linux.
- Document Linux prerequisites, installation and upgrade steps.

For changes during the remaining work, rerun the native platform checks above;
on macOS also run the affected manual smoke checks.
