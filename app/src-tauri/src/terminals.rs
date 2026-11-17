use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use orchestrator_core::terminal::{
    self, ClaudeLaunch, TerminalId, TerminalManager, TerminalSize, TerminalSpec,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};
use tauri::{AppHandle, State};
use tauri_specta::Event;
use uuid::Uuid;

use orchestrator_core::state::Workspace;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::{Ctx, CtxExt, blocking, lock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TerminalKind {
    Claude,
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: TerminalId,
    pub workspace_id: String,
    pub kind: TerminalKind,
    pub running: bool,
    pub exit_code: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExited {
    pub id: TerminalId,
    pub exit_code: u32,
}

/// Raw terminal output; the front end receives each message as an `ArrayBuffer`.
pub struct TerminalBytes(Vec<u8>);

impl IpcResponse for TerminalBytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

impl specta::Type for TerminalBytes {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta_typescript::define("ArrayBuffer").into()
    }
}

#[derive(Clone, Default)]
pub struct Registry(Arc<Mutex<HashMap<TerminalId, TerminalInfo>>>);

impl Registry {
    pub fn list(&self, workspace_id: &str) -> Vec<TerminalInfo> {
        let mut found: Vec<_> = lock(&self.0)
            .values()
            .filter(|info| info.workspace_id == workspace_id)
            .cloned()
            .collect();
        found.sort_by_key(|info| info.id);
        found
    }

    pub fn ids_for(&self, workspace_id: &str) -> Vec<TerminalId> {
        self.list(workspace_id).into_iter().map(|info| info.id).collect()
    }

    pub fn all_ids(&self) -> Vec<TerminalId> {
        lock(&self.0).keys().copied().collect()
    }

    pub fn remove(&self, id: TerminalId) {
        lock(&self.0).remove(&id);
    }

    fn kind(&self, id: TerminalId) -> Option<TerminalKind> {
        lock(&self.0).get(&id).map(|info| info.kind)
    }

    /// The newest Claude terminal of each workspace.
    pub fn claude_terminals(&self) -> HashMap<String, TerminalInfo> {
        let mut newest: HashMap<String, TerminalInfo> = HashMap::new();
        for info in lock(&self.0).values().filter(|info| info.kind == TerminalKind::Claude) {
            if newest.get(&info.workspace_id).is_none_or(|current| current.id < info.id) {
                newest.insert(info.workspace_id.clone(), info.clone());
            }
        }
        newest
    }
}

pub fn manager(app: AppHandle, registry: Registry, poll_wake: SyncSender<()>) -> TerminalManager {
    TerminalManager::new(move |id, exit_code| {
        if let Some(info) = lock(&registry.0).get_mut(&id) {
            info.running = false;
            info.exit_code = Some(exit_code);
        }
        let _ = poll_wake.try_send(());
        let _ = TerminalExited { id, exit_code }.emit(&app);
    })
}

/// Output from a Claude terminal also wakes the status poller, since its status may have changed.
fn output_sink(ctx: &Ctx, kind: Option<TerminalKind>, channel: Channel<TerminalBytes>) -> terminal::OutputSink {
    let poll_wake = (kind == Some(TerminalKind::Claude)).then(|| ctx.poll_wake.clone());
    Box::new(move |bytes| {
        let _ = channel.send(TerminalBytes(bytes.to_vec()));
        if let Some(poll_wake) = &poll_wake {
            let _ = poll_wake.try_send(());
        }
    })
}

fn spawn(
    ctx: &Ctx,
    spec: TerminalSpec,
    workspace_id: String,
    kind: TerminalKind,
    channel: Channel<TerminalBytes>,
) -> AppResult<TerminalInfo> {
    // Holding the registry lock across spawn makes a fast exit wait until the entry exists.
    let mut registry = lock(&ctx.registry.0);
    let id = ctx.terminals.spawn(spec)?;
    let info = TerminalInfo {
        id,
        workspace_id,
        kind,
        running: true,
        exit_code: None,
    };
    registry.insert(id, info.clone());
    drop(registry);
    ctx.terminals.attach(id, output_sink(ctx, Some(kind), channel))?;
    Ok(info)
}

fn claude_launch(workspace: &Workspace, resume: bool) -> AppResult<ClaudeLaunch> {
    if !resume {
        // A saved id may already exist on disk, which `--session-id` rejects, so a new launch gets a new id.
        return Ok(ClaudeLaunch::New(Uuid::new_v4()));
    }
    let id = workspace
        .claude_session_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| AppError::new(ErrorKind::NotFound, "this workspace has no saved Claude session"))?;
    Ok(ClaudeLaunch::Resume(id))
}

