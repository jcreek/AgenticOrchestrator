use orchestrator_core::files::{self, DirEntry, FileContent};
use tauri::AppHandle;

use crate::error::AppResult;
use crate::state::{CtxExt, blocking};

#[tauri::command]
#[specta::specta]
pub async fn list_dir(app: AppHandle, workspace_id: String, rel_path: String) -> AppResult<Vec<DirEntry>> {
    let root = app.state_ctx().root(&workspace_id)?;
    blocking(move || Ok(files::list_dir(&root, &rel_path)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn read_file(app: AppHandle, workspace_id: String, rel_path: String) -> AppResult<FileContent> {
    let root = app.state_ctx().root(&workspace_id)?;
    blocking(move || Ok(files::read_file(&root, &rel_path)?)).await
}

/// Saves atomically and returns the new version. Pass the version from `readFile` to save an
/// existing file, or `null` to create a new one; either fails with `conflict` rather than
/// overwriting a file that changed or already exists.
#[tauri::command]
#[specta::specta]
pub async fn write_file(
    app: AppHandle,
    workspace_id: String,
    rel_path: String,
    text: String,
    expected_version: Option<String>,
) -> AppResult<String> {
    let root = app.state_ctx().root(&workspace_id)?;
    blocking(move || Ok(files::write_file(&root, &rel_path, &text, expected_version.as_deref())?)).await
}

/// `None` when the file does not exist.
#[tauri::command]
#[specta::specta]
pub async fn file_version(app: AppHandle, workspace_id: String, rel_path: String) -> AppResult<Option<String>> {
    let root = app.state_ctx().root(&workspace_id)?;
    blocking(move || Ok(files::file_version(&root, &rel_path)?)).await
}
