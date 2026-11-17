//! Runs every `@core` scenario in `features/` against the public core API.

mod changes;
mod sessions;
mod state;
mod status;
mod terminals;
mod worktrees;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use cucumber::World;
use orchestrator_core::state::AppState;
use orchestrator_core::status::SessionStatus;
use orchestrator_core::worktrees::Blocker;
use tempfile::TempDir;

const FEATURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../features");

/// Repositories built once per run and copied into each scenario; starting git costs ~0.4 s a call on Windows.
static TEMPLATES: OnceLock<PathBuf> = OnceLock::new();

#[derive(World)]
#[world(init = Self::new)]
pub struct CoreWorld {
    dir: TempDir,
    repos: HashMap<String, PathBuf>,
    state: AppState,
    /// Debug text of the last refused action, such as `Conflict` or `Blocked([...])`.
    refusal: Option<String>,
    status: Option<SessionStatus>,
    opened_version: Option<String>,
    state_text: Option<String>,
    saved_id: Option<String>,
    worktree: Option<(PathBuf, PathBuf)>,
    inner_worktree: Option<PathBuf>,
    shown: Vec<Blocker>,
    commit: Option<String>,
    outside: Option<PathBuf>,
    terminal: Option<terminals::Rig>,
}

impl fmt::Debug for CoreWorld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CoreWorld")
            .field("dir", &self.dir.path())
            .field("repos", &self.repos)
            .field("refusal", &self.refusal)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl CoreWorld {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temp dir"),
            repos: HashMap::new(),
            state: AppState::default(),
            refusal: None,
            status: None,
            opened_version: None,
            state_text: None,
            saved_id: None,
            worktree: None,
            inner_worktree: None,
            shown: Vec::new(),
            commit: None,
            outside: None,
            terminal: None,
        }
    }

    fn repo(&self, name: &str) -> PathBuf {
        self.repos.get(name).unwrap_or_else(|| panic!("no repository named {name:?} in this scenario")).clone()
    }

    /// A repository on `main` with a committed README and a `.gitignore` that ignores `*.log`.
    fn new_repo(&mut self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        copy_dir(&templates().join("plain"), &path);
        self.repos.insert(name.to_owned(), path.clone());
        path
    }

    /// Like [`Self::new_repo`], with `main` pushed to a bare `<name>-origin.git` beside it.
    fn new_repo_with_remote(&mut self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        let origin = self.dir.path().join(format!("{name}-origin.git"));
        copy_dir(&templates().join("remote"), &path);
        copy_dir(&templates().join("remote-origin.git"), &origin);
        let config = path.join(".git").join("config");
        let template_origin = slashes(&templates().join("remote-origin.git"));
        fs::write(&config, read(&config).replace(&template_origin, &slashes(&origin))).expect("point at the new origin");
        self.repos.insert(name.to_owned(), path.clone());
        path
    }

    fn worktree(&self) -> (PathBuf, PathBuf) {
        self.worktree.clone().expect("a worktree was created earlier in the scenario")
    }

    fn state_path(&self) -> PathBuf {
        self.dir.path().join("appdata").join("AgenticOrchestrator").join("state.json")
    }

    fn record<T, E: fmt::Debug>(&mut self, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(value) => {
                self.refusal = None;
                Some(value)
            }
            Err(error) => {
                self.refusal = Some(format!("{error:?}"));
                None
            }
        }
    }

    fn assert_refused(&self, variant: &str) {
        let refusal = self.refusal.as_deref().unwrap_or("nothing (it succeeded)");
        assert!(refusal.starts_with(variant), "expected a refusal of kind {variant}, got {refusal}");
    }
}

/// Runs `work` on its own thread so other scenarios proceed while it waits on git or a terminal.
pub async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(panic::catch_unwind(AssertUnwindSafe(work)));
    });
    match receiver.await.expect("worker thread finished") {
        Ok(value) => value,
        Err(failure) => panic::resume_unwind(failure),
    }
}

/// Runs git in `dir` and returns its trimmed output, failing the step if git fails.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

pub fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().expect("parent folder")).expect("create parent folder");
    fs::write(path, content).expect("write file");
}

pub fn commit_file(dir: &Path, rel: &str, content: &str) {
    write(dir, rel, content);
    git(dir, &["add", "--", rel]);
    git(dir, &["commit", "-q", "-m", &format!("change {rel}")]);
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn junction(link: &Path, target: &Path) {
    fs::create_dir_all(link.parent().expect("parent folder")).expect("create parent folder");
    // git reports worktree paths with forward slashes, which mklink reads as switches.
    let windows_path = |path: &Path| path.to_string_lossy().replace('/', "\\");
    let made = Command::new("cmd")
        .args(["/C", "mklink", "/J", &windows_path(link), &windows_path(target)])
        .output()
        .expect("run mklink");
    assert!(made.status.success(), "could not create a junction: {}", String::from_utf8_lossy(&made.stderr));
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create folder");
    for entry in fs::read_dir(from).expect("list folder") {
        let entry = entry.expect("folder entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

fn templates() -> &'static Path {
    TEMPLATES.get().expect("templates are built before the run")
}

pub fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("create repository folder");
    git(path, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "BDD"),
        ("user.email", "bdd@example.com"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(path, &["config", key, value]);
    }
}

fn build_templates(dir: &Path) {
    let plain = dir.join("plain");
    init_repo(&plain);
    commit_file(&plain, ".gitignore", "*.log\n");
    commit_file(&plain, "README.md", "# repository\n");
    let remote = dir.join("remote");
    copy_dir(&plain, &remote);
    let origin = dir.join("remote-origin.git");
    fs::create_dir_all(&origin).expect("create origin folder");
    git(&origin, &["init", "-q", "--bare"]);
    git(&remote, &["remote", "add", "origin", &slashes(&origin)]);
    git(&remote, &["push", "-q", "-u", "origin", "main"]);
}

fn main() {
    let dir = tempfile::tempdir().expect("templates dir");
    build_templates(dir.path());
    TEMPLATES.set(dir.path().to_path_buf()).expect("templates set once");
    let outcome = panic::catch_unwind(|| {
        futures::executor::block_on(
            CoreWorld::cucumber()
                .fail_on_skipped()
                .filter_run_and_exit(FEATURES, |_, _, scenario| scenario.tags.iter().any(|tag| tag == "core")),
        );
    });
    drop(dir);
    if let Err(failure) = outcome {
        panic::resume_unwind(failure);
    }
}
