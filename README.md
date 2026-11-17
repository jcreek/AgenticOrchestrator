# Agentic Orchestrator

A Windows and macOS desktop app for running many Claude Code sessions at once. Each session gets a tab, one per repo folder or git worktree, and the tab shows whether Claude is working, waiting for input, waiting for permission, finished or idle. Each tab has the Claude terminal, extra shells, and a small IDE pane for reading what changed, editing a file and committing.

Windows is the established desktop platform. macOS app behaviour and packaging are implemented, with native validation tracked separately. Linux core support is implemented; desktop validation remains in progress. See the [roadmap](docs/roadmap.md).

## Start it

There are two ways. Most people want the first.

### 1. From this repo (recommended)

On Windows, double-click `Start Agentic Orchestrator.cmd` in the repo root. On macOS, double-click `Start Agentic Orchestrator.command`, or run `./"Start Agentic Orchestrator.command"` from Terminal.

The first run builds the app, which takes about five minutes. After that it starts straight away, and it rebuilds by itself when you pull new commits. You need the build tools listed under Requirements.

### 2. From an installer

Use this to put the app on a machine without the build tools.

Download installers from [GitHub Releases](https://github.com/jcreek/AgenticOrchestrator/releases). Releases are published automatically when a version bump reaches `main`: Windows x64, macOS Apple Silicon (`aarch64`) and macOS Intel (`x64`). If no release is available yet, use the local build instructions below.

### Windows

1. In the repo root, run `pnpm -C app tauri build` (a few minutes).
2. Take the file it writes: `target\release\bundle\nsis\Agentic Orchestrator_<version>_x64-setup.exe`. The `target` folder is ignored by git, so the file never appears in a clone.
3. On the other machine, run that file, then start **Agentic Orchestrator** from the Start menu.

It installs for the current user and needs no admin rights. It is not code-signed, so SmartScreen or Smart App Control may warn about or block it. An installed copy does not update itself: build a new installer and run it over the installed copy (see [Development](docs/development.md#upgrading-an-installed-copy)).

### macOS

1. On a Mac with the build tools, run `pnpm -C app package:macos`.
2. Share the `.dmg` in `target/<host-triple>/release/bundle/dmg/`. The `.app` is also available in `target/<host-triple>/release/bundle/macos/`. Builds match the build Mac: Apple Silicon (`aarch64-apple-darwin`) or Intel (`x86_64-apple-darwin`). Use a build matching the recipient’s Mac; the DMG filename ends in `aarch64` or `x64` respectively.
3. Open the DMG and read **Read Me First.txt**. Drag **Agentic Orchestrator** into **Applications**, then eject the image.
4. Open **Agentic Orchestrator** from Applications.
5. If macOS blocks it, open **System Settings → Privacy & Security**, scroll to the message about Agentic Orchestrator and choose **Open Anyway**. Confirm the prompt (and authenticate if requested), then click **Open**. If Open Anyway is missing, try opening the installed app again first.

The app uses an ad-hoc bundle signature, with no Developer ID signature or Apple notarisation. Only choose Open Anyway for a copy you trust; see [Apple’s instructions](https://support.apple.com/en-gb/102445). No paid Apple Developer account is required for this installation workflow. Managed Macs may restrict this override. Do not disable Gatekeeper or remove quarantine attributes.

To upgrade, quit the app (which stops its terminals), then replace the app in Applications with the new copy. Sessions and settings are stored outside the application bundle. There is no automatic updater; download new versions from GitHub Releases.

## Requirements

| To | You need |
|---|---|
| Use the app | Windows 11 or macOS, [Claude Code](https://code.claude.com) (`claude`) and `git` on PATH. `code` (VS Code) is optional. |
| Build it (the repository launchers do this) | Also Rust stable via [rustup](https://rustup.rs), Node 24 and pnpm. Windows: Visual Studio Build Tools with the C++ workload. macOS: Xcode Command Line Tools (`xcode-select --install`). |

The app shows a banner at startup if `claude`, `git` or `code` is missing.

## Use

On macOS, use Cmd instead of Ctrl for app shortcuts, including Cmd+N/J/0/1–9/backslash/S and Cmd+Enter for form submission. Tab cycling remains Ctrl+Tab/Ctrl+Shift+Tab. Cmd+W closes a session even in a terminal; terminal Ctrl+W deletes a word.

1. Press `Ctrl+N` and pick a repository.
2. Choose where to run: in the repo folder on its current branch, or in a new worktree on its own branch. Worktrees are created beside the repo in `<repo>.worktrees/<branch>` (backslashes on Windows).
3. Claude Code starts in the new tab. A folder you have not used with Claude Code before stops at its trust prompt; the tab says "Check terminal".

The tab colour and icon show what the session is doing. A repo with one session is a single tab named after the repo; a repo with several gets a group label with one tab per branch. Drag a tab onto another to re-order them; dropping onto a tab of a different repo moves the whole group. When there are more tabs than fit, scroll the strip with the mouse wheel or open the overflow button at its right-hand end, which lists every session and shows how many hidden ones need you.

When a session needs you and the window is in the background, the app requests a notification and OS attention. Native macOS notification behaviour remains part of the pending smoke checklist.

| Key | Does |
|---|---|
| `Ctrl+J` | Jump to the next session that needs you: permission, then input, then finished |
| `Ctrl+1` to `Ctrl+9` | Select a tab |
| `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next or previous tab |
| `Ctrl+0` | Overview of every session |
| `Ctrl+N` | New session |
| `Ctrl+W` | Close session (outside a terminal; inside one it goes to the shell) |
| `Ctrl+\` | Cycle the layout: terminal, both, IDE |
| `Shift+Enter` | New line in Claude's prompt |

**The IDE pane** lists changed files with a diff for each, against the last commit or against the branch point. Select "Edit file" to change a file and `Ctrl+S` to save. Stage files with the checkboxes, then commit and push from the box below the list.

**Closing a session** in a worktree the app created offers three choices: keep the worktree, commit everything and remove it, or discard and remove it. The app lists what would be lost first. It never deletes a branch.

**After a restart** each tab offers "Resume conversation" or "Start fresh", and the overview has "Resume all".

## Settings and data

State is stored in `%LOCALAPPDATA%\AgenticOrchestrator\state.json`: the workspace list, Claude session ids and settings. State uses `~/Library/Application Support/AgenticOrchestrator/state.json` on macOS and `$XDG_DATA_HOME/AgenticOrchestrator/state.json` (falling back to `~/.local/share`) on Linux. Set `AGENTIC_ORCHESTRATOR_STATE_DIR` to an absolute directory to isolate state on any platform. Use Settings for the available options; edit other settings in the file while the app is closed.

| Setting | Meaning | Default |
|---|---|---|
| `shellProgram`, `shellArgs` | The shell for extra terminals | Windows: `pwsh.exe`; Unix: executable `$SHELL`, then `/bin/zsh` (macOS) or `/bin/bash` (Linux) |
| `extraClaudeArgs` | Arguments added to every `claude` launch | none |
| `worktreeRoot` | Folder for new worktrees | `<repo>.worktrees` beside the repo |
| `postCreateCommand` | Command run in a new worktree, as an argument list, e.g. `["pnpm", "install"]` | none |
| `preRemoveCommand` | Command run before a worktree is removed | none |

To copy untracked files such as `.env` into new worktrees, list them one per line in a `.worktreeinclude` file at the repo root.

## Limits

- Sessions end when the app closes. Conversations can be resumed; work in flight cannot. The app asks before closing while sessions are running.
- Only sessions started in the app are tracked.
- Agents in different worktrees of one repo can still collide on fixed ports, a shared database or build caches.
- A worktree with initialised submodules, or one that contains another worktree, cannot be removed from the app.
- With an npm-installed Claude Code, a starting prompt containing `" & | < > ^ % !` is refused. Start without a prompt and type it in.

## Troubleshooting

| Problem | Fix |
|---|---|
| The launcher says a tool was not found on PATH | Install it (see Requirements), then open a new Explorer window so it picks up the new PATH, and run the launcher again. |
| The launcher's build fails with "Access is denied" on the `.exe` | The app is still running. Close it and run the launcher again. |
| Double-clicking does nothing | The app is already open; only one copy runs at a time. Look for it on the taskbar or Dock. |
| A tab stays on "Check terminal" | Claude Code is waiting at a prompt the app cannot see, usually the folder trust prompt. Answer it in the terminal. |
| Tabs show "Status unknown" | `claude agents --json` failed or timed out. The terminals still work; status returns when the command does. |
| The app says the state file cannot be read | `state.json` is damaged. The app will not overwrite it. Fix or delete the file while the app is closed. |

## More

- [Architecture](docs/architecture.md): how the parts fit together and the security model.
- [Development](docs/development.md): set-up, checks, where code goes, building the installer.
- [Testing](docs/testing.md): unit tests and the BDD scenarios in `features/`, and how to run them.
- [Performance](docs/performance.md): how often the app polls, measured figures and known slow spots.
- `docs/design`: the UI design the front end is built to.
