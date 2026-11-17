use std::collections::HashMap;
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub(crate) const OUTPUT_CAP: usize = 1024 * 1024;
const COMMIT_DEADLINE: Duration = Duration::from_secs(5 * 60);
const PUSH_DEADLINE: Duration = Duration::from_secs(5 * 60);
const REDIRECTING_VARS: [&str; 5] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_NAMESPACE"];

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("could not run git: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Failed(String),
    #[error("unknown revision '{0}'")]
    UnknownRevision(String),
    #[error("file is larger than 5 MB")]
    TooLarge,
    #[error("file is not valid UTF-8 text")]
    NotUtf8,
    #[error("commit message is empty")]
    EmptyMessage,
    #[error("HEAD is detached; check out a branch before pushing")]
    DetachedHead,
    #[error("unexpected git output")]
    UnexpectedOutput,
    #[error("git did not finish in time and was stopped")]
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoGroup {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// `added` and `removed` count lines against HEAD; they are `None` for untracked and binary files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub staged: bool,
    pub unstaged: bool,
    pub kind: FileKind,
    pub original_path: Option<String>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Branches {
    pub current: Option<String>,
    pub local: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BranchFile {
    pub path: String,
    pub kind: FileKind,
    pub original_path: Option<String>,
}

pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Keep {
    All,
    Head(usize),
    Tail(usize),
}

pub(crate) fn output_within(
    command: &mut Command,
    input: Option<&[u8]>,
    deadline: Option<Duration>,
    stdout_keep: Keep,
) -> io::Result<Output> {
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn()?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        let input = input.to_vec();
        thread::spawn(move || stdin.write_all(&input));
    }
    let stdout = child.stdout.take().map(|pipe| thread::spawn(move || drain(pipe, stdout_keep)));
    let stderr = child.stderr.take().map(|pipe| thread::spawn(move || drain(pipe, Keep::Tail(OUTPUT_CAP))));
    let end = deadline.map(|limit| Instant::now() + limit);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if end.is_some_and(|end| Instant::now() >= end) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ErrorKind::TimedOut.into());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let collect = |reader: Option<thread::JoinHandle<Vec<u8>>>| {
        reader.and_then(|reader| reader.join().ok()).unwrap_or_default()
    };
    Ok(Output { status, stdout: collect(stdout), stderr: collect(stderr) })
}

fn drain(mut pipe: impl Read, keep: Keep) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0; 8192];
    while let Ok(read @ 1..) = pipe.read(&mut chunk) {
        kept.extend_from_slice(&chunk[..read]);
        match keep {
            Keep::All => {}
            Keep::Head(cap) if kept.len() >= cap => {
                kept.truncate(cap);
                break;
            }
            Keep::Tail(cap) if kept.len() > 2 * cap => {
                kept.drain(..kept.len() - cap);
            }
            Keep::Head(_) | Keep::Tail(_) => {}
        }
    }
    if let Keep::Tail(cap) = keep {
        kept.drain(..kept.len().saturating_sub(cap));
    }
    kept
}

fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut command = hidden_command("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        // Stops a path such as ":(glob)*" being read as pathspec magic.
        .env("GIT_LITERAL_PATHSPECS", "1");
    for name in REDIRECTING_VARS {
        command.env_remove(name);
    }
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().to_ascii_uppercase().starts_with("GIT_CONFIG") {
            command.env_remove(name);
        }
    }
    command
}

fn run(root: &Path, args: &[&str], input: Option<&[u8]>, deadline: Option<Duration>) -> Result<Output, GitError> {
    output_within(&mut git_command(root, args), input, deadline, Keep::All).map_err(git_error)
}

fn git_error(error: io::Error) -> GitError {
    if error.kind() == ErrorKind::TimedOut {
        GitError::TimedOut
    } else {
        GitError::Io(error)
    }
}

fn check(output: Output, args: &[&str]) -> Result<Vec<u8>, GitError> {
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = text(&output.stderr);
    let message = if !stderr.is_empty() {
        stderr
    } else if !output.stdout.is_empty() {
        text(&output.stdout)
    } else {
        format!("git {} failed", args.first().unwrap_or(&""))
    };
    Err(GitError::Failed(message))
}

pub(crate) fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    check(run(root, args, None, None)?, args)
}

pub(crate) fn git_with_input(root: &Path, args: &[&str], input: &[u8]) -> Result<Vec<u8>, GitError> {
    check(run(root, args, Some(input), None)?, args)
}

fn succeeds(root: &Path, args: &[&str]) -> Result<bool, GitError> {
    Ok(run(root, args, None, None)?.status.success())
}

pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

pub(crate) fn path_arg(path: &Path) -> Result<&str, GitError> {
    path.to_str().ok_or(GitError::NotUtf8)
}

