use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use orchestrator_core::worktrees::{self, Blocker, BlockerKind};

use crate::{CoreWorld, commit_file, git, init_repo, junction, off_thread, write};

fn same_place(left: &Path, right: &Path) -> bool {
    fs::canonicalize(left).ok().is_some_and(|left| Some(left) == fs::canonicalize(right).ok())
}

async fn blockers_of(path: PathBuf) -> Vec<Blocker> {
    off_thread(move || worktrees::removal_blockers(&path).expect("blockers")).await
}

#[when(expr = "I create a worktree of {string} on the new branch {string}")]
async fn create(world: &mut CoreWorld, name: String, branch: String) {
    let repo = world.repo(&name);
    let from = repo.clone();
    let result = off_thread(move || worktrees::create(&from, &branch, None, None, true)).await;
    if let Some(path) = world.record(result) {
        world.worktree = Some((repo, path));
    }
}

#[then(expr = "the worktree is in the folder {string} beside {string}")]
fn in_folder(world: &mut CoreWorld, folder: String, name: String) {
    let (_, path) = world.worktree();
    let expected = world.repo(&name).parent().expect("parent").join(folder);
    assert!(same_place(&path, &expected), "worktree is at {}, expected {}", path.display(), expected.display());
}

#[then(expr = "the worktree is on the branch {string}")]
async fn on_branch(world: &mut CoreWorld, branch: String) {
    let (_, path) = world.worktree();
    assert_eq!(off_thread(move || git(&path, &["symbolic-ref", "--short", "HEAD"])).await, branch);
}

#[then(expr = "the branch {string} does not track {string}")]
async fn does_not_track(world: &mut CoreWorld, branch: String, _base: String) {
    let (repo, _) = world.worktree();
    let upstream = off_thread(move || {
        let key = format!("branch.{branch}.merge");
        Command::new("git").arg("-C").arg(&repo).args(["config", "--get", &key]).output().expect("run git").stdout
    })
    .await;
    assert!(upstream.is_empty(), "the branch tracks {}", String::from_utf8_lossy(&upstream));
}

#[then("it is refused as an invalid branch name")]
fn invalid_branch(world: &mut CoreWorld) {
    world.assert_refused("InvalidBranch");
}

#[then(expr = "{string} still has only its main worktree")]
async fn only_main(world: &mut CoreWorld, name: String) {
    let repo = world.repo(&name);
    assert!(!world.dir.path().join(format!("{name}.worktrees")).exists(), "a worktree folder was created");
    assert!(!world.dir.path().join("evil").exists());
    let listed = off_thread(move || git(&repo, &["worktree", "list", "--porcelain"])).await;
    assert_eq!(listed.matches("worktree ").count(), 1, "{listed}");
}

#[given(expr = "{string} has the untracked files {string}, {string} and {string}")]
fn untracked_files(world: &mut CoreWorld, name: String, first: String, second: String, third: String) {
    let repo = world.repo(&name);
    for file in [first, second, third] {
        write(&repo, &file, "local only\n");
    }
}

#[given(expr = "the {string} of {string} lists {string} and {string}")]
fn include_list(world: &mut CoreWorld, list: String, name: String, first: String, second: String) {
    write(&world.repo(&name), &list, &format!("{first}\n{second}\n"));
}

#[then(expr = "the worktree has {string} and {string}")]
fn has_both(world: &mut CoreWorld, first: String, second: String) {
    let (_, path) = world.worktree();
    for file in [first, second] {
        assert!(path.join(&file).is_file(), "{file} was not copied");
    }
}

#[then(expr = "the worktree does not have {string}")]
fn does_not_have(world: &mut CoreWorld, file: String) {
    assert!(!world.worktree().1.join(&file).exists(), "{file} was copied");
}

#[then(expr = "the worktree still has {string}")]
fn still_has(world: &mut CoreWorld, file: String) {
    assert!(world.worktree().1.join(&file).exists(), "{file} is gone");
}

