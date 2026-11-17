use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[cfg(windows)]
#[path = "terminal/job_windows.rs"]
mod job;
#[cfg(unix)]
#[path = "terminal/job_unix.rs"]
mod job;
use job::{Job, join_job};

pub fn default_shell(env: &Environment) -> String {
    #[cfg(windows)]
    { let _ = env; "pwsh.exe".into() }
    #[cfg(unix)]
    {
        env.get(OsStr::new("SHELL")).filter(|shell| {
            Path::new(shell).is_absolute() && executable(Path::new(shell))
        }).map(|shell| shell.to_string_lossy().into_owned()).unwrap_or_else(|| {
            if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" }.into()
        })
    }
}
/// Exit code reported for a session ended by [`TerminalManager::kill`] or
/// [`TerminalManager::remove`]. Spells "KILL" in ASCII so it cannot be mistaken for a program's own.
pub const KILLED_EXIT_CODE: u32 = 0x4B49_4C4C;
const CLAUDE_PROGRAM: &str = "claude";
const SCROLLBACK_BYTES: usize = 2 * 1024 * 1024;
const READ_BUFFER_BYTES: usize = 64 * 1024;
const COALESCE_WINDOW: Duration = Duration::from_millis(8);
const COALESCE_MAX_BYTES: usize = 64 * 1024;
const UNKNOWN_EXIT_CODE: u32 = u32::MAX;
#[cfg(windows)]
const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";
#[cfg(windows)]
const CURSOR_POSITION_REPLY: &[u8] = b"\x1b[1;1R";
#[cfg(windows)]
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
const BATCH_METACHARACTERS: &[char] = &['"', '&', '|', '<', '>', '^', '%', '!', '\r', '\n'];
const TRACKED_MODES: &[u16] = &[25, 1000, 1002, 1003, 1004, 1006, 2004, 9001];
const MODE_SEQUENCE_LIMIT: usize = 32;

const STRIPPED_VARIABLES: &[&str] = &[
    "NO_COLOR",
    "FORCE_COLOR",
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "COLORTERM",
    "WT_SESSION",
    "WT_PROFILE_ID",
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
];

