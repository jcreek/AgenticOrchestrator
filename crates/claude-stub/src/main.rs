//! A stand-in for `claude` that the @ui BDD suite puts first on the app's PATH.
//! Each session's reported state comes from control files under `AO_STUB_CLAUDE_DIR`; see docs/testing.md.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, process, thread};

use serde_json::{Value, json};

const DIR_VARIABLE: &str = "AO_STUB_CLAUDE_DIR";
const CONTROL_POLL: Duration = Duration::from_millis(100);
const HANG: Duration = Duration::from_secs(120);

fn main() {
    let Some(dir) = env::var_os(DIR_VARIABLE).map(PathBuf::from) else {
        eprintln!("stub claude: {DIR_VARIABLE} is not set");
        process::exit(2);
    };
    let args: Vec<String> = env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["agents", "--json"] => agents(&dir),
        [flag @ ("--session-id" | "--resume"), id, ..] => interactive(&dir, flag, id),
        _ => {
            eprintln!("stub claude: unsupported arguments {args:?}");
            process::exit(2);
        }
    }
}

/// Prints what `claude agents --json` prints for interactive sessions.
fn agents(dir: &Path) {
    match fs::read_to_string(dir.join("agents-mode")).unwrap_or_default().trim() {
        "fail" => {
            eprintln!("stub claude: agents query failed");
            process::exit(1);
        }
        "hang" => thread::sleep(HANG),
        _ => {}
    }
    let mut entries = Vec::new();
    for file in fs::read_dir(dir.join("live")).into_iter().flatten().flatten() {
        let Some(live) = read_json(&file.path()) else {
            continue;
        };
        let id = live["sessionId"].as_str().unwrap_or_default();
        let control = control(dir, id);
        if control["listed"] == false {
            continue;
        }
        let mut entry = json!({
            "pid": live["pid"],
            "cwd": live["cwd"],
            "kind": "interactive",
            "startedAt": "2026-11-16T09:00:00Z",
            "sessionId": id,
            "name": "stub",
            "status": control.get("status").cloned().unwrap_or_else(|| "idle".into()),
        });
        if let Some(waiting_for) = control.get("waitingFor") {
            entry["waitingFor"] = waiting_for.clone();
        }
        entries.push(entry);
    }
    println!("{}", Value::Array(entries));
}

/// Runs like an interactive session until its control file asks it to exit.
fn interactive(dir: &Path, flag: &str, id: &str) {
    let mode = if flag == "--resume" { "resume" } else { "new" };
    let cwd = env::current_dir().map(|path| path.display().to_string()).unwrap_or_default();
    let live = dir.join("live").join(format!("{id}.json"));
    let registered = fs::create_dir_all(dir.join("live"))
        .and_then(|()| fs::write(&live, json!({ "sessionId": id, "cwd": cwd, "pid": process::id() }).to_string()))
        .and_then(|()| {
            let mut log = OpenOptions::new().create(true).append(true).open(dir.join("launches.log"))?;
            writeln!(log, "{mode} {id} {cwd}")
        });
    if let Err(error) = registered {
        eprintln!("stub claude: cannot write to {}: {error}", dir.display());
        process::exit(2);
    }
    raw_console_input();
    say(&format!("Stub Claude Code\r\n{mode} session {id}\r\n> "));
    thread::spawn(echo_input);

    let mut shown = Value::Null;
    loop {
        let control = control(dir, id);
        if let Some(code) = control["exit"].as_i64() {
            let _ = fs::remove_file(&live);
            process::exit(i32::try_from(code).unwrap_or(1));
        }
        // Printing on every change is what wakes the app's status poller, as real output would.
        if control != shown {
            say(&format!("\r\n[stub] state {control}\r\n> "));
            shown = control;
        }
        thread::sleep(CONTROL_POLL);
    }
}

fn echo_input() {
    let mut stdin = std::io::stdin();
    let mut buffer = [0u8; 256];
    while let Ok(read @ 1..) = stdin.read(&mut buffer) {
        let mut echo = String::new();
        for &byte in &buffer[..read] {
            match byte {
                0x17 => echo.push_str("<Ctrl+W>"),
                b'\r' => echo.push_str("\r\n> "),
                0x20..=0x7e => echo.push(char::from(byte)),
                _ => {}
            }
        }
        say(&echo);
    }
}

/// Delivers each key as it is pressed, as Claude Code's own TUI does.
#[cfg(windows)]
fn raw_console_input() {
    use windows::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, GetConsoleMode, GetStdHandle,
        STD_INPUT_HANDLE, SetConsoleMode,
    };
    unsafe {
        let Ok(input) = GetStdHandle(STD_INPUT_HANDLE) else {
            return;
        };
        let mut mode = CONSOLE_MODE::default();
        if GetConsoleMode(input, &raw mut mode).is_ok() {
            let _ = SetConsoleMode(input, mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT));
        }
    }
}

fn say(text: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

/// The session's own control file, else `control\default.json`, else nothing (listed and idle).
fn control(dir: &Path, id: &str) -> Value {
    let folder = dir.join("control");
    read_json(&folder.join(format!("{id}.json")))
        .or_else(|| read_json(&folder.join("default.json")))
        .unwrap_or_else(|| json!({}))
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[cfg(unix)]
fn raw_console_input() {
    unsafe {
        let mut mode = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut mode) == 0 {
            libc::cfmakeraw(&mut mode);
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &mode);
        }
    }
}
