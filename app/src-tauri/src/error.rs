use orchestrator_core::files::FileError;
use orchestrator_core::git::GitError;
use orchestrator_core::state::StateError;
use orchestrator_core::terminal::TerminalError;
use orchestrator_core::worktrees::WorktreeError;
use serde::Serialize;

/// The one error shape the UI sees: a stable `kind` to branch on and a message to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    NotFound,
    StateUnreadable,
    InvalidArgument,
    OutsideWorkspace,
    TooLarge,
    NotText,
    Conflict,
    DuplicateWorkspace,
    NotADirectory,
    Git,
    TimedOut,
    TerminalExited,
    WorktreeBlocked,
    NotRemovable,
    ToolMissing,
    Io,
    Internal,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

fn from(kind: ErrorKind, error: &impl ToString) -> AppError {
    AppError::new(kind, error.to_string())
}

impl From<StateError> for AppError {
    fn from(error: StateError) -> Self {
        let kind = match &error {
            StateError::NoLocalAppData | StateError::Json(_) => ErrorKind::StateUnreadable,
            StateError::NotADirectory => ErrorKind::NotADirectory,
            StateError::UnsupportedPath => ErrorKind::InvalidArgument,
            StateError::DuplicateWorkspace { .. } => ErrorKind::DuplicateWorkspace,
            StateError::Io(_) => ErrorKind::Io,
        };
        from(kind, &error)
    }
}

impl From<FileError> for AppError {
    fn from(error: FileError) -> Self {
        let kind = match &error {
            FileError::OutsideWorkspace | FileError::InvalidName | FileError::GitDirectory => {
                ErrorKind::OutsideWorkspace
            }
            FileError::IsDirectory => ErrorKind::InvalidArgument,
            FileError::TooLarge => ErrorKind::TooLarge,
            FileError::NotText => ErrorKind::NotText,
            FileError::Conflict => ErrorKind::Conflict,
            FileError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => ErrorKind::NotFound,
            FileError::Io(_) | FileError::Walk(_) => ErrorKind::Io,
        };
        from(kind, &error)
    }
}

impl From<GitError> for AppError {
    fn from(error: GitError) -> Self {
        let kind = match &error {
            GitError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => ErrorKind::ToolMissing,
            GitError::Io(_) => ErrorKind::Io,
            GitError::TimedOut => ErrorKind::TimedOut,
            GitError::TooLarge => ErrorKind::TooLarge,
            GitError::NotUtf8 => ErrorKind::NotText,
            GitError::EmptyMessage | GitError::UnknownRevision(_) => ErrorKind::InvalidArgument,
            GitError::Failed(_) | GitError::DetachedHead | GitError::UnexpectedOutput => ErrorKind::Git,
        };
        from(kind, &error)
    }
}

impl From<WorktreeError> for AppError {
    fn from(error: WorktreeError) -> Self {
        let kind = match error {
            WorktreeError::Git(git) => return git.into(),
            WorktreeError::Io(_) => ErrorKind::Io,
            WorktreeError::InvalidBranch(_) | WorktreeError::UnnamedRepoFolder => ErrorKind::InvalidArgument,
            WorktreeError::TargetExists(_) | WorktreeError::RelativeRoot => ErrorKind::InvalidArgument,
            WorktreeError::UnknownWorktree
            | WorktreeError::MainWorktree
            | WorktreeError::ContainsWorktree
            | WorktreeError::HasSubmodules => ErrorKind::NotRemovable,
            WorktreeError::Blocked(_) => ErrorKind::WorktreeBlocked,
        };
        from(kind, &error)
    }
}

impl From<TerminalError> for AppError {
    fn from(error: TerminalError) -> Self {
        let kind = match &error {
            TerminalError::NotFound(_) => ErrorKind::NotFound,
            TerminalError::Exited(_) => ErrorKind::TerminalExited,
            TerminalError::MissingDirectory(_) => ErrorKind::NotADirectory,
            TerminalError::UnsafeBatchArgument(_) => ErrorKind::InvalidArgument,
            TerminalError::Pty(_) | TerminalError::Job(_) | TerminalError::Io(_) => ErrorKind::Io,
        };
        from(kind, &error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::terminal::TerminalId;
    use orchestrator_core::worktrees::{Blocker, BlockerKind};
    use std::io;

    fn kind(error: impl Into<AppError>) -> ErrorKind {
        error.into().kind
    }

    #[test]
    fn core_errors_map_to_stable_kinds() {
        assert_eq!(kind(FileError::Conflict), ErrorKind::Conflict);
        assert_eq!(kind(FileError::InvalidName), ErrorKind::OutsideWorkspace);
        assert_eq!(kind(FileError::Io(io::ErrorKind::NotFound.into())), ErrorKind::NotFound);
        assert_eq!(kind(GitError::Io(io::ErrorKind::NotFound.into())), ErrorKind::ToolMissing);
        assert_eq!(kind(GitError::EmptyMessage), ErrorKind::InvalidArgument);
        assert_eq!(kind(GitError::TimedOut), ErrorKind::TimedOut);
        assert_eq!(kind(FileError::GitDirectory), ErrorKind::OutsideWorkspace);
        assert_eq!(kind(WorktreeError::HasSubmodules), ErrorKind::NotRemovable);
        assert_eq!(kind(TerminalError::UnsafeBatchArgument("a&b".into())), ErrorKind::InvalidArgument);
        assert_eq!(kind(WorktreeError::Git(GitError::DetachedHead)), ErrorKind::Git);
        assert_eq!(kind(WorktreeError::MainWorktree), ErrorKind::NotRemovable);
        assert_eq!(kind(TerminalError::Exited(TerminalId(3))), ErrorKind::TerminalExited);
        assert_eq!(
            kind(StateError::DuplicateWorkspace { existing_id: "w".into() }),
            ErrorKind::DuplicateWorkspace
        );
    }

    #[test]
    fn blocked_worktree_keeps_the_core_message() {
        let error = AppError::from(WorktreeError::Blocked(vec![Blocker {
            kind: BlockerKind::UntrackedFiles,
            count: 2,
        }]));
        assert_eq!(error.kind, ErrorKind::WorktreeBlocked);
        assert_eq!(error.message, "the worktree has work that would be lost");
    }

    #[test]
    fn serialises_kind_as_a_camel_case_string() {
        let json = serde_json::to_string(&AppError::new(ErrorKind::StateUnreadable, "x")).unwrap_or_default();
        assert_eq!(json, r#"{"kind":"stateUnreadable","message":"x"}"#);
    }
}