/// Values a Claude Code session injects into its tools' environment; other values are the user's.
const INJECTED_VARIABLES: &[(&str, &str)] = &[("GIT_EDITOR", "true"), ("GIT_TERMINAL_PROMPT", "0")];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct TerminalId(#[cfg_attr(feature = "specta", specta(type = u32))] pub u64);

impl fmt::Display for TerminalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

pub type Environment = BTreeMap<OsString, OsString>;
/// Receives a session's output. It runs while that session's output lock is held, so it must
/// not call back into the [`TerminalManager`]: doing so deadlocks the session.
pub type OutputSink = Box<dyn Fn(&[u8]) + Send>;
type ExitHandler = Arc<dyn Fn(TerminalId, u32) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub size: TerminalSize,
    pub env: Environment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeLaunch {
    New(Uuid),
    Resume(Uuid),
}

#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("no terminal with id {0}")]
    NotFound(TerminalId),
    #[error("terminal {0} has exited")]
    Exited(TerminalId),
    #[error("working directory {0} does not exist")]
    MissingDirectory(PathBuf),
    #[error("argument {0:?} cannot be passed safely to a batch file")]
    UnsafeBatchArgument(String),
    #[error("pseudo console failed: {0}")]
    Pty(String),
    #[cfg(windows)]
    #[error("job object failed: {0}")]
    Job(#[from] job::Error),
    #[error("terminal I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Passes the parent environment through except variables describing whatever terminal or
/// Claude Code session launched the app, then declares a 256-colour, truecolor terminal.
pub fn child_environment(parent: impl IntoIterator<Item = (OsString, OsString)>) -> Environment {
    let mut env: Environment = parent
        .into_iter()
        .filter(|(name, value)| !is_stripped(name, value))
        .collect();
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    env
}

fn is_stripped(name: &OsStr, value: &OsStr) -> bool {
    let name = name.to_string_lossy();
    STRIPPED_VARIABLES
        .iter()
        .any(|stripped| name.eq_ignore_ascii_case(stripped))
        || INJECTED_VARIABLES.iter().any(|(injected, injected_value)| {
            name.eq_ignore_ascii_case(injected) && value == OsStr::new(injected_value)
        })
}

pub fn shell_spec(
    program: Option<&str>,
    cwd: PathBuf,
    size: TerminalSize,
    env: Environment,
) -> TerminalSpec {
    TerminalSpec {
        program: program.map(str::to_owned).unwrap_or_else(|| default_shell(&env)),
        args: Vec::new(),
        cwd,
        size,
        env,
    }
}

pub fn claude_spec(
    launch: ClaudeLaunch,
    extra_args: &[String],
    cwd: PathBuf,
    size: TerminalSize,
    env: Environment,
) -> TerminalSpec {
    let (flag, session_id) = match launch {
        ClaudeLaunch::New(id) => ("--session-id", id),
        ClaudeLaunch::Resume(id) => ("--resume", id),
    };
    let mut args = vec![flag.to_owned(), session_id.to_string()];
    args.extend_from_slice(extra_args);
    TerminalSpec {
        program: CLAUDE_PROGRAM.to_owned(),
        args,
        cwd,
        size,
        env,
    }
}

/// Adds a starting prompt for a new Claude session after `--`, so it is never read as an option.
pub fn append_prompt(spec: &mut TerminalSpec, prompt: &str) {
    if !prompt.trim().is_empty() {
        spec.args.extend(["--".to_owned(), prompt.to_owned()]);
    }
}

/// Owns every terminal session. Sessions outlive their listeners: output is kept in a capped
/// scrollback until [`TerminalManager::remove`], and dropping the manager kills every process tree.
pub struct TerminalManager {
    next_id: AtomicU64,
    sessions: Mutex<HashMap<TerminalId, Arc<Session>>>,
    on_exit: ExitHandler,
}

impl TerminalManager {
    /// `on_exit` receives the id and exit code once the process has exited and all of its
    /// output has been delivered.
    pub fn new(on_exit: impl Fn(TerminalId, u32) + Send + Sync + 'static) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            on_exit: Arc::new(on_exit),
        }
    }

    pub fn spawn(&self, spec: TerminalSpec) -> Result<TerminalId, TerminalError> {
        if !spec.cwd.is_dir() {
            return Err(TerminalError::MissingDirectory(spec.cwd));
        }
        let program = resolve_program(&spec.program, &spec.env);
        // cmd.exe runs a batch file and re-parses its arguments; no quoting makes these safe.
        if is_batch_file(&program)
            && let Some(arg) = spec.args.iter().find(|arg| arg.contains(BATCH_METACHARACTERS))
        {
            return Err(TerminalError::UnsafeBatchArgument(arg.clone()));
        }
        let job = Job::new()?;
        let pair = native_pty_system()
            .openpty(pty_size(spec.size))
            .map_err(pty_error)?;

        let mut command = CommandBuilder::new(&program);
        command.args(&spec.args);
        command.cwd(&spec.cwd);
        command.env_clear();
        for (name, value) in &spec.env {
            command.env(name, value);
        }
        let reader = pair.master.try_clone_reader().map_err(pty_error)?;
        let writer = pair.master.take_writer().map_err(pty_error)?;
        let mut child = pair.slave.spawn_command(command).map_err(pty_error)?;
        drop(pair.slave);
        if let Err(error) = join_job(&job, child.as_mut()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }

        let id = TerminalId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let session = Arc::new(Session {
            id,
            pid: child.process_id().unwrap_or_default(),
            exit: Mutex::new(ExitState::Running),
            master: Mutex::new(Some(pair.master)),
            writer: Mutex::new(Some(writer)),
            job,
            output: Mutex::new(Output::default()),
        });
        lock(&self.sessions).insert(id, Arc::clone(&session));

        let (chunks, received) = mpsc::channel();
        let reading = Arc::clone(&session);
        thread::spawn(move || read_output(reader, &reading, &chunks));
        let delivering = Arc::clone(&session);
        let delivery = thread::spawn(move || deliver_output(&received, &delivering));
        let on_exit = Arc::clone(&self.on_exit);
        thread::spawn(move || {
            let exit_code = child
                .wait()
                .map_or(UNKNOWN_EXIT_CODE, |status| status.exit_code());
            let exit_code = {
                let mut state = lock(&session.exit);
                let code = if matches!(*state, ExitState::Killed) { KILLED_EXIT_CODE } else { exit_code };
                *state = ExitState::Exited;
                code
            };
            let _ = session.job.terminate();
            session.close_pty();
            let _ = delivery.join();
            on_exit(id, exit_code);
        });
        Ok(id)
    }

    /// Replays the scrollback into `sink`, then streams live output to it, replacing any
    /// previous sink. Replay and live output share one lock, so no bytes are lost or repeated.
    pub fn attach(&self, id: TerminalId, sink: OutputSink) -> Result<(), TerminalError> {
        lock(&self.session(id)?.output).attach(sink);
        Ok(())
    }

    pub fn detach(&self, id: TerminalId) -> Result<(), TerminalError> {
        lock(&self.session(id)?.output).sink = None;
        Ok(())
    }

    pub fn write(&self, id: TerminalId, bytes: &[u8]) -> Result<(), TerminalError> {
        self.session(id)?.write_input(bytes)
    }

    pub fn resize(&self, id: TerminalId, size: TerminalSize) -> Result<(), TerminalError> {
        let session = self.session(id)?;
        let master = lock(&session.master);
        let master = master.as_ref().ok_or(TerminalError::Exited(id))?;
        master.resize(pty_size(size)).map_err(pty_error)
    }

    pub fn kill(&self, id: TerminalId) -> Result<(), TerminalError> {
        self.session(id)?.kill()
    }

    pub fn process_id(&self, id: TerminalId) -> Result<u32, TerminalError> {
        Ok(self.session(id)?.pid)
    }

    /// Kills the session if it is still running and discards its scrollback.
    pub fn remove(&self, id: TerminalId) -> Result<(), TerminalError> {
        self.remove_session(id).map(drop)
    }

    /// Like [`Self::remove`], then waits up to `timeout` for every process of the session to end;
    /// true once none is left.
    pub fn remove_and_wait(&self, id: TerminalId, timeout: Duration) -> Result<bool, TerminalError> {
        let session = self.remove_session(id)?;
        let deadline = Instant::now() + timeout;
        while session.job.active_processes()? > 0 {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(true)
    }

    fn remove_session(&self, id: TerminalId) -> Result<Arc<Session>, TerminalError> {
        let session = lock(&self.sessions)
            .remove(&id)
            .ok_or(TerminalError::NotFound(id))?;
        let killed = session.kill();
        session.close_pty();
        killed.map(|()| session)
    }

    fn session(&self, id: TerminalId) -> Result<Arc<Session>, TerminalError> {
        lock(&self.sessions)
            .get(&id)
            .cloned()
            .ok_or(TerminalError::NotFound(id))
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        for session in lock(&self.sessions).values() {
            let _ = session.kill();
        }
    }
}

enum ExitState { Running, Killed, Exited }

struct Session {
    id: TerminalId,
    pid: u32,
    exit: Mutex<ExitState>,
    // Separate locks so a write blocked on a full input pipe cannot hold up resize or close.
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    job: Job,
    output: Mutex<Output>,
}

impl Session {
    fn kill(&self) -> Result<(), TerminalError> {
        let mut state = lock(&self.exit);
        if matches!(*state, ExitState::Exited) { return Ok(()); }
        self.job.terminate()?;
        *state = ExitState::Killed;
        Ok(())
    }

    fn write_input(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        let mut writer = lock(&self.writer);
        let writer = writer.as_mut().ok_or(TerminalError::Exited(self.id))?;
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    fn close_pty(&self) {
        // Closing the console fails any write blocked inside the writer lock, so it goes first,
        // with no lock held because it may wait for the reader thread to drain output.
        let master = lock(&self.master).take();
        drop(master);
        lock(&self.writer).take();
    }
}

#[derive(Default)]
struct Output {
    scrollback: VecDeque<u8>,
    trimmed: bool,
    modes: BTreeMap<u16, bool>,
    partial_sequence: Vec<u8>,
    sink: Option<OutputSink>,
}

impl Output {
    fn deliver(&mut self, bytes: &[u8]) {
        self.track_modes(bytes);
        self.scrollback.extend(bytes);
        let excess = self.scrollback.len().saturating_sub(SCROLLBACK_BYTES);
        if excess > 0 {
            self.scrollback.drain(..excess);
            self.trimmed = true;
        }
        if let Some(sink) = &self.sink
            && !feed(sink, bytes)
        {
            self.sink = None;
        }
    }

    fn attach(&mut self, sink: OutputSink) {
        // Trimming may have dropped the sequences that set these modes, so restate them first.
        let mut replay = if self.trimmed {
            self.mode_sequences()
        } else {
            Vec::new()
        };
        replay.extend_from_slice(self.scrollback.make_contiguous());
        let healthy = replay.is_empty() || feed(&sink, &replay);
        self.sink = healthy.then_some(sink);
    }

    fn track_modes(&mut self, bytes: &[u8]) {
        let joined;
        let mut rest = if self.partial_sequence.is_empty() {
            bytes
        } else {
            let mut partial = std::mem::take(&mut self.partial_sequence);
            partial.extend_from_slice(bytes);
            joined = partial;
            &joined[..]
        };
        while let Some(at) = rest.iter().position(|byte| *byte == 0x1b) {
            rest = &rest[at..];
            match scan_private_mode(rest) {
                ModeScan::Complete { len, params, set } => {
                    for mode in params.split(|byte| *byte == b';').filter_map(tracked_mode) {
                        self.modes.insert(mode, set);
                    }
                    rest = &rest[len..];
                }
                ModeScan::Incomplete => {
                    self.partial_sequence = rest.to_vec();
                    return;
                }
                ModeScan::Other => rest = &rest[1..],
            }
        }
    }

    fn mode_sequences(&self) -> Vec<u8> {
        self.modes
            .iter()
            .flat_map(|(mode, set)| format!("\x1b[?{mode}{}", if *set { 'h' } else { 'l' }).into_bytes())
            .collect()
    }
}

/// Returns false if the sink panicked.
fn feed(sink: &OutputSink, bytes: &[u8]) -> bool {
    panic::catch_unwind(AssertUnwindSafe(|| sink(bytes))).is_ok()
}

enum ModeScan<'a> {
    Complete { len: usize, params: &'a [u8], set: bool },
    Incomplete,
    Other,
}

/// Scans a DEC private mode set/reset (`ESC [ ? Pm h` or `l`) at the start of `sequence`.
fn scan_private_mode(sequence: &[u8]) -> ModeScan<'_> {
    const INTRODUCER: &[u8] = b"\x1b[?";
    for (index, &byte) in sequence.iter().enumerate().take(MODE_SEQUENCE_LIMIT) {
        match (index, byte) {
            (0..3, _) if byte != INTRODUCER[index] => return ModeScan::Other,
            (0..3, _) | (_, b'0'..=b'9' | b';') => {}
            (_, b'h' | b'l') => {
                return ModeScan::Complete {
                    len: index + 1,
                    params: &sequence[INTRODUCER.len()..index],
                    set: byte == b'h',
                };
            }
            _ => return ModeScan::Other,
        }
    }
    if sequence.len() < MODE_SEQUENCE_LIMIT {
        ModeScan::Incomplete
    } else {
        ModeScan::Other
    }
}

fn tracked_mode(param: &[u8]) -> Option<u16> {
    let mode = std::str::from_utf8(param).ok()?.parse().ok()?;
    TRACKED_MODES.contains(&mode).then_some(mode)
}

/// Finds a bare program name on the spec's PATH, trying only PATHEXT extensions: npm puts an
/// extensionless sh script beside its `.cmd` shim, and that script cannot be started.
#[cfg(windows)]
pub fn resolve_program(program: &str, env: &Environment) -> OsString {
    let path = Path::new(program);
    if path.extension().is_some() || path.components().count() > 1 {
        return program.into();
    }
    let variable = |name: &str| {
        env.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_os_str())
    };
    let extensions = variable("PATHEXT")
        .unwrap_or(OsStr::new(DEFAULT_PATHEXT))
        .to_string_lossy();
    for folder in std::env::split_paths(variable("PATH").unwrap_or_default()) {
        if !folder.is_absolute() {
            continue;
        }
        for extension in extensions.split(';').filter(|extension| !extension.is_empty()) {
            let candidate = folder.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return candidate.into_os_string();
            }
        }
    }
    program.into()
}

