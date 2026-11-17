use std::fs;
use std::io::ErrorKind;
use std::panic;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::git::{self, ChangedFile, FileKind, GitError, Keep, OUTPUT_CAP, git, path_arg, text};
use crate::terminal;

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("'{0}' is not a valid branch name")]
    InvalidBranch(String),
    #[error("the repository folder has no name")]
    UnnamedRepoFolder,
    #[error("{} already exists", .0.display())]
    TargetExists(PathBuf),
    #[error("the worktree root must be an absolute path")]
    RelativeRoot,
    #[error("not a worktree of this repository")]
    UnknownWorktree,
    #[error("the main worktree cannot be removed")]
    MainWorktree,
    #[error("the worktree has work that would be lost")]
    Blocked(Vec<Blocker>),
    #[error("another worktree is inside this one; remove that first")]
    ContainsWorktree,
    #[error("worktrees with initialised submodules must be removed by hand")]
    HasSubmodules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BlockerKind {
    ModifiedFiles,
    UntrackedFiles,
    IgnoredFiles,
    UnpushedCommits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    pub kind: BlockerKind,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    pub success: bool,
    pub output: String,
}

pub fn validate_branch_name(repo: &Path, name: &str) -> Result<(), WorktreeError> {
    let invalid = || WorktreeError::InvalidBranch(name.to_owned());
    if name.is_empty() || name.starts_with('-') {
        return Err(invalid());
    }
    // --branch expands shorthands such as @{-1}, so the name must come back unchanged.
    match git(repo, &["check-ref-format", "--branch", name]) {
        Ok(output) if text(&output) == name => Ok(()),
        Ok(_) | Err(GitError::Failed(_)) => Err(invalid()),
        Err(error) => Err(error.into()),
    }
}

pub fn create(
    repo_root: &Path,
    branch: &str,
    base: Option<&str>,
    worktree_root: Option<&Path>,
    include_file_copy: bool,
) -> Result<PathBuf, WorktreeError> {
    validate_branch_name(repo_root, branch)?;
    let root = match worktree_root {
        Some(root) => root.to_path_buf(),
        None => default_worktree_root(&main_worktree(repo_root)?)?,
    };
    if !root.is_absolute() {
        return Err(WorktreeError::RelativeRoot);
    }
    fs::create_dir_all(&root)?;
    let target = root.join(path_safe(branch));
    if fs::symlink_metadata(&target).is_ok() {
        return Err(WorktreeError::TargetExists(target));
    }

    let target_arg = path_arg(&target)?;
    let reference = format!("refs/heads/{branch}");
    let branch_exists = git(repo_root, &["rev-parse", "--verify", "--quiet", &reference]).is_ok();
    if branch_exists {
        git(repo_root, &["worktree", "add", "-q", "--", target_arg, branch])?;
    } else {
        let mut args = vec!["worktree", "add", "-q", "--no-track", "-b", branch, "--", target_arg];
        args.extend(base);
        git(repo_root, &args)?;
    }

    git(&target, &["config", "core.longpaths", "true"])?;
    if include_file_copy {
        copy_included_files(repo_root, &target)?;
    }
    Ok(target)
}

fn default_worktree_root(repo_root: &Path) -> Result<PathBuf, WorktreeError> {
    let mut name = repo_root
        .file_name()
        .ok_or(WorktreeError::UnnamedRepoFolder)?
        .to_os_string();
    name.push(".worktrees");
    Ok(repo_root.with_file_name(name))
}

/// git lists the main worktree first, also when asked from a linked one.
fn main_worktree(repo: &Path) -> Result<PathBuf, WorktreeError> {
    let first = list_worktrees(repo)?.into_iter().next();
    first.map(PathBuf::from).ok_or(WorktreeError::Git(GitError::UnexpectedOutput))
}

fn path_safe(branch: &str) -> String {
    branch
        .chars()
        .map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '-' })
        .collect()
}

fn copy_included_files(repo_root: &Path, worktree: &Path) -> Result<(), WorktreeError> {
    let list = match fs::read_to_string(repo_root.join(".worktreeinclude")) {
        Ok(list) => list,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let repo = fs::canonicalize(repo_root)?;
    let patterns = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    for source in patterns.flat_map(|pattern| expand(repo_root, pattern)) {
        let Ok(source) = fs::canonicalize(&source) else {
            continue;
        };
        let Ok(relative) = source.strip_prefix(&repo) else {
            continue;
        };
        let destination = worktree.join(relative);
        if !source.is_file() || relative.starts_with(".git") || destination.exists() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination)?;
    }
    Ok(())
}

