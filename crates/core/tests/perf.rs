//! Timing benchmarks. Run in release: `cargo test --release -p orchestrator-core --test perf -- --ignored --nocapture --test-threads=1`.

#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orchestrator_core::terminal::{self, TerminalManager, TerminalSize, TerminalSpec};
use orchestrator_core::{files, git, worktrees};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

const RUNS: usize = 9;

fn report(label: &str, mut samples: Vec<f64>, unit: &str) {
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];
    println!(
        "{label}: median {median:.2} {unit} (min {:.2}, max {:.2}, n {})",
        samples[0],
        samples[samples.len() - 1],
        samples.len()
    );
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn time_ms(runs: usize, mut work: impl FnMut()) -> Vec<f64> {
    work();
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            work();
            ms(started.elapsed())
        })
        .collect()
}

fn own_cpu() -> Duration {
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user);
    }
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

fn sh(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?}");
}

/// A repository with `count` committed files spread over folders, a few edits and untracked files.
fn sample_repo(root: &Path, count: usize) {
    fs::create_dir_all(root).expect("mkdir");
    sh(root, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "T"),
        ("user.email", "t@example.com"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        sh(root, &["config", key, value]);
    }
    for index in 0..count {
        let dir = root.join(format!("src/mod{}", index % 50));
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(format!("file{index}.rs")), format!("fn f{index}() {{}}\n").repeat(20)).expect("write");
    }
    sh(root, &["add", "-A"]);
    sh(root, &["commit", "-q", "-m", "init"]);
    for index in 0..10 {
        let path = root.join(format!("src/mod{}/file{index}.rs", index % 50));
        fs::write(path, "changed\n").expect("write");
        fs::write(root.join(format!("untracked{index}.txt")), "u").expect("write");
    }
}

#[test]
#[ignore = "benchmark"]
fn perf_git_summary_and_status() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    sample_repo(&repo, 3000);
    let summary = || drop(git::repo_facts(&repo).expect("facts"));
    report("workspace_summary calls (3000 files, 10 edits, 10 untracked)", time_ms(RUNS, summary), "ms");
    report("git_status (3000 files)", time_ms(RUNS, || drop(git::status(&repo).expect("status"))), "ms");
    if let Ok(own) = std::env::var("AO_PERF_REPO") {
        let own = Path::new(&own).to_path_buf();
        report("git_status (AO_PERF_REPO)", time_ms(RUNS, || drop(git::status(&own).expect("status"))), "ms");
    }
}

#[test]
#[ignore = "benchmark"]
fn perf_file_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("small.rs"), "x".repeat(20 * 1024)).expect("write");
    fs::write(dir.path().join("big.txt"), "x".repeat(5 * 1024 * 1024)).expect("write");
    for name in ["small.rs", "big.txt"] {
        let samples = time_ms(200, || drop(files::file_version(dir.path(), name).expect("version")));
        report(&format!("file_version {name}"), samples, "ms");
        let samples = time_ms(200, || drop(files::resolve(dir.path(), name).expect("resolve")));
        report(&format!("resolve only {name}"), samples, "ms");
    }
}