#[cfg(unix)]
pub fn resolve_program(program: &str, env: &Environment) -> OsString {
    if Path::new(program).components().count() > 1 { return program.into(); }
    for folder in std::env::split_paths(env.get(OsStr::new("PATH")).map(OsString::as_os_str).unwrap_or_default()) {
        let candidate = folder.join(program);
        if folder.is_absolute() && executable(&candidate) { return candidate.into_os_string(); }
    }
    program.into()
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn is_batch_file(program: &OsStr) -> bool {
    cfg!(windows) && Path::new(program).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
    })
}

fn read_output(mut reader: Box<dyn Read + Send>, _session: &Session, chunks: &Sender<Vec<u8>>) {
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    #[cfg(windows)]
    let mut answered_cursor_query = false;
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        #[allow(unused_mut)]
        let mut chunk = buffer[..read].to_vec();
        // ConPTY is opened with INHERIT_CURSOR and stalls until its startup query is answered.
        // Answering here keeps unattached sessions running and keeps the query out of replays.
        #[cfg(windows)]
        if !answered_cursor_query && let Some(at) = find(&chunk, CURSOR_POSITION_QUERY) {
            chunk.drain(at..at + CURSOR_POSITION_QUERY.len());
            let _ = _session.write_input(CURSOR_POSITION_REPLY);
            answered_cursor_query = true;
        }
        if !chunk.is_empty() && chunks.send(chunk).is_err() {
            break;
        }
    }
}

