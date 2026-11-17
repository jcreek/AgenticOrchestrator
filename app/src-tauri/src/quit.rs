//! Wiring for native quit requests; the frontend owns the existing close guard.
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct QuitState {
    ready: AtomicBool,
    approved: AtomicBool,
}

impl QuitState {
    #[cfg(any(target_os = "macos", test))]
    pub fn should_guard(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && !self.approved.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, serde::Serialize, specta::Type, tauri_specta::Event)]
pub struct QuitRequested;

/// Called only after the quit-event listener is installed.
#[tauri::command]
#[specta::specta]
pub fn quit_guard_ready(state: State<'_, QuitState>, ready: bool) {
    state.ready.store(ready, Ordering::SeqCst);
}

#[tauri::command]
#[specta::specta]
pub fn approve_quit(app: AppHandle) {
    app.state::<QuitState>()
        .approved
        .store(true, Ordering::SeqCst);
    app.exit(0);
}

/// Keep standard editing and app actions, without the native Cmd+W window-close
/// accelerator: Cmd+W belongs to the selected session in this app.
#[cfg(target_os = "macos")]
pub fn macos_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, PredefinedMenuItem as Item, Submenu};
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                "Agentic Orchestrator",
                true,
                &[
                    &Item::about(app, None, None)?,
                    &Item::separator(app)?,
                    &Item::services(app, None)?,
                    &Item::separator(app)?,
                    &Item::hide(app, None)?,
                    &Item::hide_others(app, None)?,
                    &Item::show_all(app, None)?,
                    &Item::separator(app)?,
                    &Item::quit(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &Item::undo(app, None)?,
                    &Item::redo(app, None)?,
                    &Item::separator(app)?,
                    &Item::cut(app, None)?,
                    &Item::copy(app, None)?,
                    &Item::paste(app, None)?,
                    &Item::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(app, "View", true, &[&Item::fullscreen(app, None)?])?,
            &Submenu::with_items(
                app,
                "Window",
                true,
                &[&Item::minimize(app, None)?, &Item::maximize(app, None)?],
            )?,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_stay_guarded_until_explicit_approval() {
        let state = QuitState::default();
        assert!(!state.should_guard());
        state.ready.store(true, Ordering::SeqCst);
        assert!(state.should_guard());
        // Cancellation does not approve anything; repeated requests remain guarded.
        assert!(state.should_guard());
        state.approved.store(true, Ordering::SeqCst);
        assert!(!state.should_guard());
    }
}