pub fn repo_group(folder: &Path) -> Result<Option<RepoGroup>, GitError> {
    let output = run(
        folder,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        None,
        None,
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    group_of(&fs::canonicalize(text(&output.stdout))?).map(Some)
}

/// What the tab strip shows for a repository folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFacts {
    pub group: RepoGroup,
    pub linked_worktree: bool,
    pub branch: Option<String>,
    pub changes: Vec<ChangedFile>,
}

/// `None` when `folder` is not inside a git repository.
pub fn repo_facts(folder: &Path) -> Result<Option<RepoFacts>, GitError> {
    let args = ["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir", "--show-prefix"];
    let output = run(folder, &args, None, None)?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let (Some(git_dir), Some(common_dir)) = (lines.next(), lines.next()) else {
        return Err(GitError::UnexpectedOutput);
    };
    let common_dir = fs::canonicalize(common_dir)?;
    let (changes, branch) = changes_with_prefix(folder, Some(lines.next().unwrap_or_default()))?;
    Ok(Some(RepoFacts {
        group: group_of(&common_dir)?,
        linked_worktree: fs::canonicalize(git_dir)? != common_dir,
        branch,
        changes,
    }))
}

fn group_of(common_dir: &Path) -> Result<RepoGroup, GitError> {
    let folder_of_repo = if common_dir.file_name().is_some_and(|name| name == ".git") {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    };
    let name = folder_of_repo
        .file_name()
        .map(|name| name.to_string_lossy().trim_end_matches(".git").to_owned())
        .unwrap_or_default();
    Ok(RepoGroup {
        key: common_dir.to_string_lossy().into_owned(),
        name,
    })
}

pub fn current_branch(root: &Path) -> Result<Option<String>, GitError> {
    let output = run(root, &["symbolic-ref", "--quiet", "--short", "HEAD"], None, None)?;
    match output.status.code() {
        Some(0) => Ok(Some(text(&output.stdout))),
        Some(1) => Ok(None),
        _ => check(output, &["symbolic-ref"]).map(|_| None),
    }
}

/// Changes inside `root`, with paths relative to `root` even when it is a sub-folder of the repository.
pub fn status(root: &Path) -> Result<Vec<ChangedFile>, GitError> {
    changes_with_prefix(root, None).map(|(files, _)| files)
}

// The git processes run side by side because each costs hundreds of milliseconds to start on Windows.
fn changes_with_prefix(root: &Path, known_prefix: Option<&str>) -> Result<(Vec<ChangedFile>, Option<String>), GitError> {
    let (status, numstat, prefix) = std::thread::scope(|scope| {
        let numstat = scope.spawn(|| git(root, &["--no-optional-locks", "diff", "--numstat", "-z", "--relative", "HEAD", "--"]));
        let prefix = match known_prefix {
            Some(prefix) => Ok(prefix.to_owned()),
            None => git(root, &["rev-parse", "--show-prefix"]).map(|output| text(&output)),
        };
        (status_with_branch(root), numstat.join().expect("numstat thread panicked"), prefix)
    });
    let (mut files, branch, has_commits) = status?;
    let prefix = prefix?;
    let strip = |path: &str| path.strip_prefix(prefix.as_str()).unwrap_or(path).to_owned();
    for file in &mut files {
        file.path = strip(&file.path);
        file.original_path = file.original_path.as_deref().map(strip);
    }
    if has_commits {
        add_line_counts(&numstat?, &mut files)?;
    }
    Ok((files, branch))
}

/// Changes with repository-relative paths, the current branch, and whether HEAD has a commit.
pub(crate) fn status_with_branch(root: &Path) -> Result<(Vec<ChangedFile>, Option<String>, bool), GitError> {
    // Background polling must not take the index lock the user's own git commands need.
    let raw = git(
        root,
        &["--no-optional-locks", "status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all", "--", "."],
    )?;
    let raw = String::from_utf8_lossy(&raw);
    let (mut branch, mut has_commits) = (None, false);
    for header in raw.split('\0').map_while(|field| field.strip_prefix("# ")) {
        if let Some(head) = header.strip_prefix("branch.head ") {
            branch = Some(head.to_owned());
        } else if let Some(oid) = header.strip_prefix("branch.oid ") {
            has_commits = oid != "(initial)";
        }
    }
    // A branch may itself be named "(detached)", so only symbolic-ref can say.
    if branch.as_deref().is_none_or(|head| head == "(detached)") {
        branch = current_branch(root)?;
    }
    Ok((parse_status(&raw)?, branch, has_commits))
}

