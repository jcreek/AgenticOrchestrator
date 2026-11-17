use std::collections::HashMap;
use std::io::Read;
use std::process::Stdio;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use orchestrator_core::status::{self, AgentEntry, SessionStatus};
use orchestrator_core::{git, terminal};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, UserAttentionType};
use tauri_plugin_notification::NotificationExt;
use tauri_specta::Event;

use crate::state::{Ctx, CtxExt, lock};
use crate::terminals::TerminalInfo;

const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const QUIET_POLL_INTERVAL: Duration = Duration::from_secs(15);
const CLI_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    pub status: SessionStatus,
    /// Epoch milliseconds when the workspace entered `status`.
    // Epoch milliseconds fit a JS number exactly; the u32 override only makes specta export `number`.
    #[specta(type = u32)]
    pub since: u64,
    /// Set once the Claude process has exited.
    pub exit_code: Option<u32>,
}

/// Statuses of workspaces that have a Claude terminal. `unavailable` means the last
/// `claude agents` call failed, so running sessions keep their previous status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct StatusChanged {
    pub workspaces: HashMap<String, WorkspaceStatus>,
    pub unavailable: bool,
}

#[derive(Default)]
pub struct Board {
    snapshot: StatusChanged,
    viewed: Option<String>,
}

impl Board {
    pub fn started(&mut self, workspace_id: &str) {
        self.snapshot.workspaces.insert(
            workspace_id.to_owned(),
            WorkspaceStatus {
                status: SessionStatus::Starting,
                since: now_ms(),
                exit_code: None,
            },
        );
    }

    pub fn forget(&mut self, workspace_id: &str) {
        self.snapshot.workspaces.remove(workspace_id);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

/// Polls every `POLL_INTERVAL` while [`status::needs_fast_poll`]; otherwise waits for a wake (Claude output,
/// an exit, a tab switch or a newly failed query) or `QUIET_POLL_INTERVAL`, because each query costs about 0.5 CPU-seconds.
pub fn start_poller(app: AppHandle, woken: Receiver<()>) {
    thread::spawn(move || {
        let mut last_wake = Instant::now();
        loop {
            let (statuses, newly_unavailable) = tick(&app);
            thread::sleep(POLL_INTERVAL);
            let mut wake = newly_unavailable || woken.try_recv().is_ok();
            if !wake && !status::needs_fast_poll(statuses, last_wake.elapsed()) {
                wake = woken.recv_timeout(QUIET_POLL_INTERVAL - POLL_INTERVAL).is_ok();
            }
            if wake {
                last_wake = Instant::now();
            }
        }
    });
}

/// The statuses after this poll, and whether the query has just started failing.
fn tick(app: &AppHandle) -> (Vec<SessionStatus>, bool) {
    let ctx = app.state_ctx();
    let agents = if ctx.registry.claude_terminals().values().any(|info| info.running) {
        query_agents()
    } else {
        Some(Vec::new())
    };
    // Taken after the query, which can take seconds, so terminals replaced meanwhile are not used.
    let claude = ctx.registry.claude_terminals();
    let workspaces = ctx.state().map(|state| state.workspaces).unwrap_or_default();
    let claimed: Vec<&str> = workspaces.iter().filter_map(|workspace| workspace.claude_session_id.as_deref()).collect();

    let mut board = lock(&ctx.board);
    let viewed = board.viewed.clone();
    let previous = board.snapshot.clone();
    let mut alerts = Vec::new();
    board.snapshot.unavailable = agents.is_none();
    board.snapshot.workspaces.retain(|id, _| claude.contains_key(id));
    for (workspace_id, info) in &claude {
        let before = previous.workspaces.get(workspace_id);
        let prior = before.map_or(SessionStatus::Starting, |entry| entry.status);
        let tab = workspaces.iter().find(|workspace| &workspace.id == workspace_id).map(|workspace| Tab {
            session_id: workspace.claude_session_id.as_deref(),
            folder: workspace.path.to_string_lossy().into_owned(),
            claimed_session_ids: &claimed,
        });
        let next = next_status(prior, info, tab.as_ref(), agents.as_deref(), viewed.as_deref() == Some(workspace_id.as_str()));
        if status::newly_needs_user(prior, next) {
            alerts.push((workspace_id.clone(), next));
        }
        let since = match before {
            Some(entry) if entry.status == next => entry.since,
            _ => now_ms(),
        };
        board.snapshot.workspaces.insert(
            workspace_id.clone(),
            WorkspaceStatus {
                status: next,
                since,
                exit_code: info.exit_code,
            },
        );
    }
    let changed = board.snapshot != previous;
    let snapshot = board.snapshot.clone();
    drop(board);

    if changed {
        let _ = snapshot.emit(app);
    }
    if !alerts.is_empty() {
        alert(app, &ctx, &alerts);
    }
    let statuses = snapshot.workspaces.values().map(|workspace| workspace.status).collect();
    (statuses, snapshot.unavailable && !previous.unavailable)
}

struct Tab<'a> {
    session_id: Option<&'a str>,
    folder: String,
    claimed_session_ids: &'a [&'a str],
}