#[when("in the worktree a tracked file is modified, a file is added, a log file is written and a commit is made on a detached HEAD")]
async fn make_every_blocker(world: &mut CoreWorld) {
    let (_, path) = world.worktree();
    off_thread(move || {
        write(&path, "README.md", "changed\n");
        write(&path, "new.txt", "new\n");
        write(&path, "build.log", "log\n");
        commit_file(&path, "committed.txt", "committed\n");
        git(&path, &["checkout", "-q", "--detach"]);
    })
    .await;
}

#[then("removing the worktree would lose:")]
async fn would_lose(world: &mut CoreWorld, step: &Step) {
    let kind = |what: &str| match what {
        "modified files" => BlockerKind::ModifiedFiles,
        "untracked files" => BlockerKind::UntrackedFiles,
        "ignored files" => BlockerKind::IgnoredFiles,
        "unpushed commits" => BlockerKind::UnpushedCommits,
        other => panic!("unknown kind {other:?}"),
    };
    let rows = &step.table.as_ref().expect("table").rows;
    let expected: Vec<Blocker> = rows
        .iter()
        .skip(1)
        .map(|row| Blocker { kind: kind(&row[0]), count: row[1].parse().expect("count") })
        .collect();
    assert_eq!(blockers_of(world.worktree().1).await, expected);
}

#[given("a log file is written in the worktree")]
fn log_written(world: &mut CoreWorld) {
    write(&world.worktree().1, "build.log", "log\n");
}

#[given("I have been shown what removing the worktree would lose")]
async fn shown(world: &mut CoreWorld) {
    world.shown = blockers_of(world.worktree().1).await;
}

#[when(expr = "Claude creates {string} in the worktree")]
fn claude_creates(world: &mut CoreWorld, file: String) {
    write(&world.worktree().1, &file, "new work\n");
}

#[when("I confirm the removal")]
async fn confirm_removal(world: &mut CoreWorld) {
    let (repo, path) = world.worktree();
    let shown = world.shown.clone();
    let result = off_thread(move || worktrees::remove(&repo, &path, &shown)).await;
    world.record(result);
}

#[then(regex = r#"^the removal is refused because (the worktree changed|it is the main worktree|it is not a worktree of ".*"|the worktree contains another worktree|the worktree has submodules)$"#)]
fn removal_refused(world: &mut CoreWorld, reason: String) {
    let variant = match reason.as_str() {
        "the worktree changed" => "Blocked",
        "it is the main worktree" => "MainWorktree",
        "the worktree contains another worktree" => "ContainsWorktree",
        "the worktree has submodules" => "HasSubmodules",
        _ => "UnknownWorktree",
    };
    world.assert_refused(variant);
}

#[given("a commit is made in the worktree")]
async fn commit_in_worktree(world: &mut CoreWorld) {
    let (_, path) = world.worktree();
    world.commit = Some(
        off_thread(move || {
            commit_file(&path, "work.txt", "committed work\n");
            git(&path, &["rev-parse", "HEAD"])
        })
        .await,
    );
}

#[then("the worktree folder is gone")]
fn folder_gone(world: &mut CoreWorld) {
    assert_eq!(world.refusal, None, "the removal was refused");
    assert!(!world.worktree().1.exists(), "the worktree folder still exists");
}

#[then(expr = "the branch {string} still has that commit")]
async fn branch_has_commit(world: &mut CoreWorld, branch: String) {
    let (repo, _) = world.worktree();
    let head = off_thread(move || git(&repo, &["rev-parse", &format!("refs/heads/{branch}")])).await;
    assert_eq!(Some(head), world.commit);
}

#[when(expr = "I try to remove the main folder of {string} as a worktree")]
async fn remove_main(world: &mut CoreWorld, name: String) {
    let repo = world.repo(&name);
    let from = repo.clone();
    let result = off_thread(move || worktrees::remove(&from, &from, &[])).await;
    world.record(result);
    assert!(repo.join("README.md").exists());
}

#[when(expr = "I try to remove an unrelated folder as a worktree of {string}")]
async fn remove_unrelated(world: &mut CoreWorld, name: String) {
    let repo = world.repo(&name);
    let stranger = world.dir.path().join("stranger");
    write(&stranger, "mine.txt", "not a worktree\n");
    let target = stranger.clone();
    let result = off_thread(move || worktrees::remove(&repo, &target, &[])).await;
    world.record(result);
    assert!(stranger.join("mine.txt").exists());
}

