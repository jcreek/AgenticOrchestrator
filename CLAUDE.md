# Agentic Orchestrator

A Windows desktop app for running and switching between many Claude Code sessions, one per repo or git worktree. Tauri 2 (Rust backend, WebView2 front end), xterm.js terminals, Monaco editor.

## Layout

- `crates/core/` — all logic (status, terminals, files, git, worktrees, folder watching, state). No Tauri dependency. Tested with cargo.
- `app/src-tauri/` — thin Tauri commands and channels that adapt `orchestrator-core`. No logic here.
- `app/src/` — React + TypeScript front end.
- `spikes/` — throwaway Phase 0 experiments. Not part of the workspace; do not build on them.

## Checks (run all before finishing a change)

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm -C app build
pnpm -C app test
pnpm -C app test:bdd
```

`cargo test --workspace` includes the `@core` BDD scenarios in `features/`. `pnpm -C app test:bdd` builds the app and runs the end-to-end `@ui` scenarios against it with a stub Claude Code (a few minutes; see `docs/testing.md`).

`app/src/bindings.ts` is generated from Rust. After changing a command or a shared type, regenerate it with `UPDATE_BINDINGS=1 cargo test -p app committed_bindings`.

Run the app: `pnpm -C app tauri dev`. Install front-end packages: `pnpm -C app install`.

## Rules

- Put logic in `crates/core` with tests; keep `app/src-tauri` to argument conversion and wiring.
- No network listener. The UI talks to the backend only through Tauri commands and channels.
- Start terminals with a clean, explicit environment. Never pass the app's own environment through unfiltered.
- Git and other tools are invoked with argument arrays, never a shell string.
- File paths from the UI are canonicalised and must sit inside the workspace folder.
- Logs carry metadata only: no terminal bytes, file content or prompts.

## Environment notes

- The package manager is pnpm, not npm.
- On macOS/Linux, run the Rust workspace checks and frontend build/unit tests; `test:bdd` remains Windows-only. Native Mac UI and packaging are later milestones.
- Use `AGENTIC_ORCHESTRATOR_STATE_DIR` (absolute directory) for isolated app state.
- Invoke `cargo` and `pnpm` from PATH. A local pre-tool hook rejects any shell command that names a `bin`, `obj`, `dist`, `node_modules`, `coverage` or `.git` path.
- Status comes from polling `claude agents --json`, matched by the session id the app assigns with `--session-id`.
- Monaco deep imports are relative to `esm/vs`, e.g. `monaco-editor/editor/editor.worker.js?worker`.