fn next_status(
    prior: SessionStatus,
    info: &TerminalInfo,
    tab: Option<&Tab<'_>>,
    agents: Option<&[AgentEntry]>,
    viewed: bool,
) -> SessionStatus {
    match agents {
        Some(agents) => {
            let entry = tab.and_then(|tab| status::find_entry(agents, tab.session_id, &tab.folder, tab.claimed_session_ids));
            status::terminal_status(prior, entry, info.running, viewed)
        }
        None if !info.running => SessionStatus::Exited,
        None => prior,
    }
}

/// Runs `claude agents --json` with a timeout; `None` when it cannot be run or parsed.
fn query_agents() -> Option<Vec<AgentEntry>> {
    query_agents_with(&terminal::child_environment(std::env::vars_os()))
}

fn query_agents_with(env: &terminal::Environment) -> Option<Vec<AgentEntry>> {
    let mut child = git::hidden_command(terminal::resolve_program("claude", env))
        .args(["agents", "--json"])
        .env_clear()
        .envs(env)
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // An npm install runs through cmd.exe, so only the job reaches the node process behind it.
    let Ok(tree) = terminal::ProcessTree::adopt(&child) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut json = String::new();
        stdout.read_to_string(&mut json).map(|_| json)
    });
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(exit)) if exit.success() => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            _ => {
                drop(tree);
                let _ = child.wait();
                return None;
            }
        }
    }
    drop(tree);
    let json = reader.join().ok()?.ok()?;
    status::parse_agents(&json).ok()
}