fn deliver_output(chunks: &Receiver<Vec<u8>>, session: &Session) {
    let mut last_sent: Option<Instant> = None;
    while let Ok(mut batch) = chunks.recv() {
        // After a quiet spell the chunk goes out at once so typing echo is not delayed.
        let batch_until = last_sent
            .map(|sent| sent + COALESCE_WINDOW)
            .filter(|deadline| *deadline > Instant::now());
        let drain = |batch: &mut Vec<u8>| {
            while batch.len() < COALESCE_MAX_BYTES
                && let Ok(more) = chunks.try_recv()
            {
                batch.extend_from_slice(&more);
            }
        };
        if let Some(deadline) = batch_until {
            drain(&mut batch);
            // Sleeping rather than waiting on the channel stops ConPTY's ~100-byte reads waking this thread each time.
            if batch.len() < COALESCE_MAX_BYTES {
                thread::sleep(deadline.saturating_duration_since(Instant::now()));
                drain(&mut batch);
            }
        }
        lock(&session.output).deliver(&batch);
        last_sent = Some(Instant::now());
    }
}

/// Kills a process and everything it has started when dropped.
pub struct ProcessTree {
    _job: Job,
}

impl ProcessTree {
    #[cfg(unix)]
    fn from_child(child: &std::process::Child) -> Result<Self, TerminalError> {
        let job = Job::new()?;
        job.set_group(child.id())?;
        Ok(Self { _job: job })
    }

    /// Spawn with process ownership established before Unix exec; dropping the tree kills descendants.
    pub fn spawn(command: &mut std::process::Command) -> Result<(std::process::Child, Self), TerminalError> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // setsid is async-signal-safe and creates a group isolated from the app.
            unsafe { command.pre_exec(|| {
                if libc::setsid() < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
            }); }
        }
        let mut child = command.spawn()?;
        #[cfg(unix)]
        let tree = Self::from_child(&child);
        #[cfg(windows)]
        let tree = Self::adopt(&child);
        match tree {
            Ok(tree) => Ok((child, tree)),
            Err(error) => { let _ = child.kill(); let _ = child.wait(); Err(error) }
        }
    }
    #[cfg(windows)]
    pub fn adopt(child: &std::process::Child) -> Result<Self, TerminalError> {
        let job = Job::new()?;
        job.assign_child(child)?;
        Ok(Self { _job: job })
    }
}

fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn pty_error(error: impl fmt::Display) -> TerminalError {
    TerminalError::Pty(format!("{error:#}"))
}

