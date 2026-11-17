use std::process::Stdio;

use orchestrator_core::{files, fonts, git, state};
use serde::Serialize;
use tauri::AppHandle;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::{CtxExt, blocking};

// `code` is a .cmd script, so cmd.exe parses its arguments; refuse anything it treats specially.
const CMD_SPECIAL: &[char] = &['"', '%', '^', '&', '|', '<', '>', '!', '\r', '\n'];

fn cmd_safe(arg: &str) -> bool {
    !arg.contains(CMD_SPECIAL) && !arg.chars().any(char::is_control)
}

/// Opens the workspace folder in VS Code, optionally at a file and line.
#[tauri::command]
#[specta::specta]
pub async fn open_in_vscode(
    app: AppHandle,
    workspace_id: String,
    rel_path: Option<String>,
    line: Option<u32>,
) -> AppResult<()> {
    let root = app.state_ctx().root(&workspace_id)?;
    blocking(move || {
        let mut args = vec![root.to_string_lossy().into_owned()];
        if let Some(rel_path) = rel_path {
            let file = state::without_verbatim_prefix(&files::resolve(&root, &rel_path)?);
            let mut target = file.to_string_lossy().into_owned();
            if let Some(line) = line {
                target.push_str(&format!(":{line}"));
            }
            args.extend(["-g".to_owned(), target]);
        }
        if cfg!(windows) && !args.iter().all(|arg| cmd_safe(arg)) {
            return Err(AppError::new(
                ErrorKind::InvalidArgument,
                "the path contains characters that cannot be passed to VS Code safely",
            ));
        }
        let env = orchestrator_core::environment::environment();
        let code = which::which_in("code", env.get(std::ffi::OsStr::new("PATH")), &root)
            .map_err(|_| AppError::new(ErrorKind::ToolMissing, "VS Code ('code') is not on PATH"))?;
        git::hidden_command(code)
            .args(&args)
            .env_clear()
            .envs(env)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| AppError::new(ErrorKind::Io, error.to_string()))?;
        Ok(())
    })
    .await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolCheck {
    pub claude: bool,
    pub git: bool,
    pub code: bool,
}

/// First-run check: which of the tools the app relies on are on PATH.
#[tauri::command]
#[specta::specta]
pub async fn check_tools() -> AppResult<ToolCheck> {
    blocking(|| {
        Ok(ToolCheck {
            claude: tool_exists("claude"),
            git: tool_exists("git"),
            code: tool_exists("code"),
        })
    })
    .await
}

/// An installed Nerd Font family for the terminal, or `None` when there is none.
#[tauri::command]
#[specta::specta]
pub async fn detect_nerd_font() -> AppResult<Option<String>> {
    blocking(|| Ok(fonts::nerd_font_family(&fonts::installed_font_names()))).await
}

fn tool_exists(program: &str) -> bool {
    let env = orchestrator_core::environment::environment();
    which::which_in(program, env.get(std::ffi::OsStr::new("PATH")), std::env::current_dir().unwrap_or_default()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_safe_accepts_ordinary_paths_and_rejects_cmd_metacharacters() {
        assert!(cmd_safe(r"C:\repos\my repo (1)\src\main.rs:42"));
        assert!(cmd_safe("-g"));
        for hostile in ["a&calc", "a|b", "%PATH%", "a^b", "a\"b", "a<b", "a>b", "!x!", "a\nb", "a\tb"] {
            assert!(!cmd_safe(hostile), "{hostile:?} accepted");
        }
    }
}