fn save_session_id(ctx: &Ctx, workspace_id: &str, id: Uuid) -> AppResult<()> {
    ctx.mutate(|state| {
        let workspace = state
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "no workspace with that id"))?;
        workspace.claude_session_id = Some(id.to_string());
        Ok(())
    })
}

/// Opens a terminal and streams its raw output bytes to `on_output`. A Claude terminal that is
/// still running is re-attached instead of spawning a second one.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn terminal_open(
    app: AppHandle,
    workspace_id: String,
    kind: TerminalKind,
    cols: u16,
    rows: u16,
    resume: bool,
    prompt: Option<String>,
    on_output: Channel<TerminalBytes>,
) -> AppResult<TerminalInfo> {
    let size = TerminalSize { cols, rows };
    blocking(move || {
        let info = open(&app.state_ctx(), workspace_id, kind, size, resume, prompt.as_deref(), on_output)?;
        if kind == TerminalKind::Claude {
            crate::status::publish(&app);
        }
        Ok(info)
    })
    .await
}

fn open(
    ctx: &Ctx,
    workspace_id: String,
    kind: TerminalKind,
    size: TerminalSize,
    resume: bool,
    prompt: Option<&str>,
    on_output: Channel<TerminalBytes>,
) -> AppResult<TerminalInfo> {
    let _launching = ctx.launching();
    let workspace = ctx.workspace(&workspace_id)?;
    let settings = ctx.state()?.settings;
    let env = terminal::child_environment(std::env::vars_os());
    if kind == TerminalKind::Shell {
        let mut spec = terminal::shell_spec(Some(&settings.shell_program), workspace.path, size, env);
        spec.args = settings.shell_args;
        return spawn(ctx, spec, workspace_id, kind, on_output);
    }

    if let Some(existing) = ctx.registry.claude_terminals().remove(&workspace_id) {
        if existing.running {
            ctx.terminals.attach(existing.id, output_sink(ctx, Some(kind), on_output))?;
            return Ok(existing);
        }
        let _ = ctx.terminals.remove(existing.id);
        ctx.registry.remove(existing.id);
    }
    let launch = claude_launch(&workspace, resume)?;
    let mut spec = terminal::claude_spec(launch, &settings.extra_claude_args, workspace.path, size, env);
    if let (ClaudeLaunch::New(_), Some(prompt)) = (launch, prompt) {
        terminal::append_prompt(&mut spec, prompt);
    }
    let info = spawn(ctx, spec, workspace_id.clone(), kind, on_output)?;
    if let ClaudeLaunch::New(id) = launch
        && let Err(error) = save_session_id(ctx, &workspace_id, id)
    {
        let _ = ctx.terminals.remove(info.id);
        ctx.registry.remove(info.id);
        return Err(error);
    }
    lock(&ctx.board).started(&workspace_id);
    Ok(info)
}

/// Replays the scrollback into `on_output`, then streams live output to it instead of any
/// earlier channel.
#[tauri::command]
#[specta::specta]
pub async fn terminal_attach(app: AppHandle, id: TerminalId, on_output: Channel<TerminalBytes>) -> AppResult<()> {
    // The registry lock is held while a process spawns, so this must not wait on the main thread.
    blocking(move || {
        let ctx = app.state_ctx();
        let kind = ctx.registry.kind(id);
        Ok(ctx.terminals.attach(id, output_sink(&ctx, kind, on_output))?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_write(app: AppHandle, id: TerminalId, data: String) -> AppResult<()> {
    blocking(move || Ok(app.state_ctx().terminals.write(id, data.as_bytes())?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_resize(ctx: State<'_, Ctx>, id: TerminalId, cols: u16, rows: u16) -> AppResult<()> {
    Ok(ctx.terminals.resize(id, TerminalSize { cols, rows })?)
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_kill(ctx: State<'_, Ctx>, id: TerminalId) -> AppResult<()> {
    Ok(ctx.terminals.kill(id)?)
}

/// Kills the terminal if it is running and forgets it and its scrollback.
#[tauri::command]
#[specta::specta]
pub async fn terminal_close(ctx: State<'_, Ctx>, id: TerminalId) -> AppResult<()> {
    ctx.registry.remove(id);
    Ok(ctx.terminals.remove(id)?)
}

#[tauri::command]
#[specta::specta]
pub async fn list_terminals(app: AppHandle, workspace_id: String) -> Vec<TerminalInfo> {
    // The registry lock is held while a process spawns, so this must not wait on the main thread.
    tauri::async_runtime::spawn_blocking(move || app.state_ctx().registry.list(&workspace_id))
        .await
        .unwrap_or_default()
}
