mod error;
mod files;
mod git;
mod misc;
mod state;
mod status;
mod terminals;
mod worktrees;

use tauri::{Manager, RunEvent};
use tauri_plugin_window_state::StateFlags;
use tauri_specta::{collect_commands, collect_events};

use crate::state::Ctx;

#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            state::get_state,
            state::add_workspace,
            state::remove_workspace,
            state::reorder_workspaces,
            state::update_settings,
            state::pick_folder,
            terminals::terminal_open,
            terminals::terminal_attach,
            terminals::terminal_write,
            terminals::terminal_resize,
            terminals::terminal_kill,
            terminals::terminal_close,
            terminals::list_terminals,
            status::get_statuses,
            status::set_viewed_workspace,
            git::workspace_summary,
            git::workspace_revisions,
            git::git_status,
            git::git_file_at_head,
            git::git_default_base,
            git::git_branch_changes,
            git::git_file_at_merge_base,
            git::git_stage,
            git::git_unstage,
            git::git_stage_all,
            git::git_commit,
            git::git_push,
            git::list_branches,
            files::list_dir,
            files::read_file,
            files::write_file,
            files::file_version,
            worktrees::create_worktree,
            worktrees::worktree_removal_blockers,
            worktrees::remove_worktree,
            misc::open_in_vscode,
            misc::check_tools,
            misc::detect_nerd_font,
        ])
        .events(collect_events![status::StatusChanged, terminals::TerminalExited])
        .typ::<error::AppError>()
        .constant("KILLED_EXIT_CODE", orchestrator_core::terminal::KILLED_EXIT_CODE)
}

#[cfg(any(debug_assertions, test))]
fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>, path: &std::path::Path) -> Result<(), String> {
    builder
        .export(specta_typescript::Typescript::default(), path)
        .map_err(|error| error.to_string())
}

// WebView2 keeps a minimised window's page visible and rendering; hiding it pauses rendering and sets `document.hidden`.
#[cfg(windows)]
fn hide_webview_while_minimised(window: &tauri::WebviewWindow) {
    let visible = !window.is_minimized().unwrap_or(false);
    let _ = window.with_webview(move |webview| {
        // SAFETY: the controller is a live COM interface owned by the webview for the duration of this callback.
        let _ = unsafe { webview.controller().SetIsVisible(visible) };
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();
    #[cfg(debug_assertions)]
    if let Err(error) = export_bindings(&builder, std::path::Path::new(BINDINGS_PATH)) {
        eprintln!("could not export TypeScript bindings: {error}");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::DECORATIONS)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let (poll_wake, woken) = std::sync::mpsc::sync_channel(1);
            app.manage(Ctx::load(app.handle(), poll_wake));
            if let Ok(saved) = app.state::<Ctx>().state() {
                state::apply_zoom(app.handle(), &saved.settings);
            }
            status::start_poller(app.handle().clone(), woken);
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                let target = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(_) = event {
                        hide_webview_while_minimised(&target);
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!());

    match app {
        // The process exits without dropping managed state, so terminals are killed here.
        Ok(app) => app.run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<Ctx>().close_all_terminals();
            }
        }),
        Err(error) => eprintln!("could not start the app: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_bindings_match_the_commands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let generated_path = dir.path().join("bindings.ts");
        export_bindings(&specta_builder(), &generated_path).unwrap_or_else(|error| panic!("{error}"));
        let generated = std::fs::read_to_string(&generated_path).expect("generated bindings");
        if std::env::var_os("UPDATE_BINDINGS").is_some_and(|value| value == "1") {
            std::fs::write(BINDINGS_PATH, &generated).expect("write bindings");
            return;
        }
        let committed = std::fs::read_to_string(BINDINGS_PATH).unwrap_or_default();
        assert!(
            committed.replace("\r\n", "\n") == generated.replace("\r\n", "\n"),
            "app/src/bindings.ts is out of date; regenerate it with UPDATE_BINDINGS=1 cargo test -p app committed_bindings"
        );
    }
}
