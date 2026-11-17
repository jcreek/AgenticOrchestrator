use std::fs;
use std::path::{Path, PathBuf};

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use orchestrator_core::{files, git};

use crate::{CoreWorld, commit_file, junction, off_thread, read, write};

fn numbered(prefix: &str, count: u32) -> String {
    (1..=count).map(|n| format!("{prefix} {n}\n")).collect()
}

/// The repository the scenario is working in; scenarios using these steps have one.
fn only_repo(world: &CoreWorld) -> PathBuf {
    assert_eq!(world.repos.len(), 1, "these steps expect exactly one repository");
    world.repos.values().next().expect("repository").clone()
}

#[given(expr = "a repository {string} with {string} of {int} lines and {string} of {int} lines committed")]
async fn repo_with_files(world: &mut CoreWorld, name: String, first: String, first_lines: u32, second: String, second_lines: u32) {
    let repo = world.new_repo(&name);
    off_thread(move || {
        commit_file(&repo, &first, &numbered("line", first_lines));
        commit_file(&repo, &second, &numbered("line", second_lines));
    })
    .await;
}

#[when(expr = "{int} lines are added to {string} and {int} is removed")]
fn lines_added_and_removed(world: &mut CoreWorld, added: u32, file: String, removed: u32) {
    let repo = only_repo(world);
    let kept: String = read(&repo.join(&file)).lines().skip(removed as usize).map(|line| format!("{line}\n")).collect();
    write(&repo, &file, &(kept + &numbered("added", added)));
}

#[when(expr = "{string} is deleted")]
fn deleted(world: &mut CoreWorld, file: String) {
    fs::remove_file(only_repo(world).join(file)).expect("delete file");
}

#[when(expr = "{string} is created with {int} lines and staged")]
async fn created_and_staged(world: &mut CoreWorld, file: String, lines: u32) {
    let repo = only_repo(world);
    write(&repo, &file, &numbered("new", lines));
    off_thread(move || git::stage(&repo, &[file]).expect("stage")).await;
}

#[when(expr = "{string} is created and not staged")]
fn created_not_staged(world: &mut CoreWorld, file: String) {
    write(&only_repo(world), &file, "scratch\n");
}

#[then(expr = "the changes list of {string} is:")]
async fn changes_list(world: &mut CoreWorld, step: &Step, name: String) {
    let repo = world.repo(&name);
    let mut found: Vec<Vec<String>> = off_thread(move || git::status(&repo).expect("status"))
        .await
        .into_iter()
        .map(|file| {
            let count = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
            vec![file.path, format!("{:?}", file.kind), count(file.added), count(file.removed)]
        })
        .collect();
    found.sort();
    let expected: Vec<Vec<String>> = step.table.as_ref().expect("table").rows.iter().skip(1).cloned().collect();
    assert_eq!(found, expected);
}

#[given(expr = "a repository {string} where {string} reads {string}")]
async fn repo_where_file_reads(world: &mut CoreWorld, name: String, file: String, text: String) {
    let repo = world.new_repo(&name);
    off_thread(move || commit_file(&repo, &file, &text)).await;
}

#[given(expr = "on a new branch {string} {string} is committed as {string}")]
async fn committed_on_branch(world: &mut CoreWorld, branch: String, file: String, text: String) {
    let repo = only_repo(world);
    off_thread(move || {
        crate::git(&repo, &["checkout", "-q", "-b", &branch]);
        commit_file(&repo, &file, &text);
    })
    .await;
}

#[given(expr = "{string} has since moved on with {string} reading {string}")]
async fn base_moved_on(world: &mut CoreWorld, base: String, file: String, text: String) {
    let repo = only_repo(world);
    off_thread(move || {
        crate::git(&repo, &["checkout", "-q", &base]);
        commit_file(&repo, &file, &text);
        crate::git(&repo, &["checkout", "-q", "-"]);
    })
    .await;
}

#[when(expr = "{string} is edited to {string} on {string}")]
async fn edited_on_branch(world: &mut CoreWorld, file: String, text: String, branch: String) {
    let repo = only_repo(world);
    write(&repo, &file, &text);
    let current = off_thread(move || git::current_branch(&repo).expect("branch")).await;
    assert_eq!(current, Some(branch));
}

#[then(expr = "against the last commit {string} was {string}")]
async fn at_last_commit(world: &mut CoreWorld, file: String, text: String) {
    let repo = only_repo(world);
    let found = off_thread(move || git::file_at_head(&repo, &file).expect("read")).await;
    assert_eq!(found, Some(text));
}

#[then(expr = "against the branch point with {string} {string} was {string}")]
async fn at_branch_point(world: &mut CoreWorld, base: String, file: String, text: String) {
    let repo = only_repo(world);
    let found = off_thread(move || git::file_at_merge_base(&repo, &file, &base).expect("read")).await;
    assert_eq!(found, Some(text));
}

#[given(expr = "I opened {string} in {string}")]
fn opened(world: &mut CoreWorld, file: String, name: String) {
    world.opened_version = Some(files::read_file(&world.repo(&name), &file).expect("read").version);
}

#[given(expr = "something else changes {string} to {string}")]
fn changed_elsewhere(world: &mut CoreWorld, file: String, text: String) {
    write(&only_repo(world), &file, &text);
}

