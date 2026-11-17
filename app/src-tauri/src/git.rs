use std::collections::HashMap;
use std::path::PathBuf;

use orchestrator_core::git::{self, BranchFile, Branches, ChangedFile, RepoGroup};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::AppResult;
use crate::state::{CtxExt, blocking};

/// A repository named by an existing workspace or by a folder from `pickFolder`.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RepoRef {
    Workspace { id: String },
    Folder { path: String },
}

impl RepoRef {
    pub fn resolve(&self, app: &AppHandle) -> AppResult<PathBuf> {
        match self {
            Self::Workspace { id } => {
                let workspace = app.state_ctx().workspace(id)?;
                Ok(workspace
                    .created_worktree
                    .map_or(workspace.path, |created| created.repo_root))
            }
            Self::Folder { path } => Ok(PathBuf::from(path)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceKind {
    Folder,
    Repo,
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub kind: WorkspaceKind,
    /// Created by this app, so `removeWorktree` may delete it.
    pub removable: bool,
    pub folder_missing: bool,
    pub group: Option<RepoGroup>,
    pub branch: Option<String>,
    pub changed_files: u32,
    pub added: u32,
    pub removed: u32,
}

/// Tab-strip facts for one workspace; does not fail because the folder is gone or not a repo.
#[tauri::command]
#[specta::specta]
pub async fn workspace_summary(app: AppHandle, workspace_id: String) -> AppResult<WorkspaceSummary> {
    let workspace = app.state_ctx().workspace(&workspace_id)?;
    blocking(move || {
        let mut summary = WorkspaceSummary {
            kind: WorkspaceKind::Folder,
            removable: workspace.created_worktree.is_some(),
            folder_missing: !workspace.path.is_dir(),
            group: None,
            branch: None,
            changed_files: 0,
            added: 0,
            removed: 0,
        };
        if summary.folder_missing {
            return Ok(summary);
        }
        app.state_ctx().watches.ensure(&workspace_id, &workspace.path);
        let Some(facts) = git::repo_facts(&workspace.path)? else {
            return Ok(summary);
        };
        summary.kind = if facts.linked_worktree {
            WorkspaceKind::Worktree
        } else {
            WorkspaceKind::Repo
        };
        summary.group = Some(facts.group);
        summary.branch = facts.branch;
        summary.changed_files = u32::try_from(facts.changes.len()).unwrap_or(u32::MAX);
        summary.added = facts.changes.iter().filter_map(|file| file.added).sum();
        summary.removed = facts.changes.iter().filter_map(|file| file.removed).sum();
        Ok(summary)
    })
    .await
}

/// A number per watched workspace that changes when anything in its folder does. A workspace
/// that is missing here is not watched, so its git state must be polled.
#[tauri::command]
#[specta::specta]
pub fn workspace_revisions(app: AppHandle) -> HashMap<String, u32> {
    app.state_ctx().watches.revisions()
}

async fn in_repo<T: Send + 'static>(
    app: &AppHandle,
    workspace_id: &str,
    work: impl FnOnce(PathBuf) -> Result<T, git::GitError> + Send + 'static,
) -> AppResult<T> {
    let root = app.state_ctx().root(workspace_id)?;
    blocking(move || Ok(work(root)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_status(app: AppHandle, workspace_id: String) -> AppResult<Vec<ChangedFile>> {
    in_repo(&app, &workspace_id, |root| git::status(&root)).await
}

/// `None` when the file does not exist at HEAD.
#[tauri::command]
#[specta::specta]
pub async fn git_file_at_head(app: AppHandle, workspace_id: String, path: String) -> AppResult<Option<String>> {
    in_repo(&app, &workspace_id, move |root| git::file_at_head(&root, &path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_default_base(app: AppHandle, workspace_id: String) -> AppResult<Option<String>> {
    in_repo(&app, &workspace_id, |root| git::default_base_branch(&root)).await
}

/// Everything that differs from the merge-base with `base`, including uncommitted and untracked files.
#[tauri::command]
#[specta::specta]
pub async fn git_branch_changes(app: AppHandle, workspace_id: String, base: String) -> AppResult<Vec<BranchFile>> {
    in_repo(&app, &workspace_id, move |root| git::branch_changes(&root, &base)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_file_at_merge_base(
    app: AppHandle,
    workspace_id: String,
    path: String,
    base: String,
) -> AppResult<Option<String>> {
    in_repo(&app, &workspace_id, move |root| git::file_at_merge_base(&root, &path, &base)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_stage(app: AppHandle, workspace_id: String, paths: Vec<String>) -> AppResult<()> {
    in_repo(&app, &workspace_id, move |root| git::stage(&root, &paths)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_unstage(app: AppHandle, workspace_id: String, paths: Vec<String>) -> AppResult<()> {
    in_repo(&app, &workspace_id, move |root| git::unstage(&root, &paths)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_stage_all(app: AppHandle, workspace_id: String) -> AppResult<()> {
    in_repo(&app, &workspace_id, |root| git::stage_all(&root)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_commit(app: AppHandle, workspace_id: String, message: String) -> AppResult<()> {
    in_repo(&app, &workspace_id, move |root| git::commit(&root, &message)).await
}

#[tauri::command]
#[specta::specta]
pub async fn git_push(app: AppHandle, workspace_id: String) -> AppResult<()> {
    in_repo(&app, &workspace_id, |root| git::push(&root)).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_branches(app: AppHandle, repo: RepoRef) -> AppResult<Branches> {
    let root = repo.resolve(&app)?;
    blocking(move || Ok(git::list_branches(&root)?)).await
}