pub fn parse_status(raw: &str) -> Result<Vec<ChangedFile>, GitError> {
    let mut fields = raw.split('\0').filter(|field| !field.is_empty());
    let mut files = Vec::new();
    while let Some(record) = fields.next() {
        let file = match record.as_bytes()[0] {
            b'1' => {
                let parts: Vec<&str> = record.splitn(9, ' ').collect();
                let (xy, path) = record_parts(&parts, 9)?;
                tracked_change(xy, path, None)
            }
            b'2' => {
                let parts: Vec<&str> = record.splitn(10, ' ').collect();
                let (xy, path) = record_parts(&parts, 10)?;
                let original = fields.next().ok_or(GitError::UnexpectedOutput)?;
                tracked_change(xy, path, Some(original))
            }
            b'u' => {
                let parts: Vec<&str> = record.splitn(11, ' ').collect();
                let (_, path) = record_parts(&parts, 11)?;
                change(path, false, true, FileKind::Conflicted, None)
            }
            b'?' => {
                let path = record.strip_prefix("? ").ok_or(GitError::UnexpectedOutput)?;
                change(path, false, true, FileKind::Untracked, None)
            }
            b'!' | b'#' => continue,
            _ => return Err(GitError::UnexpectedOutput),
        };
        files.push(file);
    }
    Ok(files)
}

fn record_parts<'a>(parts: &[&'a str], expected: usize) -> Result<(&'a [u8], &'a str), GitError> {
    match parts {
        [_, xy, .., path] if parts.len() == expected && xy.len() == 2 => Ok((xy.as_bytes(), path)),
        _ => Err(GitError::UnexpectedOutput),
    }
}

fn tracked_change(xy: &[u8], path: &str, original: Option<&str>) -> ChangedFile {
    let (index, worktree) = (xy[0], xy[1]);
    let kind = if original.is_some() {
        FileKind::Renamed
    } else if index == b'A' {
        FileKind::Added
    } else if index == b'D' || worktree == b'D' {
        FileKind::Deleted
    } else {
        FileKind::Modified
    };
    change(path, index != b'.', worktree != b'.', kind, original)
}

fn change(path: &str, staged: bool, unstaged: bool, kind: FileKind, original: Option<&str>) -> ChangedFile {
    ChangedFile {
        path: path.replace('\\', "/"),
        staged,
        unstaged,
        kind,
        original_path: original.map(|original| original.replace('\\', "/")),
        added: None,
        removed: None,
    }
}

pub fn file_at_head(root: &Path, path: &str) -> Result<Option<String>, GitError> {
    file_at(root, "HEAD", path)
}

pub fn file_at_merge_base(root: &Path, path: &str, base: &str) -> Result<Option<String>, GitError> {
    file_at(root, &merge_base(root, base)?, path)
}

fn file_at(root: &Path, rev: &str, path: &str) -> Result<Option<String>, GitError> {
    // "./" makes git read the path relative to the folder rather than the repository root.
    let spec = format!("{rev}:./{}", path.replace('\\', "/"));
    let found = run(
        root,
        &["rev-parse", "--verify", "--quiet", "--end-of-options", &spec],
        None,
        None,
    )?;
    if !found.status.success() {
        return Ok(None);
    }
    // --filters applies eol conversion so the text matches what a checkout would write.
    let args = ["cat-file", "--filters", &spec];
    let cap = Keep::Head(MAX_FILE_BYTES as usize + 1);
    let output = output_within(&mut git_command(root, &args), None, None, cap).map_err(git_error)?;
    if output.stdout.len() as u64 > MAX_FILE_BYTES {
        return Err(GitError::TooLarge);
    }
    let bytes = check(output, &args)?;
    String::from_utf8(bytes).map(Some).map_err(|_| GitError::NotUtf8)
}

fn resolve_commit(root: &Path, rev: &str) -> Result<String, GitError> {
    let spec = format!("{rev}^{{commit}}");
    let output = run(
        root,
        &["rev-parse", "--verify", "--quiet", "--end-of-options", &spec],
        None,
        None,
    )?;
    if !output.status.success() {
        return Err(GitError::UnknownRevision(rev.to_owned()));
    }
    Ok(text(&output.stdout))
}

fn merge_base(root: &Path, base: &str) -> Result<String, GitError> {
    let base_commit = resolve_commit(root, base)?;
    Ok(text(&git(root, &["merge-base", "HEAD", &base_commit])?))
}

pub fn default_base_branch(root: &Path) -> Result<Option<String>, GitError> {
    let origin_head = run(
        root,
        &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"],
        None,
        None,
    )?;
    if origin_head.status.success() {
        return Ok(Some(text(&origin_head.stdout)));
    }
    for name in ["main", "master"] {
        let reference = format!("refs/heads/{name}");
        if succeeds(root, &["rev-parse", "--verify", "--quiet", &reference])? {
            return Ok(Some(name.to_owned()));
        }
    }
    Ok(None)
}

pub fn branch_changes(root: &Path, base: &str) -> Result<Vec<BranchFile>, GitError> {
    let base_commit = merge_base(root, base)?;
    let diff = git(
        root,
        &["diff", "--name-status", "-z", "-M", "--relative", &base_commit, "--"],
    )?;
    let mut files = parse_name_status(&String::from_utf8_lossy(&diff))?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    files.extend(
        String::from_utf8_lossy(&untracked)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(|path| BranchFile {
                path: path.to_owned(),
                kind: FileKind::Untracked,
                original_path: None,
            }),
    );
    Ok(files)
}

