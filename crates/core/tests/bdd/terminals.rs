use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cucumber::{given, then, when};
use orchestrator_core::terminal::{OutputSink, TerminalId, TerminalManager, TerminalSize, TerminalSpec, child_environment};
use crate::{CoreWorld, off_thread};

const TIMEOUT: Duration = Duration::from_secs(20);

type Output = Arc<Mutex<Vec<u8>>>;

pub struct Rig {
    manager: TerminalManager,
    exits: Receiver<(TerminalId, u32)>,
    id: TerminalId,
    first: Output,
    second: Output,
    child: Option<u32>,
}

impl Rig {
    fn exit_code(&self) -> u32 {
        let (id, code) = self.exits.recv_timeout(TIMEOUT).expect("the terminal reported its exit");
        assert_eq!(id, self.id);
        code
    }
}

fn start(world: &CoreWorld, command: &str) -> Rig {
    let (sender, exits) = mpsc::channel();
    let manager = TerminalManager::new(move |id, code| {
        let _ = sender.send((id, code));
    });
    let spec = TerminalSpec {
        program: if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into(),
        args: vec![if cfg!(windows) { "/c" } else { "-c" }.into(), command.into()],
        cwd: world.dir.path().to_path_buf(),
        size: TerminalSize { cols: 100, rows: 30 },
        env: child_environment(std::env::vars_os()),
    };
    let id = manager.spawn(spec).expect("spawn terminal");
    let first = Output::default();
    manager.attach(id, sink(&first)).expect("attach");
    Rig { manager, exits, id, first, second: Output::default(), child: None }
}

/// Runs `work` with the scenario's terminal off the scenario thread, then puts the terminal back.
async fn with_rig<T: Send + 'static>(world: &mut CoreWorld, work: impl FnOnce(&mut Rig) -> T + Send + 'static) -> T {
    let mut rig = world.terminal.take().expect("a terminal was started earlier in the scenario");
    let (rig, value) = off_thread(move || {
        let value = work(&mut rig);
        (rig, value)
    })
    .await;
    world.terminal = Some(rig);
    value
}

fn sink(output: &Output) -> OutputSink {
    let output = Arc::clone(output);
    Box::new(move |bytes| output.lock().expect("output lock").extend_from_slice(bytes))
}

fn processes() -> Vec<(u32, u32, String)> {
    orchestrator_core::processes::list().into_iter().filter(|row| row["running"] == true)
        .map(|row| (row["pid"].as_u64().unwrap() as u32, row["ppid"].as_u64().unwrap() as u32, row["exe"].as_str().unwrap().to_owned())).collect()
}

fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out after {TIMEOUT:?} waiting until {what}");
        thread::sleep(Duration::from_millis(50));
    }
}

#[given(expr = "a terminal that printed {int} numbered lines and exited")]
async fn printed_lines(world: &mut CoreWorld, count: u32) {
    let command = if cfg!(windows) { format!("for /l %i in (1,1,{count}) do @echo line-%i") }
        else { format!("i=1; while [ $i -le {count} ]; do echo line-$i; i=$((i+1)); done") };
    world.terminal = Some(start(world, &command));
    assert_eq!(with_rig(world, |rig| rig.exit_code()).await, 0);
}

#[when("a second viewer attaches to it")]
fn second_viewer(world: &mut CoreWorld) {
    let rig = world.terminal.as_ref().expect("terminal");
    rig.manager.attach(rig.id, sink(&rig.second)).expect("attach again");
}

#[then("the second viewer sees exactly what the first viewer saw")]
fn same_output(world: &mut CoreWorld) {
    let rig = world.terminal.as_ref().expect("terminal");
    let first = rig.first.lock().expect("lock").clone();
    let second = rig.second.lock().expect("lock").clone();
    assert!(String::from_utf8_lossy(&second).contains("line-200"), "the replay is missing the last line");
    assert!(first == second, "the replay differs from what the first viewer received");
}

#[given("a terminal running a command that started a long-running child process")]
async fn long_running_child(world: &mut CoreWorld) {
    world.terminal = Some(start(world, if cfg!(windows) { "ping -n 120 127.0.0.1" } else { "sleep 120 & wait" }));
    with_rig(world, |rig| {
        let shell = rig.manager.process_id(rig.id).expect("pid");
        eventually("the child process has started", || {
            rig.child = processes()
                .into_iter()
                .find(|(_, parent, exe)| *parent == shell && exe.eq_ignore_ascii_case(if cfg!(windows) { "PING.EXE" } else { "sleep" }))
                .map(|(pid, _, _)| pid);
            rig.child.is_some()
        });
    })
    .await;
}

#[when("the terminal is closed")]
async fn close_terminal(world: &mut CoreWorld) {
    let ended = with_rig(world, |rig| rig.manager.remove_and_wait(rig.id, TIMEOUT).expect("close terminal")).await;
    assert!(ended, "the terminal's processes did not end within {TIMEOUT:?}");
}

#[then("the terminal and its child process have both stopped")]
async fn both_stopped(world: &mut CoreWorld) {
    with_rig(world, |rig| {
        let child = rig.child.expect("child pid");
        eventually("the child process has stopped", || !processes().iter().any(|(pid, _, _)| *pid == child));
    })
    .await;
}

#[when(expr = "a terminal starts a shell that exits at once with code {int}")]
fn shell_exits_at_once(world: &mut CoreWorld, code: u32) {
    world.terminal = Some(start(world, &format!("exit {code}")));
}

#[then(expr = "the terminal is reported as exited with code {int}")]
async fn reported_exited(world: &mut CoreWorld, code: u32) {
    assert_eq!(with_rig(world, |rig| rig.exit_code()).await, code);
}