#[cfg(windows)]
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    const TIMEOUT: Duration = Duration::from_secs(20);
    const SIZE: TerminalSize = TerminalSize { cols: 100, rows: 30 };
    const ECHO_ARGS_SHIM: &str = "@echo off\r\necho args=[%*]\r\n";

    type Exits = Receiver<(TerminalId, u32)>;
    type Collected = Arc<Mutex<Vec<u8>>>;

    fn manager() -> (TerminalManager, Exits) {
        let (sender, exits) = mpsc::channel();
        let manager = TerminalManager::new(move |id, code| {
            let _ = sender.send((id, code));
        });
        (manager, exits)
    }

    fn collector() -> (OutputSink, Collected) {
        let collected = Collected::default();
        let target = Arc::clone(&collected);
        let sink = Box::new(move |bytes: &[u8]| target.lock().unwrap().extend_from_slice(bytes));
        (sink, collected)
    }

    fn text(collected: &Collected) -> String {
        String::from_utf8_lossy(&collected.lock().unwrap()).into_owned()
    }

    fn cmd(args: &[&str]) -> TerminalSpec {
        TerminalSpec {
            program: "cmd.exe".into(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cwd: std::env::temp_dir(),
            size: SIZE,
            env: child_environment(std::env::vars_os()),
        }
    }

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        condition()
    }

    fn expect_exit(exits: &Exits, id: TerminalId) -> u32 {
        let (exited, code) = exits.recv_timeout(TIMEOUT).expect("exit notification");
        assert_eq!(exited, id);
        code
    }

    struct ProcessEntry {
        pid: u32,
        parent: u32,
        exe: String,
    }

    fn processes() -> Vec<ProcessEntry> {
        let mut found = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).expect("snapshot");
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
            while more {
                let length = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(0);
                found.push(ProcessEntry {
                    pid: entry.th32ProcessID,
                    parent: entry.th32ParentProcessID,
                    exe: String::from_utf16_lossy(&entry.szExeFile[..length]),
                });
                more = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
        }
        found
    }

    fn is_running(pid: u32) -> bool {
        processes().iter().any(|process| process.pid == pid)
    }

    fn child_named(parent: u32, exe: &str) -> Option<u32> {
        processes()
            .into_iter()
            .find(|p| p.parent == parent && p.exe.eq_ignore_ascii_case(exe))
            .map(|p| p.pid)
    }

    fn spawn_ping_tree(manager: &TerminalManager) -> (TerminalId, u32, u32) {
        let id = manager
            .spawn(cmd(&["/c", "ping -n 60 127.0.0.1"]))
            .expect("spawn");
        let shell = manager.process_id(id).expect("pid");
        let mut ping = None;
        assert!(wait_until(|| {
            ping = child_named(shell, "PING.EXE");
            ping.is_some()
        }));
        (id, shell, ping.expect("ping pid"))
    }

    fn env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect()
    }

    fn prepend_path(env: &mut Environment, folder: &Path) {
        let key = env
            .keys()
            .find(|name| name.eq_ignore_ascii_case("PATH"))
            .cloned()
            .unwrap_or_else(|| "PATH".into());
        let mut path = folder.as_os_str().to_owned();
        path.push(";");
        path.push(env.get(&key).cloned().unwrap_or_default());
        env.insert(key, path);
    }

    fn shim_spec(folder: &Path, program: &str, args: &[&str]) -> TerminalSpec {
        let mut spec = cmd(args);
        spec.program = program.into();
        prepend_path(&mut spec.env, folder);
        spec
    }

    fn run_to_exit(spec: TerminalSpec) -> (u32, String) {
        let (manager, exits) = manager();
        let id = manager.spawn(spec).expect("spawn");
        let (sink, collected) = collector();
        manager.attach(id, sink).expect("attach");
        let code = expect_exit(&exits, id);
        (code, text(&collected))
    }

    /// The column count `mode con` printed last, read past ConPTY's cursor-movement sequences.
    fn reported_columns(output: &str) -> Option<u32> {
        let after = &output[output.rfind("Columns:")? + "Columns:".len()..];
        let line = &after[..after.find(['\r', '\n'])?];
        let mut digits = String::new();
        let mut in_escape = false;
        for character in line.chars() {
            match character {
                '\x1b' => in_escape = true,
                c if in_escape => in_escape = !c.is_ascii_alphabetic(),
                c if c.is_ascii_digit() => digits.push(c),
                _ => {}
            }
        }
        digits.parse().ok()
    }

    #[test]
    fn child_environment_strips_session_variables_in_any_case() {
        let child = child_environment(env(&[
            ("no_color", "1"),
            ("FORCE_COLOR", "0"),
            ("Term_Program", "vscode"),
            ("TERM_PROGRAM_VERSION", "1"),
            ("WT_SESSION", "x"),
            ("WT_PROFILE_ID", "x"),
            ("ClaudeCode", "1"),
            ("CLAUDE_CODE_ENTRYPOINT", "cli"),
            ("CLAUDE_CODE_SSE_PORT", "1234"),
            ("CLAUDE_CODE_SESSION_ID", "s"),
            ("CLAUDE_CODE_MESSAGING_TOKEN", "t"),
        ]));
        let names: Vec<_> = child.keys().cloned().collect();
        assert_eq!(names, vec![OsString::from("COLORTERM"), OsString::from("TERM")]);
    }

    #[test]
    fn child_environment_keeps_user_configuration() {
        let child = child_environment(env(&[
            ("Path", r"C:\Windows"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("CLAUDE_CODE_GIT_BASH_PATH", r"C:\git\bash.exe"),
            ("ANTHROPIC_MODEL", "m"),
            ("HTTPS_PROXY", "http://proxy"),
        ]));
        assert_eq!(child.len(), 7);
        assert_eq!(child[OsStr::new("CLAUDE_CODE_USE_BEDROCK")], "1");
        assert_eq!(child[OsStr::new("Path")], r"C:\Windows");
    }

    #[test]
    fn child_environment_strips_only_the_git_overrides_claude_code_injects() {
        let injected = child_environment(env(&[("GIT_EDITOR", "true"), ("git_terminal_prompt", "0")]));
        assert_eq!(injected.len(), 2);
        let user = child_environment(env(&[
            ("GIT_EDITOR", "code --wait"),
            ("GIT_TERMINAL_PROMPT", "1"),
        ]));
        assert_eq!(user[OsStr::new("GIT_EDITOR")], "code --wait");
        assert_eq!(user[OsStr::new("GIT_TERMINAL_PROMPT")], "1");
    }

    #[test]
    fn child_environment_replaces_terminal_type() {
        let child = child_environment(env(&[("term", "dumb"), ("COLORTERM", "24bit")]));
        assert_eq!(child.len(), 2);
        assert_eq!(child[OsStr::new("TERM")], "xterm-256color");
        assert_eq!(child[OsStr::new("COLORTERM")], "truecolor");
    }

    #[test]
    fn shell_spec_defaults_to_pwsh_and_accepts_another_program() {
        let cwd = PathBuf::from(r"C:\repo");
        let default = shell_spec(None, cwd.clone(), SIZE, Environment::new());
        assert_eq!(default.program, "pwsh.exe");
        assert!(default.args.is_empty());
        assert_eq!(default.cwd, cwd);
        let custom = shell_spec(Some("cmd.exe"), cwd, SIZE, Environment::new());
        assert_eq!(custom.program, "cmd.exe");
    }

    #[test]
    fn claude_spec_starts_or_resumes_a_session_with_extra_args() {
        let id = Uuid::new_v4();
        let extra = vec!["--model".to_owned(), "opus".to_owned()];
        let new = claude_spec(ClaudeLaunch::New(id), &extra, "C:\\r".into(), SIZE, Environment::new());
        assert_eq!(new.program, "claude");
        assert_eq!(new.args, ["--session-id", &id.to_string(), "--model", "opus"]);
        let resume = claude_spec(ClaudeLaunch::Resume(id), &[], "C:\\r".into(), SIZE, Environment::new());
        assert_eq!(resume.args, ["--resume", &id.to_string()]);
    }

    #[test]
    fn append_prompt_adds_one_operand_after_double_dash_and_skips_blank_prompts() {
        let id = Uuid::new_v4();
        let mut spec = claude_spec(ClaudeLaunch::New(id), &[], "C:\\r".into(), SIZE, Environment::new());
        append_prompt(&mut spec, "  ");
        assert_eq!(spec.args.len(), 2);
        append_prompt(&mut spec, "--help me & fix \"this\"");
        assert_eq!(spec.args, ["--session-id", &id.to_string(), "--", "--help me & fix \"this\""]);
    }

    #[test]
    fn spawned_output_reaches_the_attached_sink() {
        let (manager, exits) = manager();
        // The title shows the command line, so assert on the computed result instead.
        let id = manager.spawn(cmd(&["/c", "set /a 1234*1000"])).expect("spawn");
        let (sink, collected) = collector();
        manager.attach(id, sink).expect("attach");
        assert_eq!(expect_exit(&exits, id), 0);
        assert!(text(&collected).contains("1234000"), "{:?}", text(&collected));
    }

    #[test]
    fn second_attach_replays_exactly_what_the_first_sink_received() {
        let (manager, exits) = manager();
        let id = manager
            .spawn(cmd(&["/c", "for /l %i in (1,1,200) do @echo line-%i"]))
            .expect("spawn");
        let (first, first_seen) = collector();
        manager.attach(id, first).expect("attach");
        expect_exit(&exits, id);
        let (second, replayed) = collector();
        manager.attach(id, second).expect("reattach");
        assert!(text(&replayed).contains("line-200"));
        assert_eq!(find(&replayed.lock().unwrap(), CURSOR_POSITION_QUERY), None);
        assert_eq!(*first_seen.lock().unwrap(), *replayed.lock().unwrap());
    }

    #[test]
    fn replay_after_trim_restates_terminal_modes() {
        let mut output = Output::default();
        output.deliver(b"\x1b[?20");
        output.deliver(b"04h\x1b[?25l\x1b[?1004h\x1b[?1004");
        output.deliver(b"l\x1b[?1049h");
        output.deliver(&vec![b'x'; SCROLLBACK_BYTES]);
        let (sink, replay) = collector();
        output.attach(sink);
        let replay = replay.lock().unwrap();
        let modes: &[u8] = b"\x1b[?25l\x1b[?1004l\x1b[?2004h";
        assert_eq!(&replay[..modes.len()], modes);
        assert_eq!(replay.len(), modes.len() + SCROLLBACK_BYTES);
    }

    #[test]
    fn detached_sink_receives_nothing_more() {
        let (manager, exits) = manager();
        let id = manager.spawn(cmd(&[])).expect("spawn");
        let (sink, collected) = collector();
        manager.attach(id, sink).expect("attach");
        manager.detach(id).expect("detach");
        let before = collected.lock().unwrap().len();
        manager.write(id, b"echo more\rexit\r").expect("write");
        expect_exit(&exits, id);
        assert_eq!(collected.lock().unwrap().len(), before);
    }

    #[test]
    fn panicking_sink_is_dropped_and_the_session_keeps_running() {
        let (manager, exits) = manager();
        let id = manager.spawn(cmd(&[])).expect("spawn");
        let armed = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&armed);
        let sink = Box::new(move |_: &[u8]| assert!(!trigger.load(Ordering::SeqCst), "sink failed"));
        manager.attach(id, sink).expect("attach");
        armed.store(true, Ordering::SeqCst);
        // Far more output than the ConPTY pipe holds, so a stalled reader would block the shell.
        manager
            .write(id, b"for /l %i in (1,1,1000) do @echo padding-padding-%i\rexit 4\r")
            .expect("write");
        let (exited, code) = exits.recv_timeout(Duration::from_secs(60)).expect("exit notification");
        assert_eq!((exited, code), (id, 4));
        let (sink, replay) = collector();
        manager.attach(id, sink).expect("reattach");
        assert!(text(&replay).contains("padding-padding-1000"));
    }

    #[test]
    fn last_output_arrives_before_exit_under_load() {
        let (manager, exits) = manager();
        let mut sinks = HashMap::new();
        for _ in 0..20 {
            let id = manager
                .spawn(cmd(&["/c", "for /l %i in (1,1,1000) do @echo line-%i"]))
                .expect("spawn");
            let (sink, collected) = collector();
            manager.attach(id, sink).expect("attach");
            sinks.insert(id, collected);
        }
        for _ in 0..20 {
            let (id, code) = exits.recv_timeout(Duration::from_secs(120)).expect("exit");
            assert_eq!(code, 0);
            assert!(text(&sinks[&id]).contains("line-1000"), "{id}: last line missing at exit");
        }
    }

    #[test]
    fn exit_notification_carries_the_exit_code_and_scrollback_outlives_exit() {
        let (manager, exits) = manager();
        let id = manager.spawn(cmd(&["/c", "set /a 2468*1000 & exit 7"])).expect("spawn");
        assert_eq!(expect_exit(&exits, id), 7);
        let (sink, replayed) = collector();
        manager.attach(id, sink).expect("attach after exit");
        assert!(text(&replayed).contains("2468000"));
        assert!(matches!(manager.write(id, b"x"), Err(TerminalError::Exited(_))));
        manager.remove(id).expect("remove");
        assert!(matches!(manager.detach(id), Err(TerminalError::NotFound(_))));
    }

    #[test]
    fn input_and_resize_reach_an_interactive_shell() {
        let (manager, exits) = manager();
        let id = manager.spawn(cmd(&[])).expect("spawn");
        let (sink, collected) = collector();
        manager.attach(id, sink).expect("attach");
        manager
            .resize(id, TerminalSize { cols: 120, rows: 40 })
            .expect("resize");
        manager.write(id, b"mode con\r").expect("write");
        assert!(wait_until(|| reported_columns(&text(&collected)).is_some()));
        assert_eq!(reported_columns(&text(&collected)), Some(120), "{:?}", text(&collected));
        manager.write(id, b"exit 3\r").expect("write");
        assert_eq!(expect_exit(&exits, id), 3);
    }

    #[test]
    fn unusable_resize_is_refused_and_the_session_keeps_working() {
        let (manager, exits) = manager();
        let id = manager.spawn(cmd(&[])).expect("spawn");
        let huge = TerminalSize { cols: u16::MAX, rows: u16::MAX };
        assert!(manager.resize(id, huge).is_err());
        let _ = manager.resize(id, TerminalSize { cols: 0, rows: 0 });
        let (sink, collected) = collector();
        manager.attach(id, sink).expect("attach");
        manager.write(id, b"set /a 5151*1000\r").expect("write");
        assert!(wait_until(|| text(&collected).contains("5151000")));
        manager.write(id, b"exit\r").expect("write");
        expect_exit(&exits, id);
    }

    #[test]
    fn large_paste_does_not_delay_resize_or_exit() {
        let (manager, exits) = manager();
        let manager = Arc::new(manager);
        let (id, _, _) = spawn_ping_tree(&manager);
        let writer = Arc::clone(&manager);
        // ConPTY takes unread input at about 2 MB/s, so this write blocks for several seconds.
        let paste = thread::spawn(move || writer.write(id, &vec![b'a'; 16 * 1024 * 1024]));
        thread::sleep(Duration::from_millis(200));
        let started = Instant::now();
        manager
            .resize(id, TerminalSize { cols: 90, rows: 20 })
            .expect("resize");
        let resize_took = started.elapsed();
        manager.kill(id).expect("kill");
        assert_eq!(expect_exit(&exits, id), KILLED_EXIT_CODE);
        let exit_took = started.elapsed();
        let _ = paste.join();
        assert!(resize_took < Duration::from_secs(2), "resize took {resize_took:?}");
        assert!(exit_took < Duration::from_secs(5), "exit took {exit_took:?}");
    }

    #[test]
    fn spawn_rejects_a_missing_working_directory() {
        let (manager, _exits) = manager();
        let mut spec = cmd(&["/c", "exit"]);
        spec.cwd = PathBuf::from(r"C:\does\not\exist\ao-terminal-test");
        assert!(matches!(manager.spawn(spec), Err(TerminalError::MissingDirectory(_))));
    }

    #[test]
    fn unknown_program_fails_to_spawn() {
        let (manager, _exits) = manager();
        let mut spec = cmd(&[]);
        spec.program = "ao-definitely-not-a-program".into();
        assert!(manager.spawn(spec).is_err());
    }

    #[test]
    fn kill_terminates_the_whole_process_tree() {
        let (manager, exits) = manager();
        let (id, shell, ping) = spawn_ping_tree(&manager);
        manager.kill(id).expect("kill");
        assert_eq!(expect_exit(&exits, id), KILLED_EXIT_CODE);
        assert!(wait_until(|| !is_running(ping) && !is_running(shell)));
    }

    #[test]
    fn session_exit_kills_background_processes() {
        let (manager, exits) = manager();
        let mut spec = cmd(&[]);
        spec.program = "powershell.exe".into();
        spec.args = vec![
            "-NoProfile".into(),
            "-Command".into(),
            "Start-Process ping -ArgumentList '-n','30','127.0.0.1' -WindowStyle Hidden; Start-Sleep 1".into(),
        ];
        let id = manager.spawn(spec).expect("spawn");
        let shell = manager.process_id(id).expect("pid");
        let mut ping = None;
        assert!(wait_until(|| {
            ping = child_named(shell, "PING.EXE");
            ping.is_some()
        }));
        expect_exit(&exits, id);
        let ping = ping.expect("ping pid");
        assert!(wait_until(|| !is_running(ping)), "background ping outlived its session");
    }

    #[test]
    fn remove_and_wait_returns_once_the_whole_tree_has_ended() {
        let (manager, _exits) = manager();
        let (id, shell, ping) = spawn_ping_tree(&manager);
        assert!(manager.remove_and_wait(id, TIMEOUT).expect("remove"));
        assert!(wait_until(|| !is_running(ping) && !is_running(shell)));
        assert!(matches!(manager.remove_and_wait(id, TIMEOUT), Err(TerminalError::NotFound(_))));
    }

    #[test]
    fn dropping_a_process_tree_kills_its_descendants() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 60 127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        let tree = ProcessTree::adopt(&child).expect("adopt");
        let mut ping = None;
        assert!(wait_until(|| {
            ping = child_named(child.id(), "PING.EXE");
            ping.is_some()
        }));
        drop(tree);
        let ping = ping.expect("ping pid");
        assert!(wait_until(|| !is_running(ping)), "ping outlived its tree");
        let _ = child.wait();
    }

    #[test]
    fn dropping_the_manager_kills_every_process_tree() {
        let (manager, _exits) = manager();
        let (_, first_shell, first_ping) = spawn_ping_tree(&manager);
        let (_, second_shell, second_ping) = spawn_ping_tree(&manager);
        drop(manager);
        let pids = [first_shell, first_ping, second_shell, second_ping];
        assert!(wait_until(|| !pids.iter().any(|pid| is_running(*pid))));
    }

    #[test]
    fn cmd_shim_found_on_path_is_spawned() {
        let folder = tempfile::tempdir().expect("tempdir");
        std::fs::write(folder.path().join("ao-shim.cmd"), "@echo off\r\necho shim-%~n0-ran\r\n")
            .expect("write shim");
        let (code, out) = run_to_exit(shim_spec(folder.path(), "ao-shim", &[]));
        assert_eq!(code, 0);
        assert!(out.contains("shim-ao-shim-ran"), "{out:?}");
    }

    /// npm installs `claude` (a sh script), `claude.cmd` and `claude.ps1` side by side.
    #[test]
    fn extensionless_script_beside_a_cmd_shim_resolves_to_the_shim() {
        let folder = tempfile::tempdir().expect("tempdir");
        std::fs::write(folder.path().join("ao-npm"), "#!/bin/sh\nexec node cli.js \"$@\"\n").unwrap();
        std::fs::write(folder.path().join("ao-npm.cmd"), "@echo off\r\necho npm-shim-ran\r\n").unwrap();
        let (_, out) = run_to_exit(shim_spec(folder.path(), "ao-npm", &[]));
        assert!(out.contains("npm-shim-ran"), "{out:?}");
    }

    #[test]
    fn coalescer_drains_a_backlog_without_sleeping_per_batch() {
        let session = Session {
            id: TerminalId(0),
            pid: 0,
            exit: Mutex::new(ExitState::Running),
            master: Mutex::new(None),
            writer: Mutex::new(None),
            job: Job::new().expect("job"),
            output: Mutex::new(Output::default()),
        };
        let (sink, collected) = collector();
        lock(&session.output).attach(sink);
        let (chunks, received) = mpsc::channel();
        let total = 16 * 1024 * 1024;
        for _ in 0..total / (16 * 1024) {
            chunks.send(vec![b'x'; 16 * 1024]).expect("queue");
        }
        drop(chunks);
        let started = Instant::now();
        deliver_output(&received, &session);
        let took = started.elapsed();
        assert_eq!(collected.lock().unwrap().len(), total, "every byte delivered once");
        assert!(took < Duration::from_millis(500), "16 MB backlog took {took:?} to deliver");
    }

    #[test]
    fn cmd_shim_receives_plain_arguments() {
        let folder = tempfile::tempdir().expect("tempdir");
        std::fs::write(folder.path().join("ao-args.cmd"), ECHO_ARGS_SHIM).unwrap();
        let (_, out) = run_to_exit(shim_spec(folder.path(), "ao-args", &["--model", "opus 4"]));
        assert!(out.contains("args=[--model \"opus 4\"]"), "{out:?}");
    }

    #[test]
    fn cmd_shim_refuses_arguments_cmd_would_interpret() {
        let folder = tempfile::tempdir().expect("tempdir");
        std::fs::write(folder.path().join("ao-args.cmd"), ECHO_ARGS_SHIM).unwrap();
        let (manager, _exits) = manager();
        let unsafe_args = ["a&b", "x\" & y", "%PATH%", "a|b", "a<b", "a>b", "a^b", "a!b", "a\rb", "a\nb"];
        for program in ["ao-args", "ao-args.CMD"] {
            for arg in unsafe_args {
                let spawned = manager.spawn(shim_spec(folder.path(), program, &["-p", arg]));
                assert!(
                    matches!(spawned, Err(TerminalError::UnsafeBatchArgument(_))),
                    "{program} {arg:?}"
                );
            }
        }
    }

    #[test]
    #[ignore = "needs Claude Code on PATH"]
    fn claude_version_runs_through_the_manager() {
        let (manager, exits) = manager();
        let mut spec = cmd(&["--version"]);
        spec.program = CLAUDE_PROGRAM.into();
        let id = manager.spawn(spec).expect("spawn claude");
        let (sink, collected) = collector();
        manager.attach(id, sink).expect("attach");
        assert_eq!(expect_exit(&exits, id), 0);
        assert!(text(&collected).contains("Claude Code"), "{:?}", text(&collected));
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn spec(dir: &Path, command: &str) -> TerminalSpec {
        TerminalSpec { program: "/bin/sh".into(), args: vec!["-c".into(), command.into()],
            cwd: dir.into(), size: TerminalSize { cols: 80, rows: 24 },
            env: child_environment(std::env::vars_os()) }
    }

    fn rig() -> (TerminalManager, Receiver<(TerminalId, u32)>) {
        let (sent, received) = mpsc::channel();
        (TerminalManager::new(move |id, code| { let _ = sent.send((id, code)); }), received)
    }

    fn wait_for(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "condition timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn running(pid: u32) -> bool {
        crate::processes::snapshot().expect("process listing").iter().any(|p| p["pid"] == pid && p["running"] == true)
    }

    #[test]
    fn natural_exit_preserves_code_and_unix_cursor_query() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, exits) = rig();
        let id = manager.spawn(spec(dir.path(), "printf '\\033[6nhello'; exit 7")).unwrap();
        assert_eq!(exits.recv_timeout(Duration::from_secs(5)).unwrap(), (id, 7));
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&bytes);
        manager.attach(id, Box::new(move |chunk| lock(&output).extend_from_slice(chunk))).unwrap();
        assert_eq!(&*lock(&bytes), b"\x1b[6nhello");
        assert!(exits.try_recv().is_err());
    }

    #[test]
    fn explicit_kill_remove_and_drop_end_descendants() {
        for action in ["kill", "remove", "drop"] {
            let dir = tempfile::tempdir().unwrap();
            let (manager, exits) = rig();
            let id = manager.spawn(spec(dir.path(), "sleep 120 & echo $! > child; wait")).unwrap();
            let file = dir.path().join("child");
            wait_for(|| std::fs::read_to_string(&file).is_ok_and(|s| s.trim().parse::<u32>().is_ok()));
            let pid = std::fs::read_to_string(file).unwrap().trim().parse::<u32>().unwrap();
            match action {
                "kill" => manager.kill(id).unwrap(),
                "remove" => assert!(manager.remove_and_wait(id, Duration::from_secs(5)).unwrap()),
                _ => drop(manager),
            }
            assert_eq!(exits.recv_timeout(Duration::from_secs(5)).unwrap(), (id, KILLED_EXIT_CODE));
            wait_for(|| !running(pid));
            assert!(exits.try_recv().is_err());
        }
    }

    #[test]
    fn cleanup_reaches_separate_job_control_groups() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, exits) = rig();
        let mut command = spec(dir.path(), "set -m; sleep 120 & echo $! > child; wait");
        command.program = "/bin/bash".into();
        let id = manager.spawn(command).unwrap();
        let file = dir.path().join("child");
        wait_for(|| std::fs::read_to_string(&file).is_ok_and(|s| s.trim().parse::<u32>().is_ok()));
        let pid = std::fs::read_to_string(file).unwrap().trim().parse::<u32>().unwrap();
        assert_ne!(unsafe { libc::getpgid(pid as i32) }, manager.process_id(id).unwrap() as i32);
        manager.kill(id).unwrap();
        assert_eq!(exits.recv_timeout(Duration::from_secs(5)).unwrap().1, KILLED_EXIT_CODE);
        wait_for(|| !running(pid));
    }

    #[test]
    fn natural_exit_cleans_background_children() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, exits) = rig();
        manager.spawn(spec(dir.path(), "sleep 120 & echo $! > child; exit 0")).unwrap();
        assert_eq!(exits.recv_timeout(Duration::from_secs(5)).unwrap().1, 0);
        let pid = std::fs::read_to_string(dir.path().join("child")).unwrap().trim().parse::<u32>().unwrap();
        wait_for(|| !running(pid));
    }

    #[test]
    fn process_tree_owns_a_separate_group_and_kills_descendants() {
        let dir = tempfile::tempdir().unwrap();
        let (mut child, tree) = ProcessTree::spawn(std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 120 & echo $! > child; wait"]).current_dir(dir.path())).unwrap();
        assert_ne!(unsafe { libc::getpgid(child.id() as i32) }, unsafe { libc::getpgrp() });
        let file = dir.path().join("child");
        wait_for(|| std::fs::read_to_string(&file).is_ok_and(|s| s.trim().parse::<u32>().is_ok()));
        let pid = std::fs::read_to_string(file).unwrap().trim().parse::<u32>().unwrap();
        drop(tree);
        assert!(!child.wait().unwrap().success());
        wait_for(|| !running(pid));
    }

    #[test]
    fn shell_and_resolution_require_executable_files() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("tool");
        std::fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
        let mut env = Environment::from([("PATH".into(), dir.path().as_os_str().to_owned()), ("SHELL".into(), program.as_os_str().to_owned())]);
        assert_eq!(resolve_program("tool", &env), OsStr::new("tool"));
        assert_ne!(default_shell(&env), program.to_str().unwrap());
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(resolve_program("tool", &env), program.as_os_str());
        assert_eq!(default_shell(&env), program.to_str().unwrap());
        env.insert("SHELL".into(), "relative".into());
        assert!(Path::new(&default_shell(&env)).is_absolute());
    }

    #[test]
    fn failed_startup_does_not_emit_an_exit() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, exits) = rig();
        let mut command = spec(dir.path(), "");
        command.program = "/ao-no-such-executable".into();
        assert!(manager.spawn(command).is_err());
        assert!(exits.try_recv().is_err());
        command = spec(&dir.path().join("missing"), "");
        assert!(matches!(manager.spawn(command), Err(TerminalError::MissingDirectory(_))));
    }
}