#[given(expr = "another worktree {string} with uncommitted work inside it")]
async fn nested_worktree(world: &mut CoreWorld, branch: String) {
    let (repo, outer) = world.worktree();
    let inner = off_thread(move || {
        worktrees::create(&repo, &branch, None, Some(&outer.join("nested")), false).expect("inner worktree")
    })
    .await;
    write(&inner, "precious.txt", "uncommitted\n");
    world.inner_worktree = Some(inner);
}

#[then(expr = "the uncommitted work in {string} is still there")]
fn inner_work_kept(world: &mut CoreWorld, _branch: String) {
    let inner = world.inner_worktree.clone().expect("inner worktree");
    assert!(inner.join("precious.txt").exists());
}

#[given(expr = "a repository {string} with a remote and a submodule {string}")]
async fn repo_with_submodule(world: &mut CoreWorld, name: String, sub: String) {
    let repo = world.new_repo_with_remote(&name);
    let sub_origin = world.dir.path().join(format!("{sub}-origin"));
    off_thread(move || {
        init_repo(&sub_origin);
        commit_file(&sub_origin, "s.txt", "submodule file\n");
        let url = sub_origin.to_str().expect("utf-8 path");
        git(&repo, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", "--", url, &sub]);
        git(&repo, &["commit", "-q", "-m", "add submodule"]);
    })
    .await;
}

#[given(expr = "a worktree of {string} on the new branch {string} with its submodules initialised")]
async fn worktree_with_submodules(world: &mut CoreWorld, name: String, branch: String) {
    let repo = world.repo(&name);
    let from = repo.clone();
    let path = off_thread(move || {
        let path = worktrees::create(&from, &branch, None, None, false).expect("create worktree");
        git(&path, &["-c", "protocol.file.allow=always", "submodule", "update", "-q", "--init"]);
        path
    })
    .await;
    world.worktree = Some((repo, path));
}

#[given(expr = "the submodule {string} in the worktree has a commit that is not pushed")]
async fn unpushed_submodule_commit(world: &mut CoreWorld, sub: String) {
    let folder = world.worktree().1.join(sub);
    off_thread(move || {
        let identity = ["-c", "user.name=BDD", "-c", "user.email=bdd@example.com", "-c", "commit.gpgsign=false"];
        git(&folder, &[&identity[..], &["commit", "-q", "--allow-empty", "-m", "unpushed"]].concat());
    })
    .await;
}

#[given(expr = "the submodule folder {string} in the worktree is emptied")]
fn submodule_emptied(world: &mut CoreWorld, sub: String) {
    let folder = world.worktree().1.join(sub);
    fs::remove_dir_all(&folder).expect("empty the submodule folder");
    fs::create_dir(&folder).expect("recreate the submodule folder");
}

#[then(expr = "the worktree's git data for submodule {string} is still there")]
async fn submodule_data_kept(world: &mut CoreWorld, sub: String) {
    let path = world.worktree().1;
    let git_dir = off_thread(move || git(&path, &["rev-parse", "--path-format=absolute", "--git-dir"])).await;
    let modules = PathBuf::from(git_dir).join("modules").join(sub);
    assert!(modules.is_dir(), "{} is gone", modules.display());
}

#[given(expr = "a folder outside the worktree holding {string}")]
fn outside_folder(world: &mut CoreWorld, file: String) {
    let outside = world.dir.path().join("shared-store");
    write(&outside, &file, "keep me\n");
    world.outside = Some(outside);
}

#[given(expr = "a junction at {string} in the worktree pointing to that folder")]
async fn junction_in_worktree(world: &mut CoreWorld, at: String) {
    let target = world.outside.clone().expect("outside folder");
    let link = world.worktree().1.join(at);
    off_thread(move || junction(&link, &target)).await;
}

#[then(expr = "the folder outside still holds {string}")]
fn outside_kept(world: &mut CoreWorld, file: String) {
    let outside = world.outside.clone().expect("outside folder");
    assert!(outside.join(&file).exists(), "{file} was deleted through the junction");
}