fn parse_name_status(raw: &str) -> Result<Vec<BranchFile>, GitError> {
    let mut fields = raw.split('\0').filter(|field| !field.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let mut path = fields.next().ok_or(GitError::UnexpectedOutput)?;
        let mut original_path = None;
        let kind = match status.as_bytes()[0] {
            b'A' => FileKind::Added,
            b'D' => FileKind::Deleted,
            b'U' => FileKind::Conflicted,
            b'R' | b'C' => {
                original_path = Some(path.to_owned());
                path = fields.next().ok_or(GitError::UnexpectedOutput)?;
                FileKind::Renamed
            }
            _ => FileKind::Modified,
        };
        files.push(BranchFile {
            path: path.to_owned(),
            kind,
            original_path,
        });
    }
    Ok(files)
}

pub fn stage(root: &Path, paths: &[String]) -> Result<(), GitError> {
    with_paths(root, &["add"], paths)
}

pub fn unstage(root: &Path, paths: &[String]) -> Result<(), GitError> {
    with_paths(root, &["reset", "-q"], paths)
}

fn with_paths(root: &Path, command: &[&str], paths: &[String]) -> Result<(), GitError> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = command.to_vec();
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    git(root, &args).map(drop)
}

pub fn stage_all(root: &Path) -> Result<(), GitError> {
    git(root, &["add", "-A", "--", "."]).map(drop)
}

pub fn commit(root: &Path, message: &str) -> Result<(), GitError> {
    if message.trim().is_empty() {
        return Err(GitError::EmptyMessage);
    }
    let args = ["commit", "-q", "-F", "-"];
    check(run(root, &args, Some(message.as_bytes()), Some(COMMIT_DEADLINE))?, &args).map(drop)
}

pub fn push(root: &Path) -> Result<(), GitError> {
    push_within(root, PUSH_DEADLINE)
}

fn push_within(root: &Path, deadline: Duration) -> Result<(), GitError> {
    let branch = current_branch(root)?.ok_or(GitError::DetachedHead)?;
    let has_upstream = succeeds(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
    )?;
    let with_upstream = ["push", "-q"];
    let without_upstream = ["push", "-q", "-u", "--", "origin", &branch];
    let args: &[&str] = if has_upstream { &with_upstream } else { &without_upstream };
    check(run(root, args, None, Some(deadline))?, args).map(drop)
}

fn add_line_counts(numstat: &[u8], files: &mut [ChangedFile]) -> Result<(), GitError> {
    let counts = parse_numstat(&String::from_utf8_lossy(numstat))?;
    for file in files.iter_mut().filter(|file| file.kind != FileKind::Untracked) {
        if let Some(&(added, removed)) = counts.get(&file.path) {
            file.added = Some(added);
            file.removed = Some(removed);
        }
    }
    Ok(())
}

/// Parses `git diff --numstat -z` into line counts keyed by (new) path. Binary files, which
/// git reports as `-`, are left out.
pub fn parse_numstat(raw: &str) -> Result<HashMap<String, (u32, u32)>, GitError> {
    let mut fields = raw.split('\0');
    let mut counts = HashMap::new();
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(mut path)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(GitError::UnexpectedOutput);
        };
        // A rename leaves the path empty and follows the record with the old and new paths.
        if path.is_empty() {
            fields.next().ok_or(GitError::UnexpectedOutput)?;
            path = fields
                .next()
                .filter(|new| !new.is_empty())
                .ok_or(GitError::UnexpectedOutput)?;
        }
        if let (Ok(added), Ok(removed)) = (added.parse(), removed.parse()) {
            counts.insert(path.replace('\\', "/"), (added, removed));
        }
    }
    Ok(counts)
}