fn alert(app: &AppHandle, ctx: &Ctx, alerts: &[(String, SessionStatus)]) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_focused().unwrap_or(true) {
        return;
    }
    let _ = window.request_user_attention(Some(UserAttentionType::Critical));
    for (workspace_id, status) in alerts {
        let Ok(workspace) = ctx.workspace(workspace_id) else {
            continue;
        };
        let group = git::repo_group(&workspace.path)
            .ok()
            .flatten()
            .map_or_else(|| workspace.name.clone(), |group| group.name);
        let title = match git::current_branch(&workspace.path).ok().flatten() {
            Some(branch) => format!("{group} · {branch}"),
            None => group,
        };
        let body = if *status == SessionStatus::NeedsPermission {
            "Claude needs your permission"
        } else {
            "Claude needs your input"
        };
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

/// Sends the current statuses now; the poller only sends changes, and a start is recorded outside it.
pub fn publish(app: &AppHandle) {
    let snapshot = lock(&app.state_ctx().board).snapshot.clone();
    let _ = snapshot.emit(app);
}

#[tauri::command]
#[specta::specta]
pub fn get_statuses(ctx: State<'_, Ctx>) -> StatusChanged {
    lock(&ctx.board).snapshot.clone()
}

/// Tells the poller which workspace the user is looking at; finished work there is not "unseen".
#[tauri::command]
#[specta::specta]
pub fn set_viewed_workspace(app: AppHandle, ctx: State<'_, Ctx>, workspace_id: Option<String>) {
    let mut board = lock(&ctx.board);
    let entry = workspace_id.as_ref().and_then(|id| board.snapshot.workspaces.get_mut(id));
    let seen = entry.filter(|entry| status::when_viewed(entry.status) != entry.status).map(|entry| {
        entry.status = status::when_viewed(entry.status);
        entry.since = now_ms();
    });
    board.viewed = workspace_id;
    let snapshot = seen.map(|()| board.snapshot.clone());
    drop(board);
    if let Some(snapshot) = snapshot {
        let _ = snapshot.emit(&app);
    }
    let _ = ctx.poll_wake.try_send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminals::TerminalKind;
    use orchestrator_core::terminal::TerminalId;

    fn running_claude() -> TerminalInfo {
        TerminalInfo {
            id: TerminalId(1),
            workspace_id: "w".into(),
            kind: TerminalKind::Claude,
            running: true,
            exit_code: None,
        }
    }

    fn waiting(session: &str) -> AgentEntry {
        AgentEntry {
            session_id: session.into(),
            cwd: r"C:\repo".into(),
            status: Some("waiting".into()),
            waiting_for: Some("question".into()),
        }
    }

    #[test]
    fn running_claude_missing_from_the_list_is_not_exited_and_does_not_re_alert() {
        let saved = Tab {
            session_id: Some("s-saved"),
            folder: r"C:\repo".into(),
            claimed_session_ids: &["s-saved"],
        };
        let info = running_claude();
        let first = next_status(SessionStatus::Starting, &info, Some(&saved), Some(&[waiting("s-saved")]), false);
        assert_eq!(first, SessionStatus::NeedsInput);
        let gap = next_status(first, &info, Some(&saved), Some(&[]), false);
        let again = next_status(gap, &info, Some(&saved), Some(&[waiting("s-saved")]), false);
        assert!(
            gap != SessionStatus::Exited && !status::newly_needs_user(gap, again),
            "running process shown as {gap:?}, then re-alerted as {again:?}"
        );
    }

    /// npm layout: `claude.cmd` beside an extensionless sh script, and the real work in a grandchild.
    #[cfg(windows)]
    #[test]
    fn timed_out_agents_query_leaves_no_process_behind() {
        use std::process::Command;
        let folder = tempfile::tempdir().expect("tempdir");
        let orphan = format!("ao-orphan-{}.exe", std::process::id());
        std::fs::copy(r"C:\Windows\System32\PING.EXE", folder.path().join(&orphan)).expect("copy ping");
        std::fs::write(folder.path().join("claude"), "#!/bin/sh\nexit 1\n").expect("sh");
        std::fs::write(
            folder.path().join("claude.cmd"),
            format!("@echo off\r\n\"%~dp0{orphan}\" -n 30 127.0.0.1\r\n"),
        )
        .expect("cmd");
        let mut env = terminal::child_environment(std::env::vars_os());
        let key = env.keys().find(|name| name.eq_ignore_ascii_case("PATH")).cloned().unwrap_or_else(|| "PATH".into());
        let mut path = folder.path().as_os_str().to_owned();
        path.push(";");
        path.push(env.get(&key).cloned().unwrap_or_default());
        env.insert(key, path);

        let started = Instant::now();
        assert!(query_agents_with(&env).is_none());
        let took = started.elapsed();
        thread::sleep(Duration::from_millis(500));
        let listing = Command::new(r"C:\Windows\System32\tasklist.exe")
            .args(["/FI", &format!("IMAGENAME eq {orphan}"), "/NH"])
            .output()
            .expect("tasklist");
        let survivors = String::from_utf8_lossy(&listing.stdout).contains(&orphan);
        let _ = Command::new(r"C:\Windows\System32\taskkill.exe")
            .args(["/F", "/IM", &orphan])
            .output();
        assert!(took < Duration::from_secs(7), "query took {took:?}");
        assert!(!survivors, "{orphan} kept running (and holding the stdout pipe) after the query timed out");
    }
}
