# Cross-platform roadmap: remaining work

**Phase 4 is complete (2026-11-17).** macOS packaging, the repository launcher
and installation/upgrade documentation are delivered. The DMG includes a visible
Read Me First guide with Open Anyway instructions; its contents and the local
installed app have been verified on Apple Silicon. Developer ID signing,
notarisation and macOS UI automation are deliberately excluded.

The remaining work below tracks broader platform validation and phase 5; it does
not reopen phase 4. Validation records and the macOS manual smoke checklist live
in [Testing](testing.md).

## Decisions

- **macOS UI validation is manual.** The existing `@ui` harness is Windows-only
  because it drives WebView2 over CDP. No macOS automation is planned. Linux
  UI automation belongs to phase 5 and will use tauri-driver.
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

All five checks from `CLAUDE.md` passed on native Windows on 2026-11-19 (see
[Testing](testing.md)). Rerun them on native Windows after cross-platform changes:

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