fn expand(repo_root: &Path, pattern: &str) -> Vec<PathBuf> {
    let pattern = crate::git::normalise_git_path(pattern);
    let (folder, name) = pattern.rsplit_once('/').unwrap_or(("", &pattern));
    let folder = repo_root.join(folder);
    if !name.contains(['*', '?']) {
        return vec![folder.join(name)];
    }
    let Ok(entries) = fs::read_dir(&folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| wildcard_match(name.as_bytes(), entry.file_name().as_encoded_bytes()))
        .map(|entry| entry.path())
        .collect()
}

fn wildcard_match(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let mut last_star = None;
    while n < name.len() {
        if pattern.get(p) == Some(&b'*') {
            last_star = Some((p, n));
            p += 1;
        } else if pattern.get(p).is_some_and(|&want| want == b'?' || want == name[n]) {
            p += 1;
            n += 1;
        } else if let Some((star, matched)) = last_star {
            p = star + 1;
            n = matched + 1;
            last_star = Some((star, n));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

const RUN_COMMAND_DEADLINE: Duration = Duration::from_secs(15 * 60);
pub fn run_command(cwd: &Path, argv: &[String]) -> CommandOutput {
    run_command_with(cwd, argv, crate::environment::environment())
}

fn run_command_with(cwd: &Path, argv: &[String], env: &terminal::Environment) -> CommandOutput {
    let Some((program, args)) = argv.split_first() else {
        return CommandOutput {
            success: true,
            output: String::new(),
        };
    };
    let mut command = git::hidden_command(terminal::resolve_program(program, env));
    command.args(args).current_dir(cwd).env_clear().envs(env);
    match git::output_within(&mut command, None, Some(RUN_COMMAND_DEADLINE), Keep::Tail(OUTPUT_CAP)) {
        Ok(output) => {
            let mut bytes = output.stdout;
            bytes.extend(output.stderr);
            bytes.drain(..bytes.len().saturating_sub(OUTPUT_CAP));
            CommandOutput {
                success: output.status.success(),
                output: String::from_utf8_lossy(&bytes).into_owned(),
            }
        }
        Err(error) if error.kind() == ErrorKind::TimedOut => CommandOutput {
            success: false,
            output: "the command did not finish within 15 minutes and was stopped".to_owned(),
        },
        Err(error) => CommandOutput {
            success: false,
            output: error.to_string(),
        },
    }
}

pub fn removal_blockers(worktree_path: &Path) -> Result<Vec<Blocker>, WorktreeError> {
    let Inspection { index, status, ignored } = inspect(worktree_path);
    blockers(worktree_path, &index?, status?, &ignored?)
}

struct Inspection {
    index: Result<IndexScan, WorktreeError>,
    status: Result<(Vec<ChangedFile>, Option<String>, bool), GitError>,
    ignored: Result<Vec<u8>, GitError>,
}

/// Runs the independent git calls behind the removal checks in parallel; each costs a process start.
fn inspect(worktree: &Path) -> Inspection {
    let ignored_args = ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"];
    thread::scope(|scope| {
        let index = scope.spawn(|| scan_index(worktree));
        let ignored = scope.spawn(|| git(worktree, &ignored_args));
        let status = git::status_with_branch(worktree);
        Inspection { index: joined(index), status, ignored: joined(ignored) }
    })
}

fn joined<T>(handle: thread::ScopedJoinHandle<'_, T>) -> T {
    handle.join().unwrap_or_else(|panic| panic::resume_unwind(panic))
}

fn blockers(
    worktree_path: &Path,
    index: &IndexScan,
    (changes, branch, _): (Vec<ChangedFile>, Option<String>, bool),
    ignored: &[u8],
) -> Result<Vec<Blocker>, WorktreeError> {
    let untracked = changes
        .iter()
        .filter(|change| change.kind == FileKind::Untracked)
        .count();
    let modified = changes.len() - untracked + index.hidden_edits;
    let ignored = ignored
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .count();
    // On a branch the commits survive removal because the branch is kept.
    let unpushed = if branch.is_some() {
        0
    } else {
        text(&git(worktree_path, &["rev-list", "--count", "HEAD", "--not", "--remotes"])?)
            .parse()
            .map_err(|_| GitError::UnexpectedOutput)?
    };

    Ok([
        (BlockerKind::ModifiedFiles, modified),
        (BlockerKind::UntrackedFiles, untracked),
        (BlockerKind::IgnoredFiles, ignored),
        (BlockerKind::UnpushedCommits, unpushed),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(kind, count)| Blocker { kind, count })
    .collect())
}

struct IndexScan {
    /// Assume-unchanged and skip-worktree files whose content differs from the index; `git status` hides them.
    hidden_edits: usize,
    /// A submodule that is checked out, or merely has files in its folder, or is in conflict, or a
    /// submodule git dir (which can hold unpushed commits) in the worktree's git dir.
    has_submodules: bool,
}

/// The git dir a linked worktree's `.git` file points to.
fn worktree_git_dir(worktree: &Path) -> PathBuf {
    let dot_git = worktree.join(".git");
    fs::read_to_string(&dot_git)
        .ok()
        .and_then(|text| text.strip_prefix("gitdir:").map(|dir| worktree.join(dir.trim())))
        .unwrap_or(dot_git)
}

fn scan_index(worktree: &Path) -> Result<IndexScan, WorktreeError> {
    let raw = git(worktree, &["ls-files", "-v", "-s", "-z"])?;
    let raw = String::from_utf8_lossy(&raw);
    let entries: Vec<([&str; 4], &str)> = raw
        .split('\0')
        .filter_map(|record| record.split_once('\t'))
        .filter_map(|(meta, path)| Some((meta.split(' ').collect::<Vec<_>>().try_into().ok()?, path)))
        .collect();
    let has_submodules = worktree_git_dir(worktree).join("modules").is_dir()
        || entries.iter().any(|([_, mode, _, stage], path)| {
            *mode == "160000"
                && (*stage != "0" || fs::read_dir(worktree.join(path)).is_ok_and(|mut found| found.next().is_some()))
        });
    let flagged: Vec<(&str, &str)> = entries
        .iter()
        .filter(|([tag, ..], _)| *tag == "S" || tag.bytes().all(|c| c.is_ascii_lowercase()))
        .map(|([_, _, oid, _], path)| (*oid, *path))
        .filter(|(_, path)| worktree.join(path).is_file())
        .collect();
    if flagged.is_empty() {
        return Ok(IndexScan { hidden_edits: 0, has_submodules });
    }
    let paths: String = flagged.iter().map(|(_, path)| format!("{path}\n")).collect();
    let hashes = git::git_with_input(worktree, &["hash-object", "--stdin-paths"], paths.as_bytes())?;
    let hidden_edits = String::from_utf8_lossy(&hashes)
        .lines()
        .zip(&flagged)
        .filter(|(hash, (oid, _))| hash != oid)
        .count();
    Ok(IndexScan { hidden_edits, has_submodules })
}

/// Every reason [`remove`] would refuse, checked without changing anything.
pub fn check_removable(repo_root: &Path, worktree_path: &Path, accepted: &[Blocker]) -> Result<(), WorktreeError> {
    removable(repo_root, worktree_path, accepted).map(drop)
}

pub fn remove(repo_root: &Path, worktree_path: &Path, accepted: &[Blocker]) -> Result<(), WorktreeError> {
    let (target, listed_path, blockers) = removable(repo_root, worktree_path, accepted)?;
    // git follows directory junctions while deleting and would empty their targets.
    unlink_directory_links(&target)?;
    let mut args = vec!["worktree", "remove"];
    if !blockers.is_empty() {
        args.push("--force");
    }
    args.extend(["--", listed_path.as_str()]);
    git(repo_root, &args)?;
    Ok(())
}

/// The canonical worktree path, the path git lists it under, and its blockers.
fn removable(
    repo_root: &Path,
    worktree_path: &Path,
    accepted: &[Blocker],
) -> Result<(PathBuf, String, Vec<Blocker>), WorktreeError> {
    let target = fs::canonicalize(worktree_path).map_err(|_| WorktreeError::UnknownWorktree)?;
    let listed: Vec<(String, Option<PathBuf>)> = list_worktrees(repo_root)?
        .into_iter()
        .map(|path| {
            let canonical = fs::canonicalize(&path).ok();
            (path, canonical)
        })
        .collect();
    let index = listed
        .iter()
        .position(|(_, canonical)| canonical.as_ref() == Some(&target))
        .ok_or(WorktreeError::UnknownWorktree)?;
    if index == 0 {
        return Err(WorktreeError::MainWorktree);
    }
    let nested = listed.iter().enumerate().any(|(other, (_, canonical))| {
        other != index && canonical.as_ref().is_some_and(|path| path.starts_with(&target))
    });
    if nested {
        return Err(WorktreeError::ContainsWorktree);
    }
    let listed_path = listed[index].0.clone();
    let Inspection { index: scan, status, ignored } = inspect(Path::new(&listed_path));
    let scan = scan?;
    if scan.has_submodules {
        return Err(WorktreeError::HasSubmodules);
    }

    let blockers = blockers(Path::new(&listed_path), &scan, status?, &ignored?)?;
    if !blockers.is_empty() && blockers != accepted {
        return Err(WorktreeError::Blocked(blockers));
    }
    Ok((target, listed_path, blockers))
}

fn unlink_directory_links(folder: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() && entry.path().is_dir() {
            #[cfg(windows)]
            fs::remove_dir(entry.path())?;
            #[cfg(unix)]
            fs::remove_file(entry.path())?;
        } else if kind.is_dir() {
            unlink_directory_links(&entry.path())?;
        }
    }
    Ok(())
}

fn list_worktrees(repo_root: &Path) -> Result<Vec<String>, WorktreeError> {
    let raw = git(repo_root, &["worktree", "list", "--porcelain", "-z"])?;
    Ok(String::from_utf8_lossy(&raw)
        .split('\0')
        .filter_map(|field| field.strip_prefix("worktree "))
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::current_branch;
    use crate::git::test_support::{add_origin, commit_file, init_repo, sh};
    use tempfile::{TempDir, tempdir};

    struct Fixture {
        dir: TempDir,
        repo: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        init_repo(&repo);
        commit_file(&repo, ".gitignore", "*.log\n");
        add_origin(&repo, &dir.path().join("origin.git"));
        sh(&repo, &["push", "-q", "-u", "origin", "main"]);
        Fixture { dir, repo }
    }

    fn same_path(left: &Path, right: &Path) -> bool {
        fs::canonicalize(left).expect("left exists") == fs::canonicalize(right).expect("right exists")
    }

    #[test]
    fn creates_new_branch_at_default_path() {
        let f = fixture();
        let path = create(&f.repo, "feature/x", None, None, false).expect("create");
        assert!(same_path(&path, &f.dir.path().join("repo.worktrees").join("feature-x")));
        assert_eq!(current_branch(&path).expect("branch"), Some("feature/x".to_owned()));
        assert_eq!(sh(&path, &["config", "core.longpaths"]), "true");
        assert!(removal_blockers(&path).expect("blockers").is_empty());
    }

    #[test]
    fn default_path_from_a_linked_worktree_sits_beside_the_main_repo() {
        let f = fixture();
        let linked = create(&f.repo, "first", None, None, false).expect("first");
        let path = create(&linked, "second", None, None, false).expect("second");
        assert!(same_path(&path, &f.dir.path().join("repo.worktrees").join("second")), "{}", path.display());
    }

    #[test]
    fn checks_out_an_existing_branch() {
        let f = fixture();
        commit_file(&f.repo, "a.txt", "a");
        sh(&f.repo, &["branch", "existing", "HEAD~1"]);
        let path = create(&f.repo, "existing", Some("main"), None, false).expect("create");
        assert_eq!(current_branch(&path).expect("branch"), Some("existing".to_owned()));
        assert_eq!(sh(&path, &["rev-parse", "HEAD"]), sh(&f.repo, &["rev-parse", "main~1"]));
    }

    #[test]
    fn creates_from_a_base_under_a_custom_root() {
        let f = fixture();
        let first = sh(&f.repo, &["rev-parse", "HEAD"]);
        commit_file(&f.repo, "a.txt", "a");
        let root = f.dir.path().join("custom root");
        let path = create(&f.repo, "from-base", Some(&first), Some(&root), false).expect("create");
        assert!(same_path(path.parent().expect("parent"), &root));
        assert_eq!(sh(&path, &["rev-parse", "HEAD"]), first);
    }

    #[test]
    fn rejects_hostile_branch_names_and_existing_targets() {
        let f = fixture();
        for name in ["-x", "--force", "../../evil", "has space", "@{-1}", ""] {
            assert!(
                matches!(create(&f.repo, name, None, None, false), Err(WorktreeError::InvalidBranch(_))),
                "{name} accepted"
            );
        }
        assert!(!f.dir.path().join("evil").exists());
        assert_eq!(sh(&f.repo, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1);

        fs::create_dir_all(f.dir.path().join("repo.worktrees").join("taken")).expect("mkdir");
        assert!(matches!(
            create(&f.repo, "taken", None, None, false),
            Err(WorktreeError::TargetExists(_))
        ));
    }

    #[test]
    fn copies_worktreeinclude_files_without_escaping_the_repo() {
        let f = fixture();
        fs::write(f.dir.path().join("outside.txt"), "secret").expect("write");
        fs::create_dir_all(f.repo.join("config")).expect("mkdir");
        fs::write(f.repo.join(".env.log"), "KEY=1").expect("write");
        fs::write(f.repo.join("config").join("dev.json"), "{}").expect("write");
        fs::write(f.repo.join("config").join("prod.json"), "{}").expect("write");
        fs::write(f.repo.join("config").join("notes.md"), "x").expect("write");
        fs::write(
            f.repo.join(".worktreeinclude"),
            "# comment\n\n.env.log\nconfig/*.json\n../outside.txt\nmissing.txt\n.git/config\n",
        )
        .expect("write");

        let path = create(&f.repo, "with-files", None, None, true).expect("create");
        assert_eq!(fs::read_to_string(path.join(".env.log")).expect("copied"), "KEY=1");
        assert!(path.join("config").join("dev.json").exists());
        assert!(path.join("config").join("prod.json").exists());
        assert!(!path.join("config").join("notes.md").exists());
        assert!(!path.join("outside.txt").exists());
        assert!(!f.dir.path().join("repo.worktrees").join("outside.txt").exists());
        assert!(!path.join("missing.txt").exists());
        assert!(path.join(".git").is_file(), ".git/config entry turned the gitdir pointer into a folder");

        let skipped = create(&f.repo, "without-files", None, None, false).expect("create");
        assert!(!skipped.join(".env.log").exists());
    }

    #[test]
    fn wildcards_match_names() {
        assert!(wildcard_match(b"*.json", b"dev.json"));
        assert!(wildcard_match(b"a?c*", b"abcdef"));
        assert!(!wildcard_match(b"*.json", b"dev.jsonx"));
        assert!(!wildcard_match(b"a?c", b"ac"));
    }

    #[test]
    fn detects_each_blocker_and_refuses_unconfirmed_removal() {
        let f = fixture();
        commit_file(&f.repo, "tracked.txt", "a");
        sh(&f.repo, &["push", "-q"]);
        let path = create(&f.repo, "dirty", None, None, false).expect("create");
        fs::write(path.join("tracked.txt"), "changed").expect("write");
        fs::write(path.join("new.txt"), "new").expect("write");
        fs::write(path.join("build.log"), "log").expect("write");
        commit_file(&path, "committed.txt", "c");
        sh(&path, &["checkout", "-q", "--detach"]);

        let expected = vec![
            Blocker { kind: BlockerKind::ModifiedFiles, count: 1 },
            Blocker { kind: BlockerKind::UntrackedFiles, count: 1 },
            Blocker { kind: BlockerKind::IgnoredFiles, count: 1 },
            Blocker { kind: BlockerKind::UnpushedCommits, count: 1 },
        ];
        assert_eq!(removal_blockers(&path).expect("blockers"), expected);
        assert!(matches!(remove(&f.repo, &path, &[]), Err(WorktreeError::Blocked(b)) if b == expected));
        assert!(path.join("new.txt").exists());

        remove(&f.repo, &path, &expected).expect("accepted removal");
        assert!(!path.exists());
        assert_eq!(sh(&f.repo, &["branch", "--list", "dirty"]), "dirty");
    }

    #[test]
    fn removes_a_clean_worktree_and_keeps_its_branch() {
        let f = fixture();
        let path = create(&f.repo, "clean", None, None, false).expect("create");
        commit_file(&path, "unpushed.txt", "kept on the branch");
        remove(&f.repo, &path, &[]).expect("remove");
        assert!(!path.exists());
        assert_eq!(sh(&f.repo, &["branch", "--list", "clean"]), "clean");
    }

    #[test]
    fn refuses_to_remove_main_or_unknown_worktrees() {
        let f = fixture();
        assert!(matches!(check_removable(&f.repo, &f.repo, &[]), Err(WorktreeError::MainWorktree)));
        assert!(matches!(remove(&f.repo, &f.repo, &[]), Err(WorktreeError::MainWorktree)));
        let stranger = f.dir.path().join("stranger");
        fs::create_dir_all(&stranger).expect("mkdir");
        assert!(matches!(check_removable(&f.repo, &stranger, &[]), Err(WorktreeError::UnknownWorktree)));
        assert!(matches!(remove(&f.repo, &stranger, &[]), Err(WorktreeError::UnknownWorktree)));
        assert!(stranger.exists());
        assert!(matches!(
            remove(&f.repo, &f.dir.path().join("missing"), &[]),
            Err(WorktreeError::UnknownWorktree)
        ));
    }

    #[test]
    fn runs_commands_without_a_shell() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(
            run_command(dir.path(), &[]),
            CommandOutput { success: true, output: String::new() }
        );
        let version = run_command(dir.path(), &["git".to_owned(), "--version".to_owned()]);
        assert!(version.success && version.output.contains("git version"));
        let chained = run_command(dir.path(), &["git".to_owned(), "--version && whoami".to_owned()]);
        assert!(!chained.success);
        assert!(!run_command(dir.path(), &["no-such-program-xyz".to_owned()]).success);
    }

    fn utf8(path: &Path) -> &str {
        path.to_str().expect("utf-8 path")
    }

    fn junction(folder: &Path, name: &str, target: &Path) {
        #[cfg(windows)]
        let argv: Vec<String> = ["cmd", "/c", "mklink", "/J", name, utf8(target)].map(str::to_owned).into();
        #[cfg(windows)]
        let made = run_command(folder, &argv);
        #[cfg(windows)]
        assert!(made.success, "{}", made.output);
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, folder.join(name)).unwrap();
    }

    #[test]
    fn new_branch_from_a_remote_base_pushes_to_its_own_name() {
        let f = fixture();
        let path = create(&f.repo, "feat", Some("origin/main"), None, false).expect("create");
        commit_file(&path, "f.txt", "f");
        let pushed = git::push(&path);
        let remote = sh(&f.dir.path().join("origin.git"), &["for-each-ref", "--format=%(refname)"]);
        assert!(pushed.is_ok() && remote.contains("refs/heads/feat"), "push: {pushed:?}; remote: {remote}");
    }

    #[test]
    fn new_branch_from_a_remote_base_never_pushes_onto_that_base() {
        let f = fixture();
        sh(&f.repo, &["config", "push.default", "upstream"]);
        let origin = f.dir.path().join("origin.git");
        let main_before = sh(&origin, &["rev-parse", "refs/heads/main"]);
        let path = create(&f.repo, "feat", Some("origin/main"), None, false).expect("create");
        commit_file(&path, "f.txt", "unreviewed feature work");
        let _ = git::push(&path);
        assert_eq!(sh(&origin, &["rev-parse", "refs/heads/main"]), main_before);
    }

    #[test]
    fn removal_is_refused_when_blockers_differ_from_those_accepted() {
        let f = fixture();
        let path = create(&f.repo, "race", None, None, false).expect("create");
        fs::write(path.join("build.log"), "log").expect("write");
        let seen = removal_blockers(&path).expect("blockers");
        fs::write(path.join("agent-work.txt"), "new work").expect("write");
        let current = removal_blockers(&path).expect("blockers");
        assert!(matches!(check_removable(&f.repo, &path, &seen), Err(WorktreeError::Blocked(b)) if b == current));
        check_removable(&f.repo, &path, &current).expect("accepted blockers are removable");
        assert!(matches!(remove(&f.repo, &path, &seen), Err(WorktreeError::Blocked(b)) if b == current));
        assert!(path.join("agent-work.txt").exists());
    }

    #[test]
    fn edits_hidden_by_assume_unchanged_are_blockers() {
        let f = fixture();
        commit_file(&f.repo, "appsettings.json", "{}");
        let path = create(&f.repo, "local-config", None, None, false).expect("create");
        sh(&path, &["update-index", "--assume-unchanged", "appsettings.json"]);
        assert!(removal_blockers(&path).expect("blockers").is_empty());
        fs::write(path.join("appsettings.json"), "{\"local\":\"only copy\"}").expect("write");
        let expected = vec![Blocker { kind: BlockerKind::ModifiedFiles, count: 1 }];
        assert_eq!(removal_blockers(&path).expect("blockers"), expected);
        assert!(matches!(remove(&f.repo, &path, &[]), Err(WorktreeError::Blocked(_))));
        assert!(path.join("appsettings.json").exists());
    }

    fn add_submodule(f: &Fixture) {
        let sub_origin = f.dir.path().join("sub-origin");
        init_repo(&sub_origin);
        commit_file(&sub_origin, "s.txt", "s");
        sh(&f.repo, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", "--", utf8(&sub_origin), "sub"]);
        sh(&f.repo, &["commit", "-q", "-m", "add sub"]);
    }

    #[test]
    fn an_unpopulated_submodule_is_removable_unless_its_folder_has_files() {
        let f = fixture();
        add_submodule(&f);
        let path = create(&f.repo, "empty-sub", None, None, false).expect("create");
        check_removable(&f.repo, &path, &[]).expect("unpopulated submodule");
        fs::create_dir_all(path.join("sub")).expect("mkdir");
        fs::write(path.join("sub").join("stray.txt"), "not tracked by anyone").expect("write");
        let shown = removal_blockers(&path).expect("blockers");
        assert!(matches!(remove(&f.repo, &path, &shown), Err(WorktreeError::HasSubmodules)));
        assert!(path.join("sub").join("stray.txt").exists());
    }

    #[test]
    fn worktree_with_initialised_submodules_is_never_removed() {
        let f = fixture();
        add_submodule(&f);

        let path = create(&f.repo, "with-sub", None, None, false).expect("create");
        sh(&path, &["-c", "protocol.file.allow=always", "submodule", "update", "-q", "--init"]);
        fs::write(path.join("build.log"), "log").expect("write");
        let shown = removal_blockers(&path).expect("blockers");
        assert!(matches!(check_removable(&f.repo, &path, &shown), Err(WorktreeError::HasSubmodules)));
        assert!(matches!(remove(&f.repo, &path, &shown), Err(WorktreeError::HasSubmodules)));
        assert!(matches!(remove(&f.repo, &path, &[]), Err(WorktreeError::HasSubmodules)));
        assert!(path.join("sub").join("s.txt").exists());
    }

    /// A worktree whose submodule has an unpushed commit, then has its folder emptied.
    fn emptied_submodule_with_unpushed_commit(f: &Fixture, name: &str, drop_mapping: bool) -> PathBuf {
        add_submodule(f);
        let path = create(&f.repo, name, None, None, false).expect("create");
        sh(&path, &["-c", "protocol.file.allow=always", "submodule", "update", "-q", "--init"]);
        let sub = path.join("sub");
        sh(&sub, &["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "unpushed"]);
        if drop_mapping {
            sh(&path, &["config", "-f", ".gitmodules", "--remove-section", "submodule.sub"]);
            sh(&path, &["commit", "-q", "-am", "drop mapping"]);
        }
        fs::remove_dir_all(&sub).expect("empty the submodule folder");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(path.join("build.log"), "log").expect("write");
        path
    }

    #[test]
    fn submodule_git_dir_blocks_removal_when_its_folder_is_empty() {
        for drop_mapping in [false, true] {
            let f = fixture();
            let path = emptied_submodule_with_unpushed_commit(&f, "emptied", drop_mapping);
            let modules = PathBuf::from(sh(&path, &["rev-parse", "--path-format=absolute", "--git-dir"])).join("modules");
            let shown = removal_blockers(&path).expect("blockers");
            assert!(matches!(check_removable(&f.repo, &path, &shown), Err(WorktreeError::HasSubmodules)));
            assert!(matches!(remove(&f.repo, &path, &shown), Err(WorktreeError::HasSubmodules)), "mapping dropped: {drop_mapping}");
            assert!(modules.join("sub").is_dir());
        }
    }

    #[test]
    fn refuses_to_remove_a_worktree_containing_another() {
        let f = fixture();
        let outer = create(&f.repo, "outer", None, None, false).expect("outer");
        let inner = create(&f.repo, "inner", None, Some(&outer.join("nested")), false).expect("inner");
        fs::write(inner.join("precious.txt"), "uncommitted").expect("write");
        let shown = removal_blockers(&outer).expect("blockers");
        assert!(matches!(check_removable(&f.repo, &outer, &shown), Err(WorktreeError::ContainsWorktree)));
        assert!(matches!(remove(&f.repo, &outer, &shown), Err(WorktreeError::ContainsWorktree)));
        assert!(inner.join("precious.txt").exists());
    }

    #[test]
    fn removal_unlinks_junctions_without_emptying_their_targets() {
        let f = fixture();
        let outside = f.dir.path().join("shared");
        fs::create_dir_all(&outside).expect("mkdir");
        fs::write(outside.join("keep.txt"), "keep").expect("write");
        let path = create(&f.repo, "junction", None, None, false).expect("create");
        junction(&path, "link", &outside);
        let shown = removal_blockers(&path).expect("blockers");
        remove(&f.repo, &path, &shown).expect("remove");
        assert!(!path.exists());
        assert!(outside.join("keep.txt").exists());
    }

    #[test]
    fn junction_nested_in_an_ignored_folder_is_unlinked_not_followed() {
        let f = fixture();
        commit_file(&f.repo, ".gitignore", "*.log\nnode_modules/\n");
        let outside = f.dir.path().join("global-store");
        fs::create_dir_all(&outside).expect("mkdir");
        fs::write(outside.join("index.js"), "keep").expect("write");
        let path = create(&f.repo, "pnpm", None, None, false).expect("create");
        let scope = path.join("node_modules").join("@scope");
        fs::create_dir_all(&scope).expect("mkdir");
        junction(&scope, "pkg", &outside);
        let shown = removal_blockers(&path).expect("blockers");
        assert_eq!(shown, vec![Blocker { kind: BlockerKind::IgnoredFiles, count: 1 }]);
        remove(&f.repo, &path, &shown).expect("remove");
        assert!(!path.exists());
        assert!(outside.join("index.js").exists());
    }

    #[test]
    fn rejects_a_relative_worktree_root() {
        let f = fixture();
        let relative = PathBuf::from("relative-root");
        let result = create(&f.repo, "rel", None, Some(&relative), false);
        assert!(matches!(result, Err(WorktreeError::RelativeRoot)), "{result:?}");
        assert!(!relative.exists());
        assert!(!f.repo.join(&relative).exists());
    }

    #[test]
    fn wildcard_match_handles_many_stars_quickly() {
        let started = std::time::Instant::now();
        assert!(!wildcard_match(b"*a*a*a*a*a*a*a*a*a*a*a*a*a*a*b", &[b'a'; 64]));
        assert!(wildcard_match(b"*a*a*a*a*a*a*a*a*a*a*a*a*a*a*b", b"aaaaaaaaaaaaaaaaab"));
        assert!(wildcard_match(b"*", b""));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn run_command_gets_the_same_environment_as_terminals() {
        let dir = tempdir().expect("tempdir");
        let argv: Vec<String> = if cfg!(windows) { vec!["cmd", "/c", "set"] } else { vec!["/usr/bin/env"] }.into_iter().map(str::to_owned).collect();
        let listing = run_command(dir.path(), &argv).output.to_ascii_uppercase();
        assert!(listing.contains("PATH="));
        assert!(listing.contains("TERM=XTERM-256COLOR"));
    }

    #[cfg(windows)]
    #[test]
    fn run_command_finds_a_cmd_shim_on_path() {
        let shims = tempdir().expect("tempdir");
        fs::write(shims.path().join("ao-npm.cmd"), "@echo off\r\necho shim-ran %*\r\n").expect("write");
        let mut env = crate::environment::environment().clone();
        let key = env.keys().find(|name| name.eq_ignore_ascii_case("PATH")).cloned().unwrap_or_else(|| "PATH".into());
        let mut path = shims.path().as_os_str().to_owned();
        path.push(";");
        path.push(env.get(&key).cloned().unwrap_or_default());
        env.insert(key, path);
        let argv: Vec<String> = ["ao-npm", "ci"].map(str::to_owned).into();
        let result = run_command_with(shims.path(), &argv, &env);
        assert!(result.success && result.output.contains("shim-ran ci"), "{result:?}");
    }

    #[test]
    fn run_command_keeps_the_tail_of_large_output() {
        let dir = tempdir().expect("tempdir");
        let mut content = "x".repeat(2 * OUTPUT_CAP);
        content.push_str("THE END");
        fs::write(dir.path().join("big.txt"), &content).expect("write");
        let argv: Vec<String> = if cfg!(windows) { vec!["cmd", "/c", "type", "big.txt"] } else { vec!["/bin/cat", "big.txt"] }.into_iter().map(str::to_owned).collect();
        let result = run_command(dir.path(), &argv);
        assert!(result.success);
        assert_eq!(result.output.len(), OUTPUT_CAP);
        assert!(result.output.ends_with("THE END"));
    }
}
