//! Prints every process as JSON (`pid`, `ppid`, `exe`, `created`, `running`) so the @ui suite can find what
//! the app started. `created` (a FILETIME, 0 when unreadable) tells a real child from one whose parent id was reused.

#[cfg(windows)]
use serde_json::{Value, json};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, FILETIME};
#[cfg(windows)]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[cfg(windows)]
const STILL_ACTIVE: u32 = 259;

/// Start time (0 when unreadable) and whether the process is still running. A terminated process
/// stays listed while anything holds a handle to it.
#[cfg(windows)]
fn details(pid: u32) -> (u64, bool) {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (0, true);
        };
        let (mut creation, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let read = GetProcessTimes(
            process,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
        .is_ok();
        let mut code = STILL_ACTIVE;
        let running = GetExitCodeProcess(process, &raw mut code).is_err() || code == STILL_ACTIVE;
        let _ = CloseHandle(process);
        let created = if read {
            (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime)
        } else {
            0
        };
        (created, running)
    }
}

#[cfg(windows)]
pub fn list() -> Vec<Value> {
    let mut rows = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).expect("process snapshot");
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snapshot, &raw mut entry).is_ok();
        while more {
            let length = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(0);
            let (created, running) = details(entry.th32ProcessID);
            rows.push(json!({
                "pid": entry.th32ProcessID,
                "ppid": entry.th32ParentProcessID,
                "exe": String::from_utf16_lossy(&entry.szExeFile[..length]),
                "created": created,
                "running": running,
            }));
            more = Process32NextW(snapshot, &raw mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    rows
}

#[cfg(unix)]
pub fn list() -> Vec<serde_json::Value> {
    snapshot().unwrap_or_default()
}

#[cfg(unix)]
pub(crate) fn snapshot() -> std::io::Result<Vec<serde_json::Value>> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pgid=,stat=,comm="])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("process listing failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        let pid = fields.next()?.parse::<u32>().ok()?;
        let ppid = fields.next()?.parse::<u32>().ok()?;
        let group = fields.next()?.parse::<i32>().ok()?;
        let state = fields.next()?;
        let exe = fields.collect::<Vec<_>>().join(" ");
        Some(serde_json::json!({"pid": pid, "ppid": ppid,
            "exe": std::path::Path::new(&exe).file_name()?.to_string_lossy(),
            "group": group, "session": unsafe { libc::getsid(pid as i32) }, "created": 0, "running": !state.starts_with('Z')}))
    }).collect())
}
