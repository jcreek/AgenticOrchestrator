use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use orchestrator_core::state::{self, AppState, Settings, Workspace};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::status::Board;
use crate::terminals::{self, Registry};
use orchestrator_core::terminal::TerminalManager;
use orchestrator_core::watch::Watches;

pub struct Ctx {
    store: Mutex<Store>,
    pub terminals: TerminalManager,
    pub registry: Registry,
    pub board: Mutex<Board>,
    pub watches: Watches,
    /// Wakes the status poller early.
    pub poll_wake: SyncSender<()>,
    /// Held while opening terminals and while removing a workspace's terminals.
    launch: Mutex<()>,
}

const TERMINAL_EXIT_WAIT: Duration = Duration::from_secs(5);

struct Store {
    path: Option<PathBuf>,
    state: AppState,
    unreadable: Option<String>,
}

impl Ctx {
    pub fn load(app: &AppHandle, poll_wake: SyncSender<()>) -> Self {
        let _ = orchestrator_core::environment::environment();
        let loaded = state::default_state_path().and_then(|path| Ok((state::load(&path)?, path)));
        let store = match loaded {
            Ok((state, path)) => Store {
                path: Some(path),
                state,
                unreadable: None,
            },
            Err(error) => Store {
                path: None,
                state: AppState::default(),
                unreadable: Some(format!("The saved state could not be read, so nothing will be saved until it is fixed: {error}")),
            },
        };
        let registry = Registry::default();
        Self {
            store: Mutex::new(store),
            terminals: terminals::manager(app.clone(), registry.clone(), poll_wake.clone()),
            registry,
            board: Mutex::new(Board::default()),
            watches: Watches::default(),
            poll_wake,
            launch: Mutex::new(()),
        }
    }

    pub fn launching(&self) -> MutexGuard<'_, ()> {
        lock(&self.launch)
    }

    pub fn state(&self) -> AppResult<AppState> {
        let store = lock(&self.store);
        match &store.unreadable {
            Some(message) => Err(AppError::new(ErrorKind::StateUnreadable, message.clone())),
            None => Ok(store.state.clone()),
        }
    }

    pub fn workspace(&self, id: &str) -> AppResult<Workspace> {
        self.state()?
            .workspaces
            .into_iter()
            .find(|workspace| workspace.id == id)
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "no workspace with that id"))
    }

    pub fn root(&self, id: &str) -> AppResult<PathBuf> {
        Ok(self.workspace(id)?.path)
    }

    /// Applies `change` to a copy of the state and keeps it only once it is saved.
    pub fn mutate<T>(&self, change: impl FnOnce(&mut AppState) -> AppResult<T>) -> AppResult<T> {
        let mut store = lock(&self.store);
        if let Some(message) = &store.unreadable {
            return Err(AppError::new(ErrorKind::StateUnreadable, message.clone()));
        }
        let path = store
            .path
            .clone()
            .ok_or_else(|| AppError::new(ErrorKind::StateUnreadable, "no state path"))?;
        let mut next = store.state.clone();
        let result = change(&mut next)?;
        state::save(&path, &next)?;
        store.state = next;
        Ok(result)
    }

    pub fn remove_workspace(&self, id: &str, _launching: &MutexGuard<'_, ()>) -> AppResult<()> {
        self.close_terminals(id);
        lock(&self.board).forget(id);
        self.watches.forget(id);
        self.mutate(|state| {
            state::remove_workspace(state, id)
                .map(drop)
                .ok_or_else(|| AppError::new(ErrorKind::NotFound, "no workspace with that id"))
        })
    }

    /// Kills the workspace's terminals; true once all of their processes have ended.
    pub fn close_terminals(&self, workspace_id: &str) -> bool {
        let mut ended = true;
        for id in self.registry.ids_for(workspace_id) {
            ended &= self.terminals.remove_and_wait(id, TERMINAL_EXIT_WAIT).unwrap_or(false);
            self.registry.remove(id);
        }
        ended
    }

    pub fn close_all_terminals(&self) {
        for id in self.registry.all_ids() {
            let _ = self.terminals.remove(id);
        }
    }
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| AppError::new(ErrorKind::Internal, "background task failed"))?
}

#[tauri::command]
#[specta::specta]
pub async fn get_state(app: AppHandle) -> AppResult<AppState> {
    blocking(move || app.state_ctx().state()).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_workspace(app: AppHandle, path: String) -> AppResult<Workspace> {
    blocking(move || {
        app.state_ctx().mutate(|state| {
            Ok(state::add_workspace(state, &PathBuf::from(path), None)?.clone())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_workspace(app: AppHandle, id: String) -> AppResult<()> {
    blocking(move || {
        let ctx = app.state_ctx();
        let launching = ctx.launching();
        ctx.remove_workspace(&id, &launching)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn reorder_workspaces(app: AppHandle, ids: Vec<String>) -> AppResult<()> {
    blocking(move || {
        app.state_ctx().mutate(|state| {
            state::reorder_workspaces(state, &ids);
            Ok(())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn update_settings(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    blocking(move || {
        let saved = app.state_ctx().mutate(|state| {
            state.settings = settings;
            Ok(state.settings.clone())
        })?;
        apply_zoom(&app, &saved);
        Ok(saved)
    })
    .await
}

pub fn apply_zoom(app: &AppHandle, settings: &Settings) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_zoom(settings.zoom_factor());
    }
}

/// Opens the native folder picker; `None` when the user cancels.
#[tauri::command]
#[specta::specta]
pub async fn pick_folder(app: AppHandle) -> AppResult<Option<String>> {
    blocking(move || {
        let Some(picked) = app.dialog().file().blocking_pick_folder() else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|error| AppError::new(ErrorKind::InvalidArgument, error.to_string()))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}

pub trait CtxExt {
    fn state_ctx(&self) -> State<'_, Ctx>;
}

impl CtxExt for AppHandle {
    fn state_ctx(&self) -> State<'_, Ctx> {
        tauri::Manager::state::<Ctx>(self)
    }
}
