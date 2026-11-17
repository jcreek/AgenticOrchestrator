use std::path::PathBuf;

use cucumber::{given, then, when};
use orchestrator_core::git::{RepoGroup, repo_group};
use orchestrator_core::{state, worktrees};

use crate::{CoreWorld, off_thread};

async fn group_of(path: PathBuf) -> RepoGroup {
    off_thread(move || repo_group(&path).expect("git runs").expect("folder is in a repository")).await
}

#[given(expr = "a repository {string}")]
fn a_repository(world: &mut CoreWorld, name: String) {
    world.new_repo(&name);
}

#[given(expr = "a repository {string} with a remote")]
fn a_repository_with_remote(world: &mut CoreWorld, name: String) {
    world.new_repo_with_remote(&name);
}

#[given(expr = "a worktree of {string} on the new branch {string}")]
async fn a_worktree(world: &mut CoreWorld, repo: String, branch: String) {
    let repo = world.repo(&repo);
    let from = repo.clone();
    let path = off_thread(move || worktrees::create(&from, &branch, None, None, false).expect("create worktree")).await;
    world.worktree = Some((repo, path));
}

#[then(expr = "the worktree {string} is in the {string} group")]
async fn worktree_in_group(world: &mut CoreWorld, _branch: String, group: String) {
    let found = group_of(world.worktree().1).await;
    let expected = group_of(world.repo(&group)).await;
    assert_eq!(found, expected);
    assert_eq!(found.name, group);
}

#[then(expr = "{string} is in a group of its own named {string}")]
async fn group_of_its_own(world: &mut CoreWorld, repo: String, name: String) {
    let group = group_of(world.repo(&repo)).await;
    assert_eq!(group.name, name);
    for (other, path) in world.repos.clone() {
        if other != repo {
            assert_ne!(group_of(path).await.key, group.key, "{repo} shares a group with {other}");
        }
    }
}

#[given(expr = "a repository {string} open as a workspace")]
fn repository_open_as_workspace(world: &mut CoreWorld, name: String) {
    let path = world.new_repo(&name);
    state::add_workspace(&mut world.state, &path, None).expect("add workspace");
}

#[when(expr = "I open {string} as a workspace again, spelled in upper case")]
fn open_again_upper_case(world: &mut CoreWorld, name: String) {
    let upper = PathBuf::from(world.repo(&name).to_string_lossy().to_uppercase());
    let result = state::add_workspace(&mut world.state, &upper, None).map(|workspace| workspace.id.clone());
    world.record(result);
}

#[then("it is refused because the folder is already open")]
fn refused_already_open(world: &mut CoreWorld) {
    world.assert_refused("DuplicateWorkspace");
}

#[then("there is still one workspace")]
fn one_workspace(world: &mut CoreWorld) {
    assert_eq!(world.state.workspaces.len(), 1);
}
