use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

const GIT_DIR_NAME: &str = ".git";

#[cfg(windows)]
const RESERVED_NAMES: [&str; 30] = [
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
    "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

#[cfg(windows)]
const INVALID_CHARS: [char; 7] = ['<', '>', '"', '|', '?', '*', ':'];

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("path is outside the workspace")]
    OutsideWorkspace,
    #[error("path contains a name Windows does not allow")]
    InvalidName,
    #[error("file is larger than the {MAX_FILE_BYTES} byte limit")]
    TooLarge,
    #[error("file is not UTF-8 text")]
    NotText,
    #[error("file changed on disk since it was read")]
    Conflict,
    #[error("path is inside the git directory")]
    GitDirectory,
    #[error("path is a directory")]
    IsDirectory,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Walk(#[from] ignore::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub text: String,
    pub version: String,
}

/// Resolves a UI-supplied relative path to a canonical path inside `root`.
/// The target need not exist, but its parent directory must.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, FileError> {
    resolve_parts(root, &split_relative(rel)?)
}

pub fn list_dir(root: &Path, rel: &str) -> Result<Vec<DirEntry>, FileError> {
    let parts = split_relative(rel)?;
    let dir = resolve_parts(root, &parts)?;
    let mut entries = Vec::new();
    let walker = WalkBuilder::new(&dir)
        .max_depth(Some(1))
        .hidden(false)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != GIT_DIR_NAME)
        .build();
    for entry in walker {
        let entry = entry?;
        if entry.depth() == 0 {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let rel_path = if parts.is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", parts.join("/"))
        };
        entries.push(DirEntry {
            is_dir: entry.path().is_dir(),
            name,
            rel_path,
        });
    }
    entries
        .sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase(), entry.name.clone()));
    Ok(entries)
}

pub fn read_file(root: &Path, rel: &str) -> Result<FileContent, FileError> {
    let bytes = read_capped(&resolve(root, rel)?)?;
    let version = version_of(&bytes);
    if bytes.contains(&0) {
        return Err(FileError::NotText);
    }
    let text = String::from_utf8(bytes).map_err(|_| FileError::NotText)?;
    Ok(FileContent { text, version })
}

/// Writes `text` atomically and returns the new version. With `expected_version` the file on
/// disk must still match it; without, the file must not exist. Otherwise `Conflict` is returned
/// and nothing is written.
pub fn write_file(
    root: &Path,
    rel: &str,
    text: &str,
    expected_version: Option<&str>,
) -> Result<String, FileError> {
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(FileError::TooLarge);
    }
    let path = resolve(root, rel)?;
    if path.is_dir() {
        return Err(FileError::IsDirectory);
    }
    let dir = path.parent().ok_or(FileError::OutsideWorkspace)?;
    let mut temp = NamedTempFile::new_in(dir)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    match (expected_version, version_at(&path)) {
        (None, Ok(None)) => {}
        (Some(expected), Ok(Some(current))) if current == expected => {}
        (_, Ok(_) | Err(FileError::TooLarge)) => return Err(FileError::Conflict),
        (_, Err(error)) => return Err(error),
    }
    temp.persist(&path).map_err(|error| error.error)?;
    Ok(version_of(text.as_bytes()))
}

/// Returns the current version of a file, or `None` if it does not exist.
pub fn file_version(root: &Path, rel: &str) -> Result<Option<String>, FileError> {
    match resolve(root, rel) {
        Ok(path) => version_at(&path),
        Err(FileError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn split_relative(rel: &str) -> Result<Vec<&str>, FileError> {
    let mut parts = Vec::new();
    for component in Path::new(rel).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part.to_str().ok_or(FileError::InvalidName)?;
                // Resolved paths are `\\?\` verbatim, so Windows would create these names
                // literally instead of normalising them, leaving files most tools cannot open.
                #[cfg(windows)]
                if part.contains(INVALID_CHARS)
                    || part.ends_with(['.', ' '])
                    || is_reserved_name(part)
                {
                    return Err(FileError::InvalidName);
                }
                parts.push(part);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(FileError::OutsideWorkspace);
            }
        }
    }
    Ok(parts)
}

#[cfg(windows)]
fn is_reserved_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end();
    RESERVED_NAMES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(stem))
}

