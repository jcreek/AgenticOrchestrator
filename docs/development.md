# Development

## Set up

1. Install [rustup](https://rustup.rs) (Rust stable), Node 24, pnpm and the Visual Studio Build Tools with the C++ workload.
2. Install front-end packages: `pnpm -C app install`.
3. Run with live reload: `pnpm -C app tauri dev`. The first build takes a few minutes.

To run the optimised app without live reload, double-click `Start Agentic Orchestrator.cmd` in the repo root.

## Checks

Run all four before finishing a change:

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm -C app build
pnpm -C app test
```

The core tests start real terminals and run real git in temporary folders, so they take about a minute. `cargo test --workspace` also runs the `@core` BDD scenarios in `features/`. One test is ignored by default because it needs Claude Code on PATH: `cargo test -p orchestrator-core -- --ignored`.

For changes a user would see in the window, also run the end-to-end `@ui` scenarios, which build and drive the real app with a stub Claude Code:

```
pnpm -C app test:bdd
```

[Testing](testing.md) explains the layers, how to write a scenario and how the stub works.

## Where things go

| Change | Where |
|---|---|
| New behaviour or a rule | `crates/core`, with a test |
| Exposing it to the UI | A command in `app/src-tauri/src`, registered in `lib.rs` |
| UI | `app/src` |

After adding or changing a command or a type it returns, regenerate the TypeScript bindings:

```
$env:UPDATE_BINDINGS = '1'; cargo test -p app committed_bindings
```

The same test fails in normal runs if `app/src/bindings.ts` is out of date.

## Front end layout

| Folder | Holds |
|---|---|
| `app/src/shell` | Title bar, tabs, status bar |
| `app/src/overview` | Overview table and empty state |
| `app/src/session` | Session toolbar, split layout, start and resume banner |
| `app/src/terminal` | xterm.js wrapper and the shell tab row |
| `app/src/ide` | Changes list, file tree, Monaco diff and editor, commit box |
| `app/src/dialogs` | New session, close session, close app |
| `app/src/notifications` | In-app toasts |
| `app/src/state` | Pure logic: status mapping, ordering, shortcuts, toasts |

Colours, sizes and fonts come from the CSS variables in `app/src/theme.css`, taken from the design in `docs/design`. Do not hard-code colours in components.

## Trying things without touching your own data

The app stores its state under `%LOCALAPPDATA%`. To run a throwaway copy, start it with `LOCALAPPDATA` pointing at an empty folder:

```
$env:LOCALAPPDATA = "$env:TEMP\ao-scratch"; pnpm -C app tauri dev
```

This redirects `state.json` only. WebView2 keeps its own profile, which holds the saved pane layouts, in the real `%LOCALAPPDATA%\dev.agenticorchestrator.app`.

Only one copy of the app runs at a time: starting a second one focuses the first.

## App icon

The icons are in `app/src-tauri/icons`, listed under `bundle.icon` in `tauri.conf.json`. `icon.svg` is the master artwork.

| File | Used for |
|---|---|
| `icon.ico`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png` | Windows: the exe, taskbar and installer. Drawn by hand for each size; do not overwrite them with generated ones. |
| `icon.icns` | macOS. Not used by the Windows build; kept because the app is meant to become cross-platform. |
| `Square*Logo.png`, `StoreLogo.png` | Microsoft Store packages. Not used by the current installer. |

`app/public/icon.svg` is the simplified small-size artwork, used as the page icon.

To regenerate `icon.icns` and the Store logos after changing the artwork, generate into a scratch folder and copy only those files across:

```
pnpm -C app tauri icon src-tauri/icons/icon.svg -o $env:TEMP\ao-icons
```

## Building the installer

`pnpm -C app tauri build` writes a per-user installer to `target\release\bundle\nsis`. It is not code-signed.

The app version comes from `app/src-tauri/Cargo.toml`. Keep `app/package.json` in step with it. The Settings dialog shows the installed version.

## Upgrading an installed copy

1. Raise the version in `app/src-tauri/Cargo.toml` and `app/package.json`.
2. Run `pnpm -C app tauri build`.
3. Close the app on the target machine. This ends its running terminals.
4. Run the new installer. On the "Already Installed" page choose **Do not uninstall**, which replaces the installed copy in place.

"Uninstall before installing" also works, but it runs the old uninstaller first. If you use it, leave "Delete the application data" unticked, or the saved pane layouts are lost.

Sessions and settings are kept: `state.json` and the WebView2 profile live under `%LOCALAPPDATA%`, outside the install folder, and settings missing from an older `state.json` take their defaults.

## Notes for agents working in this repo

- The package manager is pnpm, not npm.
- Invoke `cargo` and `pnpm` from PATH. A local pre-tool hook rejects shell commands that name a `bin`, `obj`, `dist`, `node_modules`, `coverage` or `.git` path.
- Monaco deep imports are relative to `esm/vs`, for example `monaco-editor/editor/editor.worker.js?worker`.