#[test]
#[ignore = "benchmark"]
fn perf_claude_agents_query() {
    let env = terminal::child_environment(std::env::vars_os());
    let program = terminal::resolve_program("claude", &env);
    let mut wall = Vec::new();
    let mut cpu = Vec::new();
    for _ in 0..RUNS {
        let started = Instant::now();
        let mut child = Command::new(&program)
            .args(["agents", "--json"])
            .env_clear()
            .envs(&env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("claude runs");
        let tree = Job::adopt(&child);
        let exit = loop {
            match child.try_wait().expect("wait") {
                Some(exit) => break Some(exit),
                None if started.elapsed() > Duration::from_secs(30) => break None,
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        wall.push(ms(started.elapsed()));
        cpu.push(ms(tree.cpu()));
        drop(tree);
        let _ = child.wait();
        assert!(exit.is_some_and(|exit| exit.success()), "claude agents failed or took over 30 s");
    }
    report("claude agents --json wall", wall, "ms");
    report("claude agents --json CPU (whole tree)", cpu, "ms");
}

struct Job(windows::Win32::Foundation::HANDLE);

impl Job {
    fn adopt(child: &std::process::Child) -> Self {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
        unsafe {
            let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).expect("job");
            let _ = AssignProcessToJobObject(job, windows::Win32::Foundation::HANDLE(child.as_raw_handle()));
            Self(job)
        }
    }

    fn cpu(&self) -> Duration {
        use windows::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation, QueryInformationJobObject,
        };
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            let _ = QueryInformationJobObject(
                Some(self.0),
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            );
        }
        let ticks = u64::try_from(info.TotalUserTime + info.TotalKernelTime).unwrap_or(0);
        Duration::from_nanos(ticks * 100)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn cmd(args: &[&str], cwd: &Path) -> TerminalSpec {
    TerminalSpec {
        program: "cmd.exe".into(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: cwd.to_path_buf(),
        size: TerminalSize { cols: 120, rows: 40 },
        env: terminal::child_environment(std::env::vars_os()),
    }
}

fn manager() -> (TerminalManager, mpsc::Receiver<u64>) {
    let (sender, exits) = mpsc::channel();
    let manager = TerminalManager::new(move |id, _| {
        let _ = sender.send(id.0);
    });
    (manager, exits)
}

fn wait_for(seen: &Mutex<Vec<u8>>, needle: &[u8], from: usize, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if seen.lock().expect("lock")[from..].windows(needle.len()).any(|window| window == needle) {
            return true;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    false
}

fn echo_latency(flooding: bool) -> Vec<f64> {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("flood.txt"), "0123456789abcdef".repeat(64 * 1024 * 4)).expect("write");
    let (manager, _exits) = manager();
    let flood = flooding.then(|| {
        let id = manager
            .spawn(cmd(&["/c", "for /l %i in (1,1,1000) do @type flood.txt"], dir.path()))
            .expect("spawn flood");
        manager.attach(id, Box::new(|_| {})).expect("attach");
        id
    });
    let id = manager.spawn(cmd(&[], dir.path())).expect("spawn");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let target = Arc::clone(&seen);
    manager.attach(id, Box::new(move |bytes| target.lock().expect("lock").extend_from_slice(bytes))).expect("attach");
    assert!(wait_for(&seen, b">", 0, Duration::from_secs(10)), "no prompt");
    std::thread::sleep(Duration::from_millis(300));
    let mut samples = Vec::new();
    for key in b"qwertyuiopasdfghjklzxcvbnmQWERTYUIOPASDFGHJKLZXCVBNM".iter().take(40) {
        let from = seen.lock().expect("lock").len();
        let started = Instant::now();
        manager.write(id, &[*key]).expect("write");
        assert!(wait_for(&seen, &[*key], from, Duration::from_secs(5)), "no echo");
        samples.push(ms(started.elapsed()));
        std::thread::sleep(Duration::from_millis(30));
    }
    if let Some(flood) = flood {
        let _ = manager.remove(flood);
    }
    let _ = manager.remove(id);
    samples
}

#[test]
#[ignore = "benchmark"]
fn perf_terminal_echo_latency() {
    report("echo latency idle", echo_latency(false), "ms");
    report("echo latency while another terminal floods", echo_latency(true), "ms");
}

fn flood(terminals: usize, megabytes: usize) -> (f64, f64, f64) {
    let dir = tempfile::tempdir().expect("tempdir");
    let line = format!("{}\r\n", "0123456789abcdefghijklmnopqrstuvwxyz".repeat(3));
    let content = line.repeat(megabytes * 1024 * 1024 / line.len());
    fs::write(dir.path().join("flood.txt"), &content).expect("write");
    let (manager, exits) = manager();
    let received = Arc::new(AtomicUsize::new(0));
    let cpu_before = own_cpu();
    let started = Instant::now();
    for _ in 0..terminals {
        let id = manager.spawn(cmd(&["/c", "type flood.txt"], dir.path())).expect("spawn");
        let counter = Arc::clone(&received);
        manager
            .attach(id, Box::new(move |bytes| _ = counter.fetch_add(bytes.len(), Ordering::Relaxed)))
            .expect("attach");
    }
    for _ in 0..terminals {
        exits.recv_timeout(Duration::from_secs(600)).expect("exit");
    }
    let elapsed = started.elapsed().as_secs_f64();
    let cpu = own_cpu().saturating_sub(cpu_before);
    let mb = received.load(Ordering::Relaxed) as f64 / 1024.0 / 1024.0;
    (mb / elapsed, ms(cpu) / mb, mb)
}

#[test]
#[ignore = "benchmark"]
fn perf_terminal_flood() {
    for (terminals, megabytes) in [(1, 20), (20, 4)] {
        let mut rate = Vec::new();
        let mut cpu = Vec::new();
        for _ in 0..5 {
            let (mb_per_s, cpu_ms_per_mb, mb) = flood(terminals, megabytes);
            assert!(mb > (terminals * megabytes) as f64 * 0.9, "only {mb} MB arrived");
            rate.push(mb_per_s);
            cpu.push(cpu_ms_per_mb);
        }
        report(&format!("flood {terminals} terminal(s) x {megabytes} MB, total throughput"), rate, "MB/s");
        report(&format!("flood {terminals} terminal(s), app-process CPU per MB"), cpu, "ms/MB");
    }
}

#[test]
#[ignore = "benchmark"]
fn perf_worktree_create_and_remove() {
    let mut create = Vec::new();
    let mut blockers = Vec::new();
    let mut remove = Vec::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    sample_repo(&repo, 300);
    sh(&repo, &["add", "-A"]);
    sh(&repo, &["commit", "-q", "-m", "clean"]);
    for run in 0..RUNS {
        let branch = format!("perf-{run}");
        let started = Instant::now();
        let path = worktrees::create(&repo, &branch, None, None, true).expect("create");
        create.push(ms(started.elapsed()));
        let started = Instant::now();
        let found = worktrees::removal_blockers(&path).expect("blockers");
        blockers.push(ms(started.elapsed()));
        let started = Instant::now();
        worktrees::check_removable(&repo, &path, &found).expect("check");
        worktrees::remove(&repo, &path, &found).expect("remove");
        remove.push(ms(started.elapsed()));
    }
    report("worktree create (300 files)", create, "ms");
    report("removal_blockers", blockers, "ms");
    report("check_removable + remove (as remove_worktree does)", remove, "ms");
}