#[when(expr = "I save {string} as {string}")]
fn save_as(world: &mut CoreWorld, file: String, text: String) {
    let result = files::write_file(&only_repo(world), &file, &text, world.opened_version.as_deref());
    world.record(result);
}

#[when(expr = "I choose to keep my version {string} of {string}")]
fn keep_mine(world: &mut CoreWorld, text: String, file: String) {
    let repo = only_repo(world);
    let current = files::file_version(&repo, &file).expect("version");
    let result = files::write_file(&repo, &file, &text, current.as_deref());
    world.record(result);
}

#[then(expr = "{string} in {string} reads {string}")]
fn file_reads(world: &mut CoreWorld, file: String, name: String, text: String) {
    assert_eq!(read(&world.repo(&name).join(file)), text);
}

#[then("the save is refused because the file changed on disk")]
fn refused_conflict(world: &mut CoreWorld) {
    world.assert_refused("Conflict");
}

#[given(expr = "a repository {string} with a junction {string} to a folder outside it")]
async fn repo_with_junction(world: &mut CoreWorld, name: String, link: String) {
    let repo = world.new_repo(&name);
    let outside = world.dir.path().join("outside-target");
    fs::create_dir_all(&outside).expect("create outside folder");
    world.outside = Some(outside.clone());
    off_thread(move || junction(&repo.join(link), &outside)).await;
}

#[when(expr = "I try to save {string} in {string}")]
fn try_save(world: &mut CoreWorld, path: String, name: String) {
    let result = files::write_file(&world.repo(&name), &path, "payload", None);
    world.record(result);
}

#[then(regex = r"^the save is refused because the path is (outside the workspace|inside the git directory)$")]
fn refused_path(world: &mut CoreWorld, reason: String) {
    world.assert_refused(if reason == "outside the workspace" { "OutsideWorkspace" } else { "GitDirectory" });
}

#[then(expr = "nothing was written outside {string} or inside its git directory")]
fn nothing_written(world: &mut CoreWorld, name: String) {
    let repo = world.repo(&name);
    let outside = world.outside.clone().expect("outside folder");
    assert_eq!(fs::read_dir(&outside).expect("list outside folder").count(), 0, "a file appeared outside");
    assert!(!world.dir.path().join("outside.txt").exists());
    assert!(!Path::new(r"C:\Windows\evil.txt").exists());
    let git_dir = repo.join(".git");
    assert!(!git_dir.join("hooks").join("pre-commit").exists());
    assert!(!read(&git_dir.join("config")).contains("payload"));
}

#[given(expr = "{string} and {string} are changed in {string}")]
fn two_changed(world: &mut CoreWorld, first: String, second: String, name: String) {
    let repo = world.repo(&name);
    write(&repo, &first, "first change\n");
    write(&repo, &second, "second change\n");
}

async fn stage(world: &mut CoreWorld, file: String, name: &str) {
    let repo = world.repo(name);
    off_thread(move || git::stage(&repo, &[file]).expect("stage")).await;
}

#[given(expr = "I stage {string} in {string}")]
async fn given_staged(world: &mut CoreWorld, file: String, name: String) {
    stage(world, file, &name).await;
}

#[when(expr = "I stage {string} in {string}")]
async fn when_staged(world: &mut CoreWorld, file: String, name: String) {
    stage(world, file, &name).await;
}

#[when(expr = "I commit {string} with the message {string}")]
async fn commit(world: &mut CoreWorld, name: String, message: String) {
    let repo = world.repo(&name);
    let result = off_thread(move || git::commit(&repo, &message)).await;
    world.record(result);
}

#[then(expr = "the last commit of {string} is {string} and changes only {string}")]
async fn last_commit(world: &mut CoreWorld, name: String, message: String, file: String) {
    assert_eq!(world.refusal, None);
    let repo = world.repo(&name);
    let (subject, files) = off_thread(move || {
        (crate::git(&repo, &["log", "-1", "--format=%s"]), crate::git(&repo, &["show", "--name-only", "--format=", "HEAD"]))
    })
    .await;
    assert_eq!((subject, files), (message, file));
}

#[then(expr = "{string} is still uncommitted in {string}")]
async fn still_uncommitted(world: &mut CoreWorld, file: String, name: String) {
    let repo = world.repo(&name);
    let changes = off_thread(move || git::status(&repo).expect("status")).await;
    assert!(changes.iter().any(|change| change.path == file), "{file} is not among the changes");
}

#[when(expr = "I push {string}")]
async fn push(world: &mut CoreWorld, name: String) {
    let repo = world.repo(&name);
    off_thread(move || git::push(&repo).expect("push")).await;
}

#[then(expr = "the remote has {string} on {string}")]
async fn remote_has(world: &mut CoreWorld, message: String, branch: String) {
    let origin = world.dir.path().join(format!("{}-origin.git", world.repos.keys().next().expect("repo")));
    let found = off_thread(move || crate::git(&origin, &["log", "-1", "--format=%s", &branch])).await;
    assert_eq!(found, message);
}

#[then("the commit is refused because the message is empty")]
fn refused_empty_message(world: &mut CoreWorld) {
    world.assert_refused("EmptyMessage");
}
