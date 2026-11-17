use std::path::PathBuf;

use orchestrator_core::state::{self, CreatedWorktree, Workspace};
use orchestrator_core::worktrees::{self, Blocker, CommandOutput};
use serde::Serialize;
use tauri::AppHandle;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::git::RepoRef;
use crate::state::{CtxExt, blocking};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreatedWorkspace {
    pub workspace: Workspace,
    /// Output of the post-create command, when one is configured.
    pub post_create: Option<CommandOutput>,
}

fn configured(cwd: &std::path::Path, argv: &[String]) -> Option<CommandOutput> {
    (!argv.is_empty()).then(|| worktrees::run_command(cwd, argv))
}

/// Creates a worktree (new branch from `base`, or an existing branch) and adds it as a workspace.
#[tauri::command]
#[specta::specta]
pub async fn create_worktree(
    app: AppHandle,
    repo: RepoRef,
    branch: String,
    base: Option<String>,
) -> AppResult<CreatedWorkspace> {
    let repo_root = repo.resolve(&app)?;
    let settings = app.state_ctx().state()?.settings;
    blocking(move || {
        let worktree_root = settings.worktree_root.as_deref().map(PathBuf::from);
        let path = worktrees::create(&repo_root, &branch, base.as_deref(), worktree_root.as_deref(), true)?;
        let workspace = app.state_ctx().mutate(|state| {
            state::add_workspace(state, &path, None)?;
            let added = state
                .workspaces
                .last_mut()
                .ok_or_else(|| AppError::new(ErrorKind::Internal, "workspace was not added"))?;
            added.created_worktree = Some(CreatedWorktree { repo_root, branch });
            Ok(added.clone())
        })?;
        let post_create = configured(&path, &settings.post_create_command);
        Ok(CreatedWorkspace { workspace, post_create })
    })
    .await
}

/// Work that removing this worktree would lose; empty when it is safe to remove.
#[tauri::command]
#[specta::specta]
pub async fn worktree_removal_blockers(app: AppHandle, workspace_id: String) -> AppResult<Vec<Blocker>> {
    let root = app.state_ctx().root(&workspace_id)?;
    blocking(move || Ok(worktrees::removal_blockers(&root)?)).await
}

/// Removes a worktree this app created and its workspace. `accepted` is the blocker list the user
/// saw and agreed to lose (empty for a clean worktree); if the blockers have changed since, fails
/// with `worktreeBlocked` and removes nothing. Returns the pre-remove command's output, if any.
#[tauri::command]
#[specta::specta]
pub async fn remove_worktree(
    app: AppHandle,
    workspace_id: String,
    accepted: Vec<Blocker>,
) -> AppResult<Option<CommandOutput>> {
    let workspace = app.state_ctx().workspace(&workspace_id)?;
    let created = workspace.created_worktree.ok_or_else(|| {
        AppError::new(ErrorKind::NotRemovable, "only worktrees created by this app can be removed")
    })?;
    let settings = app.state_ctx().state()?.settings;
    blocking(move || {
        let path = workspace.path;
        // Checked before any terminal is killed; `remove` checks again after the pre-remove command.
        worktrees::check_removable(&created.repo_root, &path, &accepted)?;
        let ctx = app.state_ctx();
        let launching = ctx.launching();
        if !ctx.close_terminals(&workspace_id) {
            return Err(AppError::new(ErrorKind::Io, "a terminal in this worktree did not stop; try again"));
        }
        let pre_remove = configured(&path, &settings.pre_remove_command);
        ctx.watches.suspend(&workspace_id);
        if let Err(error) = worktrees::remove(&created.repo_root, &path, &accepted) {
            ctx.watches.resume(&workspace_id);
            return Err(error.into());
        }
        ctx.remove_workspace(&workspace_id, &launching)?;
        Ok(pre_remove)
    })
    .await
}
