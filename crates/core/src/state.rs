use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("state directory must be absolute and a home directory must be available")]
    InvalidStateDirectory,
    #[error("LOCALAPPDATA is not set")]
    NoLocalAppData,
    #[error("state file is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("workspace path is not an existing directory")]
    NotADirectory,
    #[error("workspace path is not valid Unicode or has a folder name ending in a dot or space")]
    UnsupportedPath,
    #[error("folder is already open as workspace {existing_id}")]
    DuplicateWorkspace { existing_id: String },
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct AppState {
    pub workspaces: Vec<Workspace>,
    pub settings: Settings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub claude_session_id: Option<String>,
    pub created_worktree: Option<CreatedWorktree>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct CreatedWorktree {
    pub repo_root: PathBuf,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub shell_program: String,
    pub shell_args: Vec<String>,
    pub extra_claude_args: Vec<String>,
    pub worktree_root: Option<String>,
    pub post_create_command: Vec<String>,
    pub pre_remove_command: Vec<String>,
    pub zoom_percent: u32,
    pub default_layout: LayoutMode,
    pub default_where: WhereToRun,
    pub diff_layout: DiffLayout,
    pub changes_layout: ChangesLayout,
    /// Font family tried before the built-in terminal fonts; empty for none.
    pub terminal_font: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LayoutMode {
    Terminal,
    #[default]
    Both,
    Ide,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum WhereToRun {
    Folder,
    #[default]
    Worktree,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DiffLayout {
    #[default]
    SideBySide,
    Inline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ChangesLayout {
    #[default]
    Tree,
    List,
}

const DEFAULT_ZOOM_PERCENT: u32 = 100;
const MIN_ZOOM_PERCENT: u32 = 50;
const MAX_ZOOM_PERCENT: u32 = 200;

impl Settings {
    pub fn zoom_factor(&self) -> f64 {
        f64::from(self.zoom_percent.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT)) / 100.0
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shell_program: crate::terminal::default_shell(crate::environment::environment()),
            shell_args: Vec::new(),
            extra_claude_args: Vec::new(),
            worktree_root: None,
            post_create_command: Vec::new(),
            pre_remove_command: Vec::new(),
            zoom_percent: DEFAULT_ZOOM_PERCENT,
            default_layout: LayoutMode::default(),
            default_where: WhereToRun::default(),
            diff_layout: DiffLayout::default(),
            changes_layout: ChangesLayout::default(),
            terminal_font: String::new(),
        }
    }
}

pub fn default_state_path() -> Result<PathBuf, StateError> {
    state_path_with(|name| std::env::var_os(name))
}

fn state_path_with(get: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, StateError> {
    if let Some(folder) = get("AGENTIC_ORCHESTRATOR_STATE_DIR") {
        let folder = PathBuf::from(folder);
        return if folder.is_absolute() { Ok(folder.join("state.json")) } else { Err(StateError::InvalidStateDirectory) };
    }
    #[cfg(windows)]
    { state_path_under(get("LOCALAPPDATA")) }
    #[cfg(unix)]
    {
        let home = || get("HOME").map(PathBuf::from).filter(|p| p.is_absolute()).ok_or(StateError::InvalidStateDirectory);
        #[cfg(target_os = "macos")]
        let base = home()?.join("Library/Application Support");
        #[cfg(not(target_os = "macos"))]
        let base = match get("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()) {
            Some(base) => base,
            None => home()?.join(".local/share"),
        };
        Ok(base.join("AgenticOrchestrator/state.json"))
    }
}

#[cfg(windows)]
fn state_path_under(local_app_data: Option<OsString>) -> Result<PathBuf, StateError> {
    let local = PathBuf::from(local_app_data.ok_or(StateError::NoLocalAppData)?);
    if !local.is_absolute() {
        return Err(StateError::NoLocalAppData);
    }
    Ok(local.join("AgenticOrchestrator").join("state.json"))
}

pub fn load(path: &Path) -> Result<AppState, StateError> {
    match fs::read(path) {
        Ok(bytes) => {
            let json = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
            Ok(serde_json::from_slice(json)?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppState::default()),
        Err(error) => Err(error.into()),
    }
}

pub fn save(path: &Path, state: &AppState) -> Result<(), StateError> {
    // Windows fails a rename onto a file another rename is replacing, so saves take turns.
    static SAVE_LOCK: Mutex<()> = Mutex::new(());
    let _guard = SAVE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = path.parent().ok_or(StateError::NotADirectory)?;
    fs::create_dir_all(dir)?;
    let mut temp = NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut temp, state)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    replace_with_retry(temp, path)?;
    Ok(())
}

const REPLACE_ATTEMPTS: u32 = 5;
const REPLACE_RETRY_DELAY: Duration = Duration::from_millis(20);

// Antivirus and the search indexer briefly open a freshly written file, which makes
// the replace fail with access denied; the hold clears within milliseconds.
fn replace_with_retry(mut temp: NamedTempFile, path: &Path) -> io::Result<()> {
    for _ in 1..REPLACE_ATTEMPTS {
        match temp.persist(path) {
            Ok(_) => return Ok(()),
            Err(failed) if failed.error.kind() == io::ErrorKind::PermissionDenied => {
                temp = failed.file;
                thread::sleep(REPLACE_RETRY_DELAY);
            }
            Err(failed) => return Err(failed.error),
        }
    }
    temp.persist(path).map(drop).map_err(|failed| failed.error)
}

pub fn add_workspace<'a>(
    state: &'a mut AppState,
    path: &Path,
    name: Option<String>,
) -> Result<&'a Workspace, StateError> {
    if !path.is_dir() {
        return Err(StateError::NotADirectory);
    }
    let canonical = fs::canonicalize(path)?;
    // Stripping `\\?\` below would make Windows trim these names and resolve another folder.
    let trimmed_name = cfg!(windows) && canonical.components().any(|part| {
        matches!(part, Component::Normal(name) if name.to_string_lossy().ends_with(['.', ' ']))
    });
    if trimmed_name || canonical.to_str().is_none() {
        return Err(StateError::UnsupportedPath);
    }
    let real_path = without_verbatim_prefix(&canonical);
    let key = dedupe_key(&real_path);
    if let Some(existing) = state.workspaces.iter().find(|w| dedupe_key(&w.path) == key) {
        return Err(StateError::DuplicateWorkspace {
            existing_id: existing.id.clone(),
        });
    }
    let name = name.unwrap_or_else(|| {
        real_path
            .file_name()
            .unwrap_or(real_path.as_os_str())
            .to_string_lossy()
            .into_owned()
    });
    let index = state.workspaces.len();
    state.workspaces.push(Workspace {
        id: Uuid::new_v4().to_string(),
        name,
        path: real_path,
        claude_session_id: None,
        created_worktree: None,
    });
    Ok(&state.workspaces[index])
}

pub fn remove_workspace(state: &mut AppState, id: &str) -> Option<Workspace> {
    let index = state.workspaces.iter().position(|w| w.id == id)?;
    Some(state.workspaces.remove(index))
}

/// Puts the workspaces in the order of `ids`; any not named keep their order, after the rest.
pub fn reorder_workspaces(state: &mut AppState, ids: &[String]) {
    state
        .workspaces
        .sort_by_key(|w| ids.iter().position(|id| *id == w.id).unwrap_or(usize::MAX));
}

// Stored paths become terminal working directories, and many tools reject `\\?\` paths.
pub fn without_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn dedupe_key(path: &Path) -> String {
    let real =
        fs::canonicalize(path).map_or_else(|_| path.to_path_buf(), |p| without_verbatim_prefix(&p));
    real.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn default_path_is_under_local_app_data() {
        let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
        assert_eq!(
            default_state_path().unwrap(),
            local.join("AgenticOrchestrator").join("state.json")
        );
    }

    #[cfg(windows)]
    #[test]
    fn state_path_requires_an_absolute_local_app_data() {
        assert_eq!(
            state_path_under(Some(r"C:\Users\u\AppData\Local".into())).unwrap(),
            Path::new(r"C:\Users\u\AppData\Local\AgenticOrchestrator\state.json")
        );
        for value in [None, Some(""), Some("relative"), Some(r"\no-drive")] {
            assert!(
                matches!(
                    state_path_under(value.map(OsString::from)),
                    Err(StateError::NoLocalAppData)
                ),
                "{value:?} was accepted"
            );
        }
    }

    #[test]
    fn load_missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let state = load(&dir.path().join("state.json")).unwrap();
        assert_eq!(state, AppState::default());
        assert_eq!(state.settings.shell_program, Settings::default().shell_program);
    }

    #[test]
    fn load_unparseable_file_errors_and_leaves_it_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(load(&path), Err(StateError::Json(_))));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn load_fills_missing_fields_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(
            &path,
            r#"{"workspaces":[{"id":"w1","name":"repo","path":"C:\\repo"}],"settings":{"shellArgs":["-NoLogo"]}}"#,
        )
        .unwrap();

        let state = load(&path).unwrap();

        assert_eq!(state.workspaces[0].claude_session_id, None);
        assert_eq!(state.settings.shell_program, Settings::default().shell_program);
        assert_eq!(state.settings.shell_args, ["-NoLogo"]);
        assert_eq!(state.settings.zoom_percent, 100);
        assert_eq!(state.settings.default_layout, LayoutMode::Both);
        assert_eq!(state.settings.default_where, WhereToRun::Worktree);
        assert_eq!(state.settings.diff_layout, DiffLayout::SideBySide);
        assert_eq!(state.settings.changes_layout, ChangesLayout::Tree);
    }

    #[test]
    fn zoom_factor_is_clamped_to_half_and_double() {
        let zoom = |zoom_percent| Settings { zoom_percent, ..Settings::default() }.zoom_factor();
        assert_eq!([zoom(0), zoom(100), zoom(150), zoom(900)], [0.5, 1.0, 1.5, 2.0]);
    }

    #[test]
    fn save_creates_directory_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("state.json");
        let mut state = AppState::default();
        state.workspaces.push(Workspace {
            id: "w1".into(),
            name: "repo".into(),
            path: PathBuf::from(r"C:\repo"),
            claude_session_id: Some("s1".into()),
            created_worktree: Some(CreatedWorktree {
                repo_root: PathBuf::from(r"C:\main"),
                branch: "feature".into(),
            }),
        });
        state.settings.post_create_command = vec!["npm".into(), "ci".into()];

        save(&path, &state).unwrap();
        save(&path, &state).unwrap();

        assert_eq!(load(&path).unwrap(), state);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("claudeSessionId")
        );
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn add_workspace_uses_folder_name_and_real_path() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("my-repo");
        fs::create_dir(&folder).unwrap();
        let mut state = AppState::default();

        let workspace = add_workspace(&mut state, &folder.join("."), None)
            .unwrap()
            .clone();

        assert_eq!(workspace.name, "my-repo");
        assert!(!workspace.path.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(
            fs::canonicalize(&workspace.path).unwrap(),
            fs::canonicalize(&folder).unwrap()
        );
        assert!(Uuid::parse_str(&workspace.id).is_ok());
        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn add_workspace_accepts_custom_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::default();
        let workspace = add_workspace(&mut state, dir.path(), Some("Mine".into())).unwrap();
        assert_eq!(workspace.name, "Mine");
    }

    #[test]
    fn add_workspace_rejects_duplicate_by_real_path_ignoring_case() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::default();
        let first_id = add_workspace(&mut state, dir.path(), None)
            .unwrap()
            .id
            .clone();
        let upper = PathBuf::from(dir.path().to_string_lossy().to_uppercase());

        let result = add_workspace(&mut state, &upper, None);

        assert!(
            matches!(result, Err(StateError::DuplicateWorkspace { existing_id }) if existing_id == first_id)
        );
        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn add_workspace_rejects_missing_folder_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, "x").unwrap();
        let mut state = AppState::default();
        assert!(matches!(
            add_workspace(&mut state, &dir.path().join("missing"), None),
            Err(StateError::NotADirectory)
        ));
        assert!(matches!(
            add_workspace(&mut state, &file, None),
            Err(StateError::NotADirectory)
        ));
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn remove_workspace_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::default();
        let id = add_workspace(&mut state, dir.path(), None)
            .unwrap()
            .id
            .clone();

        assert_eq!(remove_workspace(&mut state, "unknown"), None);
        assert_eq!(remove_workspace(&mut state, &id).map(|w| w.id), Some(id));
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn reorder_workspaces_follows_the_ids_and_keeps_unnamed_ones_last() {
        let workspace = |id: &str| Workspace { id: id.into(), ..Workspace::default() };
        let mut state = AppState {
            workspaces: ["a", "b", "c", "d"].map(workspace).into(),
            ..AppState::default()
        };

        reorder_workspaces(&mut state, &["c".into(), "unknown".into(), "a".into()]);

        let order: Vec<_> = state.workspaces.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(order, ["c", "a", "b", "d"]);
    }

    #[cfg(windows)]
    #[test]
    fn add_workspace_rejects_folder_names_ending_in_dot_or_space() {
        let dir = tempfile::tempdir().unwrap();
        // The `\\?\` form keeps the trailing dot and space literal.
        let base = fs::canonicalize(dir.path()).unwrap();
        let mut state = AppState::default();
        for name in ["repo.", "repo "] {
            let folder = base.join(name);
            fs::create_dir(&folder).unwrap();
            assert!(
                matches!(
                    add_workspace(&mut state, &folder, None),
                    Err(StateError::UnsupportedPath)
                ),
                "{name:?} was accepted"
            );
        }
        assert!(state.workspaces.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn add_workspace_rejects_non_utf8_path() {
        use std::os::windows::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(OsString::from_wide(&[0x61, 0xD800]));
        fs::create_dir(&folder).unwrap();
        let mut state = AppState::default();

        assert!(matches!(
            add_workspace(&mut state, &folder, None),
            Err(StateError::UnsupportedPath)
        ));
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn concurrent_saves_all_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let failures: Vec<String> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    s.spawn(|| {
                        (0..200)
                            .filter_map(|_| save(&path, &AppState::default()).err())
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect()
        });
        let leftovers = fs::read_dir(dir.path()).unwrap().count();
        assert!(
            failures.is_empty() && leftovers == 1,
            "{} of 800 saves failed, e.g. {:?}; {leftovers} files left",
            failures.len(),
            failures.first()
        );
    }

    #[test]
    fn load_accepts_state_file_with_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(
            &path,
            b"\xEF\xBB\xBF{\"settings\":{\"shellProgram\":\"cmd.exe\"}}",
        )
        .unwrap();
        assert_eq!(load(&path).unwrap().settings.shell_program, "cmd.exe");
    }
}

#[cfg(test)]
mod platform_path_tests {
    use super::*;

    #[test]
    fn override_is_absolute_and_does_not_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let get = |name: &str| (name == "AGENTIC_ORCHESTRATOR_STATE_DIR").then(|| dir.path().as_os_str().to_owned());
        assert_eq!(state_path_with(get).unwrap(), dir.path().join("state.json"));
        assert!(matches!(state_path_with(|_| Some("relative".into())), Err(StateError::InvalidStateDirectory)));
    }

    #[cfg(unix)]
    #[test]
    fn unix_default_uses_the_platform_data_directory() {
        let path = state_path_with(|name| match name {
            "HOME" => Some("/home/test".into()),
            "XDG_DATA_HOME" => Some("/data".into()),
            _ => None,
        }).unwrap();
        #[cfg(target_os = "macos")]
        assert_eq!(path, Path::new("/home/test/Library/Application Support/AgenticOrchestrator/state.json"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(path, Path::new("/data/AgenticOrchestrator/state.json"));
    }
}
