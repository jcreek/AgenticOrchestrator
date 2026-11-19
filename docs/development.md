# Development

## Set up

1. Install [rustup](https://rustup.rs) (Rust stable), Node 24, pnpm and the Visual Studio Build Tools with the C++ workload.
   On macOS, install Xcode Command Line Tools (`xcode-select --install`) instead of the Visual Studio tools. Rust stable, Node 24 and pnpm are still required. The macOS app can be built as an application bundle and DMG; outstanding native smoke checks are recorded in [Testing](testing.md). Linux also needs its distribution’s Tauri 2 WebKitGTK development prerequisites.
2. Install front-end packages: `pnpm -C app install`.
3. Run with live reload: `pnpm -C app tauri dev`. The first build takes a few minutes.

To run the optimised app without live reload, double-click `Start Agentic Orchestrator.cmd` on Windows or `Start Agentic Orchestrator.command` on macOS in the repo root. The launchers build on first use and when the Git commit changes; uncommitted source changes require a manual build.

## Checks

Run all four before finishing a change:

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm -C app build
pnpm -C app test
```

The core tests start real terminals and run real git in temporary folders, so they take about a minute. `cargo test --workspace` also runs the `@core` BDD scenarios in `features/`. One test is ignored by default because it needs Claude Code on PATH: `cargo test -p orchestrator-core -- --ignored`.

On Windows, for changes a user would see in the window, also run the end-to-end `@ui` scenarios, which build and drive the real app with a stub Claude Code:

```
pnpm -C app test:bdd
```

On macOS, walk the manual smoke checklist instead; the WebView2-based `@ui` harness is Windows-only.

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

On macOS/Linux, use `UPDATE_BINDINGS=1 cargo test -p app committed_bindings`.

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

Use `AGENTIC_ORCHESTRATOR_STATE_DIR` to select an absolute directory containing `state.json`, without changing the user’s normal state. Relative or empty overrides are rejected. In PowerShell:

```
$env:AGENTIC_ORCHESTRATOR_STATE_DIR = "$env:TEMP\ao-scratch"; pnpm -C app tauri dev
```

On macOS/Linux:

```sh
AGENTIC_ORCHESTRATOR_STATE_DIR="$(mktemp -d)" pnpm -C app tauri dev
```

Default state directories are `%LOCALAPPDATA%\AgenticOrchestrator` on Windows, `~/Library/Application Support/AgenticOrchestrator` on macOS, and `$XDG_DATA_HOME/AgenticOrchestrator` (or `~/.local/share/AgenticOrchestrator`) on Linux. Existing state formats and Windows locations are unchanged.

Unix captures the configured login shell’s environment once at startup, with a three-second shell deadline, bounded output collection and fallback to the inherited environment. Every tool receives the existing filtered environment. The default shell is an absolute executable `$SHELL`, falling back to `/bin/zsh` on macOS or `/bin/bash` on Linux.

This redirects `state.json` only. WebView2 keeps its own profile, which holds the saved pane layouts, in the real `%LOCALAPPDATA%\dev.agenticorchestrator.app`.

Only one copy of the app runs at a time: starting a second one focuses the first.

## App icon

The icons are in `app/src-tauri/icons`, listed under `bundle.icon` in `tauri.conf.json`. `icon.svg` is the master artwork.

| File | Used for |
|---|---|
| `icon.ico`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png` | Windows: the exe, taskbar and installer. Drawn by hand for each size; do not overwrite them with generated ones. |
| `icon.icns` | macOS application bundle and DMG. |
| `Square*Logo.png`, `StoreLogo.png` | Microsoft Store packages. Not used by the current installer. |

`app/public/icon.svg` is the simplified small-size artwork, used as the page icon.

To regenerate `icon.icns` and the Store logos after changing the artwork, generate into a scratch folder and copy only those files across:

```
pnpm -C app tauri icon src-tauri/icons/icon.svg -o $env:TEMP\ao-icons
```

## Building the installer

`pnpm -C app tauri build` writes a per-user Windows installer to `target\release\bundle\nsis`.

On macOS, run `pnpm -C app package:macos` for the distributable DMG. It defaults to the host architecture; use `pnpm -C app package:macos --target x86_64-apple-darwin` to build for Intel on Apple Silicon (first install the target with `rustup target add x86_64-apple-darwin`). Use `--target aarch64-apple-darwin` to select Apple Silicon explicitly. It builds the selected architecture’s app using Tauri, then packages the app, an Applications shortcut and a visible **Read Me First.txt** with `hdiutil`. Outputs are `target/<target-triple>/release/bundle/macos/Agentic Orchestrator.app` and a `.dmg` under `target/<target-triple>/release/bundle/dmg/`. Apple Silicon uses `aarch64-apple-darwin` / the filename suffix `aarch64`; Intel uses `x86_64-apple-darwin` / `x64`. `CARGO_TARGET_DIR`, if set, replaces the `target` root. Universal binaries are outside this workflow. Plain `pnpm -C app tauri build` produces only the macOS app bundle; Windows NSIS packaging is unchanged.

The macOS bundle uses an ad-hoc signature (`bundle.macOS.signingIdentity: "-"`) to seal the executable and resources. Packaging checks the executable’s architecture with `lipo` and verifies the staged bundle with `codesign --verify --deep --strict` before creating the DMG, and fails if verification fails. Neither platform uses a distribution signing certificate. macOS Developer ID signing and notarisation are deliberately excluded. Installation and first launch:

1. Open the DMG and read **Read Me First.txt**.
2. Drag the app onto Applications, then eject the image.
3. Open the installed app from Applications.
4. If blocked, go to **System Settings → Privacy & Security**, scroll to the app’s message and click **Open Anyway**. Confirm (and authenticate if requested), then click **Open**. If the button is missing, attempt to open the installed app again first.

Only use this override for a copy you trust. See [Apple’s instructions](https://support.apple.com/en-gb/102445); managed Macs may restrict the override. Do not disable Gatekeeper or remove quarantine attributes. The standalone installation guide source is `app/packaging/macos/Read Me First.txt`; keep it consistent with README.

For a release executable without installers, run `pnpm -C app tauri build --no-bundle`.

The app version comes from `app/src-tauri/Cargo.toml`. Keep `app/package.json` in step with it. The Settings dialog shows the installed version.

## Automated releases

The **Desktop release** workflow builds both Apple Silicon and Intel DMGs on the repository’s self-hosted Mac mini (`mac-mini-agentic-orchestrator`), selected by the labels `self-hosted`, `macOS`, `ARM64` and `mac-mini`. The Intel app is cross-compiled with the `x86_64-apple-darwin` Rust target. Windows x64 uses a GitHub-hosted runner. The Mac mini must be online with Xcode Command Line Tools and `rustup` available on the runner service’s PATH; the workflow installs Node 24, pnpm, Rust stable and each required Rust target. Rust caching sets `cache-bin: "false"` to preserve the runner’s installed Rust executables; the cache action’s default cleanup can remove them between jobs. With one runner, the two Mac jobs run sequentially. Both run Rust tests natively on Apple Silicon; Intel binaries are architecture-checked but not executed, so Intel runtime testing still requires an Intel Mac or Rosetta. Only trusted release commits on `main` run on the Mac mini. No signing credentials or personal access token is needed; publication uses the built-in `GITHUB_TOKEN` on a GitHub-hosted runner.

1. Increase the stable `major.minor.patch` version in `app/src-tauri/Cargo.toml` and `app/package.json` together.
2. Run `cargo check` to refresh `Cargo.lock` and commit all three files. Keep `app/pnpm-lock.yaml` current if dependencies also change.
3. Merge to `main`. The workflow compares the version against the commit before the entire push, runs frontend and Rust tests on each platform, and builds all three installers.
4. Once every build succeeds, the workflow publishes `v<version>` at the built commit with generated release notes and all installers at [GitHub Releases](https://github.com/jcreek/AgenticOrchestrator/releases).

Ordinary pushes do not build installers. Mismatched versions, a stale app lockfile entry, invalid versions and version decreases fail the version check. Prerelease versions are not supported by this workflow. Existing tags at another commit are rejected; published releases are never overwritten.

For an initial release without a bump, open **Actions → Desktop release → Run workflow** on `main`. This also permits retrying the current unpublished version. Prefer **Re-run failed jobs** on the original run after a failure, so the release still targets the original commit. An interrupted publication leaves a draft; retrying replaces its installer assets before publishing it. A draft without a materialized tag must target the exact commit being built. Build artifacts remain downloadable from the run for 14 days, including when a different platform fails.

The workflow serializes releases; GitHub concurrency retains at most one pending run, so avoid pushing several version bumps while a release is running. Windows distribution signing, macOS Developer ID signing and notarisation remain excluded; macOS uses an ad-hoc bundle signature. The app does not update itself; it checks GitHub for a newer release at startup and links to it from Settings.

Test the release logic locally with `python3 -m unittest discover -s .github/scripts -p 'test_*.py'` (Python 3.11+).

## Upgrading an installed copy

### Windows

1. Raise the version in `app/src-tauri/Cargo.toml` and `app/package.json`.
2. Run `pnpm -C app tauri build`.
3. Close the app on the target machine. This ends its running terminals.
4. Run the new installer. On the "Already Installed" page choose **Do not uninstall**, which replaces the installed copy in place.

"Uninstall before installing" also works, but it runs the old uninstaller first. If you use it, leave "Delete the application data" unticked, or the saved pane layouts are lost.

Sessions and settings are kept: `state.json` and the WebView2 profile live under `%LOCALAPPDATA%`, outside the install folder, and settings missing from an older `state.json` take their defaults.

### macOS

1. For a versioned release, raise the version in both manifests, then run `pnpm -C app package:macos` on the desired architecture.
2. Quit the installed app, ending its running terminals.
3. Mount the new DMG and replace **Agentic Orchestrator.app** in Applications. Eject the DMG and launch the installed copy; use Open Anyway if prompted.

Session state remains in `~/Library/Application Support/AgenticOrchestrator/state.json`. The macOS identifier is `dev.agenticorchestrator.desktop`; Tauri window state is stored under its app configuration directory (`~/Library/Application Support/dev.agenticorchestrator.desktop`). WKWebView also manages website data outside the application bundle. Replacing the app preserves these stores. The session-state override does not isolate window geometry or WebKit storage.

For icon generation on macOS/Linux, use a temporary output directory:

```sh
pnpm -C app tauri icon src-tauri/icons/icon.svg -o "$(mktemp -d)"
```

## Notes for agents working in this repo

- The package manager is pnpm, not npm.
- Invoke `cargo` and `pnpm` from PATH. A local pre-tool hook rejects shell commands that name a `bin`, `obj`, `dist`, `node_modules`, `coverage` or `.git` path.
- Monaco deep imports are relative to `esm/vs`, for example `monaco-editor/editor/editor.worker.js?worker`.