pub fn list_branches(root: &Path) -> Result<Branches, GitError> {
    let raw = git(
        root,
        &["for-each-ref", "--format=%(refname:lstrip=2)", "refs/heads"],
    )?;
    Ok(Branches {
        current: current_branch(root)?,
        local: text(&raw).lines().map(str::to_owned).collect(),
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::git;
    use std::fs;
    use std::path::Path;

    pub fn sh(root: &Path, args: &[&str]) -> String {
        super::text(&git(root, args).expect("git command succeeds"))
    }

    pub fn init_repo(dir: &Path) {
        fs::create_dir_all(dir).expect("create repo dir");
        sh(dir, &["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            sh(dir, &["config", key, value]);
        }
    }

    pub fn commit_file(dir: &Path, path: &str, content: &str) {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(full, content).expect("write file");
        sh(dir, &["add", "--", path]);
        sh(dir, &["commit", "-q", "-m", &format!("change {path}")]);
    }

    pub fn add_origin(dir: &Path, origin: &Path) {
        fs::create_dir_all(origin).expect("create origin dir");
        sh(origin, &["init", "-q", "--bare"]);
        sh(dir, &["remote", "add", "origin", origin.to_str().expect("utf-8 path")]);
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{add_origin, commit_file, init_repo, sh};
    use super::*;
    use tempfile::tempdir;

    fn file(path: &str, staged: bool, unstaged: bool, kind: FileKind, original: Option<&str>) -> ChangedFile {
        ChangedFile {
            path: path.to_owned(),
            staged,
            unstaged,
            kind,
            original_path: original.map(str::to_owned),
            added: None,
            removed: None,
        }
    }

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn counted(mut file: ChangedFile, added: u32, removed: u32) -> ChangedFile {
        file.added = Some(added);
        file.removed = Some(removed);
        file
    }

    #[test]
    fn parses_numstat_records_renames_and_skips_binaries() {
        let raw = concat!(
            "3\t1\tsrc/main.rs\0",
            "-\t-\timage.png\0",
            "0\t0\t\0docs/old name.md\0docs/new name.md\0",
            "12\t0\tdir/ünïcode file.txt\0",
        );
        let counts = parse_numstat(raw).expect("parses");
        assert_eq!(counts.len(), 3);
        assert_eq!(counts["src/main.rs"], (3, 1));
        assert_eq!(counts["docs/new name.md"], (0, 0));
        assert_eq!(counts["dir/ünïcode file.txt"], (12, 0));
        assert!(parse_numstat("3\tonly-two\0").is_err());
        assert!(parse_numstat("1\t1\t\0old-only\0").is_err());
        assert!(parse_numstat("").expect("empty parses").is_empty());
    }

    #[test]
    fn a_sub_folder_sees_its_own_changes_with_paths_relative_to_itself() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "top.txt", "top\n");
        commit_file(dir.path(), "sub/in.txt", "one\n");
        sh(dir.path(), &["mv", "top.txt", "sub/moved.txt"]);
        fs::write(dir.path().join("sub/in.txt"), "one\ntwo\n").expect("write");
        fs::write(dir.path().join("outside.txt"), "x").expect("write");
        fs::write(dir.path().join("sub/new.txt"), "x").expect("write");
        let sub = dir.path().join("sub");

        let mut changes = status(&sub).expect("status");
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            changes,
            vec![
                counted(file("in.txt", false, true, FileKind::Modified, None), 1, 0),
                counted(file("moved.txt", true, false, FileKind::Added, None), 1, 0),
                file("new.txt", false, true, FileKind::Untracked, None),
            ]
        );
        assert_eq!(file_at_head(&sub, "in.txt").expect("read"), Some("one\n".to_owned()));
        let mut branch: Vec<String> = branch_changes(&sub, "main").expect("diff").into_iter().map(|f| f.path).collect();
        branch.sort();
        assert_eq!(branch, paths(&["in.txt", "moved.txt", "new.txt"]));

        unstage(&sub, &paths(&["moved.txt"])).expect("unstage");
        stage_all(&sub).expect("stage all");
        let staged = sh(dir.path(), &["diff", "--cached", "--name-only", "--no-renames"]);
        assert_eq!(staged, "sub/in.txt\nsub/moved.txt\nsub/new.txt\ntop.txt");
    }

    #[test]
    fn lists_local_branches_and_the_current_one() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "a");
        sh(dir.path(), &["branch", "feature/x"]);
        sh(dir.path(), &["tag", "feature/x"]);
        let branches = list_branches(dir.path()).expect("branches");
        assert_eq!(branches.current.as_deref(), Some("main"));
        assert_eq!(branches.local, vec!["feature/x", "main"]);
    }

    #[test]
    fn parses_porcelain_v2_records() {
        let raw = concat!(
            "# branch.oid abc\0",
            "1 M. N... 100644 100644 100644 aaa bbb src/main.rs\0",
            "1 MM N... 100644 100644 100644 aaa bbb both sides.txt\0",
            "1 .D N... 100644 100644 000000 aaa aaa gone.txt\0",
            "1 A. N... 000000 100644 100644 000 bbb new file.rs\0",
            "2 R. N... 100644 100644 100644 aaa aaa R100 docs/new name.md\0docs/old name.md\0",
            "u UU N... 100644 100644 100644 100644 h1 h2 h3 conflict.txt\0",
            "? dir/ünïcode file.txt\0",
            "! ignored.log\0",
        );
        assert_eq!(
            parse_status(raw).expect("parses"),
            vec![
                file("src/main.rs", true, false, FileKind::Modified, None),
                file("both sides.txt", true, true, FileKind::Modified, None),
                file("gone.txt", false, true, FileKind::Deleted, None),
                file("new file.rs", true, false, FileKind::Added, None),
                file("docs/new name.md", true, false, FileKind::Renamed, Some("docs/old name.md")),
                file("conflict.txt", false, true, FileKind::Conflicted, None),
                file("dir/ünïcode file.txt", false, true, FileKind::Untracked, None),
            ]
        );
    }

    #[test]
    fn parse_rejects_malformed_records() {
        assert!(parse_status("1 M. short\0").is_err());
        assert!(parse_status("2 R. N... 100644 100644 100644 a a R100 only-new\0").is_err());
        assert!(parse_status("z what\0").is_err());
        assert_eq!(parse_status("").expect("empty parses"), vec![]);
    }

    #[test]
    fn status_reports_staged_and_unstaged_together() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "one");
        fs::write(dir.path().join("a.txt"), "two").expect("write");
        stage(dir.path(), &paths(&["a.txt"])).expect("stage");
        fs::write(dir.path().join("a.txt"), "three\nfour\n").expect("write");
        fs::write(dir.path().join("new file ü.txt"), "x").expect("write");
        assert_eq!(
            status(dir.path()).expect("status"),
            vec![
                counted(file("a.txt", true, true, FileKind::Modified, None), 2, 1),
                file("new file ü.txt", false, true, FileKind::Untracked, None),
            ]
        );
    }

    #[test]
    fn an_untracked_copy_of_a_file_removed_from_the_index_gets_no_line_counts() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "one\ntwo\n");
        sh(dir.path(), &["rm", "-q", "--cached", "a.txt"]);
        assert_eq!(
            status(dir.path()).expect("status"),
            vec![
                counted(file("a.txt", true, false, FileKind::Deleted, None), 0, 2),
                file("a.txt", false, true, FileKind::Untracked, None),
            ]
        );
    }

    #[test]
    fn status_reports_a_staged_rename() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "old.txt", "same content");
        sh(dir.path(), &["mv", "old.txt", "new.txt"]);
        assert_eq!(
            status(dir.path()).expect("status"),
            vec![counted(file("new.txt", true, false, FileKind::Renamed, Some("old.txt")), 0, 0)]
        );
    }

    #[test]
    fn stage_unstage_commit_round_trip() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        fs::write(dir.path().join("a.txt"), "a").expect("write");
        fs::write(dir.path().join("b.txt"), "b").expect("write");
        stage(dir.path(), &paths(&["a.txt", "b.txt"])).expect("stage in unborn repo");
        unstage(dir.path(), &paths(&["b.txt"])).expect("unstage in unborn repo");
        let staged: Vec<bool> = status(dir.path()).expect("status").iter().map(|f| f.staged).collect();
        assert_eq!(staged, vec![true, false]);

        commit(dir.path(), "first commit").expect("commit");
        assert_eq!(sh(dir.path(), &["log", "-1", "--format=%s"]), "first commit");
        stage_all(dir.path()).expect("stage all");
        unstage(dir.path(), &paths(&["b.txt"])).expect("unstage with HEAD");
        assert!(!status(dir.path()).expect("status")[0].staged);
    }

    #[test]
    fn commit_rejects_an_empty_message_and_reports_nothing_to_commit() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "a");
        assert!(matches!(commit(dir.path(), "  \n"), Err(GitError::EmptyMessage)));
        assert!(matches!(commit(dir.path(), "msg"), Err(GitError::Failed(text)) if !text.is_empty()));
    }

    #[test]
    fn commit_message_starting_with_a_dash_is_kept_verbatim() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        fs::write(dir.path().join("a.txt"), "a").expect("write");
        stage_all(dir.path()).expect("stage");
        commit(dir.path(), "--amend").expect("commit");
        assert_eq!(sh(dir.path(), &["log", "-1", "--format=%s"]), "--amend");
    }

    #[test]
    fn current_branch_handles_unborn_and_detached_heads() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        assert_eq!(current_branch(dir.path()).expect("branch"), Some("main".to_owned()));
        commit_file(dir.path(), "a.txt", "a");
        sh(dir.path(), &["checkout", "-q", "--detach"]);
        assert_eq!(current_branch(dir.path()).expect("branch"), None);
    }

    #[test]
    fn content_at_head_and_merge_base() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "base");
        sh(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit_file(dir.path(), "a.txt", "feature");
        commit_file(dir.path(), "only-feature.txt", "x");
        sh(dir.path(), &["checkout", "-q", "main"]);
        commit_file(dir.path(), "a.txt", "main moved on");
        sh(dir.path(), &["checkout", "-q", "feature"]);

        assert_eq!(file_at_head(dir.path(), "a.txt").expect("read"), Some("feature".to_owned()));
        assert_eq!(
            file_at_merge_base(dir.path(), "a.txt", "main").expect("read"),
            Some("base".to_owned())
        );
        assert_eq!(file_at_merge_base(dir.path(), "only-feature.txt", "main").expect("read"), None);
        assert_eq!(file_at_head(dir.path(), "missing.txt").expect("read"), None);
    }

    #[test]
    fn content_at_head_is_none_without_commits() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        assert_eq!(file_at_head(dir.path(), "a.txt").expect("read"), None);
    }

    #[test]
    fn content_enforces_size_cap_and_utf8() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        fs::write(dir.path().join("big.txt"), vec![b'a'; MAX_FILE_BYTES as usize + 1]).expect("write");
        fs::write(dir.path().join("bin.dat"), [0xff, 0xfe, 0x00]).expect("write");
        stage_all(dir.path()).expect("stage");
        commit(dir.path(), "files").expect("commit");
        assert!(matches!(file_at_head(dir.path(), "big.txt"), Err(GitError::TooLarge)));
        assert!(matches!(file_at_head(dir.path(), "bin.dat"), Err(GitError::NotUtf8)));
    }

    #[test]
    fn branch_changes_include_committed_and_uncommitted_work() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "base");
        commit_file(dir.path(), "old.txt", "rename me please");
        sh(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit_file(dir.path(), "a.txt", "changed");
        sh(dir.path(), &["mv", "old.txt", "renamed.txt"]);
        fs::write(dir.path().join("untracked.txt"), "u").expect("write");

        let mut changes = branch_changes(dir.path(), "main").expect("changes");
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        let summary: Vec<(&str, FileKind, Option<&str>)> = changes
            .iter()
            .map(|c| (c.path.as_str(), c.kind, c.original_path.as_deref()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("a.txt", FileKind::Modified, None),
                ("renamed.txt", FileKind::Renamed, Some("old.txt")),
                ("untracked.txt", FileKind::Untracked, None),
            ]
        );
    }

    #[test]
    fn default_base_prefers_origin_head_then_main_then_master() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        assert_eq!(default_base_branch(dir.path()).expect("base"), None);
        commit_file(dir.path(), "a.txt", "a");
        sh(dir.path(), &["branch", "-m", "master"]);
        assert_eq!(default_base_branch(dir.path()).expect("base"), Some("master".to_owned()));
        sh(dir.path(), &["branch", "main"]);
        assert_eq!(default_base_branch(dir.path()).expect("base"), Some("main".to_owned()));
        sh(dir.path(), &["update-ref", "refs/remotes/origin/trunk", "HEAD"]);
        sh(dir.path(), &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/trunk"]);
        assert_eq!(default_base_branch(dir.path()).expect("base"), Some("origin/trunk".to_owned()));
    }

    #[test]
    fn repo_and_its_worktree_share_a_group_key() {
        let dir = tempdir().expect("tempdir");
        let repo = dir.path().join("my-repo");
        init_repo(&repo);
        commit_file(&repo, "a.txt", "a");
        let worktree = dir.path().join("wt");
        sh(&repo, &["worktree", "add", "-q", "-b", "wt", "--", worktree.to_str().expect("utf-8")]);

        let from_repo = repo_group(&repo).expect("group").expect("is a repo");
        assert_eq!(from_repo.name, "my-repo");
        assert_eq!(repo_group(&worktree).expect("group"), Some(from_repo.clone()));
        assert_eq!(repo_group(&repo.join("missing")).expect("group"), None);

        let main = repo_facts(&repo).expect("facts").expect("is a repo");
        let linked = repo_facts(&worktree).expect("facts").expect("is a repo");
        assert_eq!((main.group, main.linked_worktree, main.branch.as_deref()), (from_repo.clone(), false, Some("main")));
        assert_eq!((linked.group, linked.linked_worktree, linked.branch.as_deref()), (from_repo, true, Some("wt")));
        assert_eq!(repo_facts(&repo.join("missing")).expect("facts"), None);
    }

    #[test]
    fn repo_facts_cover_sub_folders_unborn_and_detached_heads() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        fs::write(dir.path().join("sub/new.txt"), "x").expect("write");
        let unborn = repo_facts(&dir.path().join("sub")).expect("facts").expect("is a repo");
        assert_eq!(unborn.branch.as_deref(), Some("main"));
        assert_eq!(unborn.changes, vec![file("new.txt", false, true, FileKind::Untracked, None)]);

        commit_file(dir.path(), "sub/a.txt", "one\n");
        fs::write(dir.path().join("sub/a.txt"), "one\ntwo\n").expect("write");
        sh(dir.path(), &["checkout", "-q", "--detach"]);
        let detached = repo_facts(&dir.path().join("sub")).expect("facts").expect("is a repo");
        assert_eq!(detached.branch, None);
        assert!(detached.changes.contains(&counted(file("a.txt", false, true, FileKind::Modified, None), 1, 0)));

        sh(dir.path(), &["checkout", "-q", "-b", "(detached)"]);
        let named = repo_facts(dir.path()).expect("facts").expect("is a repo");
        assert_eq!(named.branch.as_deref(), Some("(detached)"));
    }

    #[test]
    fn hostile_paths_and_revisions_are_not_options_or_pathspec_magic() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        commit_file(dir.path(), "a.txt", "a");
        fs::write(dir.path().join("-rf.txt"), "x").expect("write");
        fs::write(dir.path().join("b.txt"), "b").expect("write");

        stage(dir.path(), &paths(&["-rf.txt"])).expect("dash path staged");
        assert!(stage(dir.path(), &paths(&["*.txt"])).is_err());
        let staged: Vec<String> = status(dir.path())
            .expect("status")
            .into_iter()
            .filter(|f| f.staged)
            .map(|f| f.path)
            .collect();
        assert_eq!(staged, vec!["-rf.txt"]);

        assert!(matches!(
            file_at_merge_base(dir.path(), "a.txt", "--output=evil.txt"),
            Err(GitError::UnknownRevision(_))
        ));
        assert!(branch_changes(dir.path(), "-h").is_err());
        assert!(!dir.path().join("evil.txt").exists());

        commit_file(dir.path(), "@{-1}", "literal name");
        assert_eq!(file_at_head(dir.path(), "@{-1}").expect("read"), Some("literal name".to_owned()));
    }

    #[test]
    fn push_sets_upstream_then_pushes_again() {
        let dir = tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        init_repo(&repo);
        commit_file(&repo, "a.txt", "a");
        assert!(matches!(push(&repo), Err(GitError::Failed(text)) if text.contains("origin")));

        add_origin(&repo, &dir.path().join("origin.git"));
        push(&repo).expect("first push");
        assert_eq!(sh(&repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]), "origin/main");
        commit_file(&repo, "b.txt", "b");
        push(&repo).expect("second push");
        assert_eq!(sh(&repo, &["rev-list", "--count", "HEAD", "--not", "--remotes"]), "0");
    }

    fn finishes_within<T: Send + 'static>(secs: u64, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
    }

    fn write_hook(repo: &Path, name: &str, body: &str) {
        let hooks = repo.join(".git").join("hooks");
        fs::create_dir_all(&hooks).expect("hooks dir");
        fs::write(hooks.join(name), format!("#!/bin/sh\n{body}\n")).expect("hook");
    }

    #[test]
    fn push_with_a_tag_of_the_same_name_pushes_the_branch_to_its_own_name() {
        let dir = tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        let origin = dir.path().join("origin.git");
        init_repo(&repo);
        commit_file(&repo, "a.txt", "a");
        add_origin(&repo, &origin);
        sh(&repo, &["checkout", "-q", "-b", "release"]);
        sh(&repo, &["tag", "release"]);
        commit_file(&repo, "b.txt", "b");
        push(&repo).expect("push");
        let remote_refs = sh(&origin, &["for-each-ref", "--format=%(refname)"]);
        assert!(remote_refs.contains("refs/heads/release"), "remote refs: {remote_refs}");
        assert!(!remote_refs.contains("refs/heads/heads/"), "pushed to a wrong ref: {remote_refs}");
    }

    #[test]
    fn size_cap_applies_to_the_filtered_content_not_the_stored_blob() {
        let dir = tempdir().expect("tempdir");
        init_repo(dir.path());
        sh(dir.path(), &["config", "filter.big.clean", "cat"]);
        sh(dir.path(), &["config", "filter.big.smudge", "yes | head -c 6000000"]);
        commit_file(dir.path(), ".gitattributes", "big.txt filter=big\n");
        fs::write(dir.path().join("big.txt"), "tiny pointer").expect("write");
        stage_all(dir.path()).expect("stage");
        commit(dir.path(), "pointer").expect("commit");
        let result = file_at_head(dir.path(), "big.txt");
        assert!(
            matches!(result, Err(GitError::TooLarge)),
            "read {} bytes despite the 5 MB cap",
            result.map(|s| s.map_or(0, |s| s.len())).unwrap_or(0)
        );
    }

    #[test]
    fn commit_with_a_long_message_and_a_chatty_hook_does_not_deadlock() {
        let dir = tempdir().expect("tempdir");
        let repo = dir.path().to_path_buf();
        init_repo(&repo);
        commit_file(&repo, "a.txt", "a");
        write_hook(&repo, "pre-commit", "head -c 1000000 /dev/zero | tr '\\0' x >&2");
        fs::write(repo.join("a.txt"), "changed").expect("write");
        stage_all(&repo).expect("stage");
        let message = format!("subject\n\n{}", "body line\n".repeat(100_000));
        let worker = repo.clone();
        let outcome = finishes_within(20, move || commit(&worker, &message).is_ok());
        assert_eq!(outcome, Some(true), "commit hung (pipe deadlock) or failed");
    }

    #[test]
    fn push_is_stopped_when_its_deadline_passes() {
        let dir = tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        init_repo(&repo);
        commit_file(&repo, "a.txt", "a");
        add_origin(&repo, &dir.path().join("origin.git"));
        write_hook(&repo, "pre-push", "sleep 30");
        let worker = repo.clone();
        let outcome = finishes_within(10, move || push_within(&worker, Duration::from_secs(1)));
        assert!(matches!(outcome, Some(Err(GitError::TimedOut))), "{outcome:?}");
    }

    #[test]
    fn git_does_not_inherit_variables_that_redirect_the_repository() {
        let command = git_command(Path::new("."), &["status"]);
        let removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().to_ascii_uppercase())
            .collect();
        for name in REDIRECTING_VARS {
            assert!(removed.contains(&name.to_owned()), "{name} is inherited");
        }
    }
}