fn resolve_parts(root: &Path, parts: &[&str]) -> Result<PathBuf, FileError> {
    let root = fs::canonicalize(root)?;
    let joined = parts
        .iter()
        .fold(root.clone(), |path, part| path.join(part));
    // symlink_metadata sees a dangling link, which must not be written through.
    let resolved = match fs::symlink_metadata(&joined) {
        Ok(_) => fs::canonicalize(&joined)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let (Some(parent), Some(name)) = (joined.parent(), joined.file_name()) else {
                return Err(FileError::OutsideWorkspace);
            };
            fs::canonicalize(parent)?.join(name)
        }
        Err(error) => return Err(error.into()),
    };
    let inside = resolved
        .strip_prefix(&root)
        .map_err(|_| FileError::OutsideWorkspace)?;
    // Writing git's hooks or config, or a worktree's `.git` pointer, runs code on the next git call.
    if inside
        .components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case(GIT_DIR_NAME))
    {
        return Err(FileError::GitDirectory);
    }
    Ok(resolved)
}

fn read_capped(path: &Path) -> Result<Vec<u8>, FileError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FileError::TooLarge);
    }
    Ok(bytes)
}

fn version_at(path: &Path) -> Result<Option<String>, FileError> {
    match read_capped(path) {
        Ok(bytes) => Ok(Some(version_of(&bytes))),
        Err(FileError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn version_of(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:x}-{:016x}", bytes.len(), hasher.finish())
}

#[cfg(windows)]
const FILE_MANAGER: &str = "explorer.exe";
#[cfg(target_os = "macos")]
const FILE_MANAGER: &str = "open";
#[cfg(all(unix, not(target_os = "macos")))]
const FILE_MANAGER: &str = "xdg-open";

/// Opens `folder` in Explorer, Finder or the desktop's file manager. Spawn it without waiting:
/// Explorer exits with status 1 even when it succeeds.
pub fn file_manager_command(folder: &Path) -> std::process::Command {
    let mut command = crate::git::hidden_command(FILE_MANAGER);
    command.arg(folder).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[cfg(windows)]
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn file_manager_command_passes_the_folder_as_its_only_argument() {
        let folder = Path::new("my repo (1)");
        let command = file_manager_command(folder);
        assert_eq!(command.get_program(), FILE_MANAGER);
        assert_eq!(command.get_args().collect::<Vec<_>>(), [folder.as_os_str()]);
    }

    fn workspace() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write(dir: &Path, rel: &str, contents: &[u8]) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn names(entries: &[DirEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn resolve_rejects_parent_and_absolute_paths() {
        let ws = workspace();
        let other = workspace();
        let other_abs = other.path().join("x.txt");
        for rel in [
            "..",
            "../x.txt",
            "a/../../x.txt",
            "a/../b.txt",
            "/x.txt",
            #[cfg(windows)]
            r"C:\Windows\win.ini",
            #[cfg(windows)]
            "C:x.txt",
            #[cfg(windows)]
            r"\x.txt",
            #[cfg(windows)]
            r"\\server\share\x.txt",
            #[cfg(windows)]
            r"\\?\C:\x.txt",
            other_abs.to_str().unwrap(),
        ] {
            assert!(
                matches!(resolve(ws.path(), rel), Err(FileError::OutsideWorkspace)),
                "{rel} was not rejected"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn resolve_rejects_device_names_and_streams() {
        let ws = workspace();
        for rel in ["nul", "NUL.txt", "a/con", "com1", "a.txt:stream"] {
            assert!(
                matches!(resolve(ws.path(), rel), Err(FileError::InvalidName)),
                "{rel} was not rejected"
            );
        }
    }

    #[test]
    fn resolve_accepts_existing_and_new_files_inside_root() {
        let ws = workspace();
        write(ws.path(), "src/main.rs", b"fn main() {}");
        let root = fs::canonicalize(ws.path()).unwrap();
        assert_eq!(
            resolve(ws.path(), "src/main.rs").unwrap(),
            root.join("src").join("main.rs")
        );
        assert_eq!(
            resolve(ws.path(), "./src/new.rs").unwrap(),
            root.join("src").join("new.rs")
        );
        assert_eq!(resolve(ws.path(), "").unwrap(), root);
    }

    #[test]
    fn resolve_rejects_junction_escaping_root() {
        let ws = workspace();
        let outside = workspace();
        write(outside.path(), "secret.txt", b"secret");
        let link = ws.path().join("link");
        #[cfg(windows)]
        let created = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .is_ok_and(|output| output.status.success());
        #[cfg(windows)]
        assert!(created, "could not create a junction");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        for rel in ["link", "link/secret.txt", "link/new.txt"] {
            assert!(
                matches!(resolve(ws.path(), rel), Err(FileError::OutsideWorkspace)),
                "{rel} was not rejected"
            );
        }
        assert!(matches!(
            read_file(ws.path(), "link/secret.txt"),
            Err(FileError::OutsideWorkspace)
        ));
        assert!(matches!(
            write_file(ws.path(), "link/new.txt", "x", None),
            Err(FileError::OutsideWorkspace)
        ));
        assert!(!outside.path().join("new.txt").exists());
    }

    #[test]
    fn list_dir_respects_gitignore_hides_git_and_sorts() {
        let ws = workspace();
        let git_dir = [".", "git"].concat();
        fs::create_dir_all(ws.path().join(&git_dir)).unwrap();
        write(
            ws.path(),
            ".gitignore",
            b"ignored.txt\nbuild/\nalpha/skip.txt\n",
        );
        write(ws.path(), "ignored.txt", b"");
        write(ws.path(), "kept.txt", b"");
        write(ws.path(), "Beta.txt", b"");
        write(ws.path(), ".env", b"");
        write(ws.path(), "build/out.txt", b"");
        write(ws.path(), "Zeta/z.txt", b"");
        write(ws.path(), "alpha/one.txt", b"");
        write(ws.path(), "alpha/skip.txt", b"");

        let top = list_dir(ws.path(), "").unwrap();
        assert_eq!(
            names(&top),
            [
                "alpha",
                "Zeta",
                ".env",
                ".gitignore",
                "Beta.txt",
                "kept.txt"
            ]
        );
        assert!(top[0].is_dir && top[1].is_dir && !top[2].is_dir);

        let alpha = list_dir(ws.path(), "alpha").unwrap();
        assert_eq!(
            alpha,
            [DirEntry {
                name: "one.txt".into(),
                rel_path: "alpha/one.txt".into(),
                is_dir: false
            }]
        );
    }

    #[test]
    fn read_rejects_large_and_binary_files() {
        let ws = workspace();
        write(
            ws.path(),
            "big.txt",
            &vec![b'a'; MAX_FILE_BYTES as usize + 1],
        );
        write(ws.path(), "max.txt", &vec![b'a'; MAX_FILE_BYTES as usize]);
        write(ws.path(), "nul-byte.bin", b"ab\0cd");
        write(ws.path(), "latin1.txt", &[0x63, 0x61, 0x66, 0xe9]);
        assert!(matches!(
            read_file(ws.path(), "big.txt"),
            Err(FileError::TooLarge)
        ));
        assert!(read_file(ws.path(), "max.txt").is_ok());
        assert!(matches!(
            read_file(ws.path(), "nul-byte.bin"),
            Err(FileError::NotText)
        ));
        assert!(matches!(
            read_file(ws.path(), "latin1.txt"),
            Err(FileError::NotText)
        ));
    }

    #[test]
    fn write_rejects_oversized_text() {
        let ws = workspace();
        let text = "a".repeat(MAX_FILE_BYTES as usize + 1);
        assert!(matches!(
            write_file(ws.path(), "big.txt", &text, None),
            Err(FileError::TooLarge)
        ));
        assert!(!ws.path().join("big.txt").exists());
    }

    #[test]
    fn write_detects_conflict_and_writes_nothing() {
        let ws = workspace();
        write(ws.path(), "a.txt", b"one");
        let read = read_file(ws.path(), "a.txt").unwrap();
        write(ws.path(), "a.txt", b"two");

        let result = write_file(ws.path(), "a.txt", "mine", Some(&read.version));

        assert!(matches!(result, Err(FileError::Conflict)));
        assert_eq!(fs::read(ws.path().join("a.txt")).unwrap(), b"two");
    }

    #[test]
    fn save_updates_version_and_keeps_line_endings() {
        let ws = workspace();
        write(ws.path(), "a.txt", b"one\n");
        let read = read_file(ws.path(), "a.txt").unwrap();

        let saved = write_file(ws.path(), "a.txt", "two\r\nthree\n", Some(&read.version)).unwrap();

        assert_ne!(saved, read.version);
        assert_eq!(
            file_version(ws.path(), "a.txt").unwrap(),
            Some(saved.clone())
        );
        assert_eq!(
            fs::read(ws.path().join("a.txt")).unwrap(),
            b"two\r\nthree\n"
        );
        assert_eq!(read_file(ws.path(), "a.txt").unwrap().version, saved);
        assert!(matches!(
            write_file(ws.path(), "a.txt", "stale", Some(&read.version)),
            Err(FileError::Conflict)
        ));
        let leftovers = fs::read_dir(ws.path()).unwrap().count();
        assert_eq!(leftovers, 1);
    }

    #[test]
    fn write_creates_new_file_and_version_tracks_existence() {
        let ws = workspace();
        write(ws.path(), "src/keep.txt", b"");
        assert_eq!(file_version(ws.path(), "src/new.txt").unwrap(), None);
        assert_eq!(file_version(ws.path(), "missing/new.txt").unwrap(), None);

        let version = write_file(ws.path(), "src/new.txt", "hello", None).unwrap();

        assert_eq!(
            fs::read_to_string(ws.path().join("src/new.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            file_version(ws.path(), "src/new.txt").unwrap(),
            Some(version)
        );
    }

    #[test]
    fn write_with_expected_version_conflicts_when_file_was_deleted() {
        let ws = workspace();
        write(ws.path(), "a.txt", b"one");
        let read = read_file(ws.path(), "a.txt").unwrap();
        fs::remove_file(ws.path().join("a.txt")).unwrap();

        let result = write_file(ws.path(), "a.txt", "mine", Some(&read.version));

        assert!(matches!(result, Err(FileError::Conflict)));
        assert!(!ws.path().join("a.txt").exists());
    }

    fn git_name() -> String {
        [".", "git"].concat()
    }

    #[test]
    fn git_directory_is_unreachable_for_read_write_list_and_version() {
        let ws = workspace();
        let git = git_name();
        fs::create_dir_all(ws.path().join(&git).join("hooks")).unwrap();
        write(ws.path(), &format!("{git}/config"), b"[core]\n");
        let upper = git.to_uppercase();
        for rel in [
            format!("{git}/hooks/pre-commit"),
            format!("{upper}/config"),
            #[cfg(windows)]
            format!(r"{git}\hooks\post-checkout"),
        ] {
            assert!(
                matches!(
                    write_file(ws.path(), &rel, "echo pwned", None),
                    Err(FileError::GitDirectory)
                ),
                "{rel} was writable"
            );
        }
        let config = format!("{git}/config");
        assert!(matches!(
            read_file(ws.path(), &config),
            Err(FileError::GitDirectory)
        ));
        assert!(matches!(
            file_version(ws.path(), &config),
            Err(FileError::GitDirectory)
        ));
        assert!(matches!(
            list_dir(ws.path(), &git),
            Err(FileError::GitDirectory)
        ));
        assert!(
            !ws.path()
                .join(&git)
                .join("hooks")
                .join("pre-commit")
                .exists()
        );
    }

    #[test]
    fn worktree_git_file_cannot_be_read_or_rewritten() {
        let ws = workspace();
        let git = git_name();
        write(ws.path(), &git, b"gitdir: C:/real/worktrees/x\n");
        assert!(matches!(
            read_file(ws.path(), &git),
            Err(FileError::GitDirectory)
        ));
        assert!(matches!(
            write_file(ws.path(), &git, "gitdir: C:/evil\n", Some("any")),
            Err(FileError::GitDirectory)
        ));
        assert_eq!(
            fs::read(ws.path().join(&git)).unwrap(),
            b"gitdir: C:/real/worktrees/x\n"
        );
    }

    #[test]
    fn write_without_version_does_not_overwrite_existing_file() {
        let ws = workspace();
        write(ws.path(), "a.txt", b"claude's work");
        assert!(matches!(
            write_file(ws.path(), "a.txt", "", None),
            Err(FileError::Conflict)
        ));
        assert_eq!(fs::read(ws.path().join("a.txt")).unwrap(), b"claude's work");
        assert_eq!(fs::read_dir(ws.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn names_windows_would_not_normalise_are_rejected() {
        let ws = workspace();
        for rel in [
            "trail.",
            "space ",
            "dir./a.txt",
            "CONOUT$",
            "CONIN$",
            "COM\u{b9}",
            "LPT\u{b3}.txt",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
        ] {
            assert!(
                matches!(
                    write_file(ws.path(), rel, "x", None),
                    Err(FileError::InvalidName)
                ),
                "{rel} was not rejected"
            );
        }
        assert_eq!(fs::read_dir(ws.path()).unwrap().count(), 0);
    }

    #[test]
    fn write_to_root_or_directory_is_rejected_before_any_temp_file() {
        let ws = workspace();
        let root = ws.path().join("ws");
        fs::create_dir_all(root.join("src")).unwrap();
        for rel in ["", "src"] {
            assert!(
                matches!(
                    write_file(&root, rel, "payload", None),
                    Err(FileError::IsDirectory)
                ),
                "{rel:?} was not rejected"
            );
        }
        assert_eq!(fs::read_dir(ws.path()).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn list_dir_skips_non_utf8_names() {
        use std::os::windows::ffi::OsStringExt;
        let ws = workspace();
        let name = std::ffi::OsString::from_wide(&[0x61, 0xD800]);
        fs::write(ws.path().join(&name), b"real").unwrap();
        write(ws.path(), "b.txt", b"");
        assert_eq!(names(&list_dir(ws.path(), "").unwrap()), ["b.txt"]);
    }
}

#[cfg(all(test, unix))]
mod unix_path_tests {
    use super::*;
    #[test]
    fn unix_filenames_keep_backslashes_and_windows_reserved_characters() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["NUL", "a:b", "trail.", "space ", "a\\b", "a?b"] {
            write_file(dir.path(), name, "content", None).unwrap();
            assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap(), "content");
        }
    }
}
