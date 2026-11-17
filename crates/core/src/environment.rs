//! One startup environment shared by every external tool. Never mutates the app environment.
use crate::terminal::{Environment, child_environment};
#[cfg(unix)]
use crate::terminal::{ProcessTree, default_shell};
#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::io::Read;
use std::sync::OnceLock;
#[cfg(unix)]
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();

pub fn environment() -> &'static Environment {
    ENVIRONMENT.get_or_init(|| {
        let inherited: Environment = std::env::vars_os().collect();
        #[cfg(unix)]
        let inherited = login_environment(&inherited).unwrap_or(inherited);
        child_environment(inherited)
    })
}

#[cfg(unix)]
fn login_environment(inherited: &Environment) -> Option<Environment> {
    capture(inherited, &default_shell(inherited), Duration::from_secs(3))
}

#[cfg(unix)]
fn capture(inherited: &Environment, shell: &str, timeout: Duration) -> Option<Environment> {
    use std::os::unix::ffi::OsStringExt;
    let (mut child, tree) = ProcessTree::spawn(
        Command::new(shell)
            .args(["-l", "-c", "printf '\\000AO_ENV\\000'; /usr/bin/env -0"])
            .env_clear()
            .envs(inherited)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sent, received) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        // Bound output from user shell startup files.
        let result = stdout
            .by_ref()
            .take(4 * 1024 * 1024)
            .read_to_end(&mut output)
            .map(|_| output);
        let _ = sent.send(result);
    });
    let deadline = Instant::now() + timeout;
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => break false,
        }
    };
    drop(tree);
    let _ = child.wait();
    if !success {
        return None;
    }
    let output = received
        .recv_timeout(Duration::from_millis(200))
        .ok()?
        .ok()?;
    let marker = b"\0AO_ENV\0";
    let start = output.windows(marker.len()).position(|w| w == marker)? + marker.len();
    let values = output[start..]
        .split(|b| *b == 0)
        .filter_map(|entry| {
            let at = entry.iter().position(|b| *b == b'=')?;
            Some((
                OsString::from_vec(entry[..at].to_vec()),
                OsString::from_vec(entry[at + 1..].to_vec()),
            ))
        })
        .collect::<Environment>();
    (!values.is_empty()).then_some(values)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture(body: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        (dir, path.to_str().unwrap().into())
    }

    #[test]
    fn capture_handles_startup_noise_and_filters_injected_values() {
        let (_dir, shell) = fixture(
            "printf 'startup noise\\n\\000AO_ENV\\000PATH=/bin\\000CLAUDECODE=1\\000USER_VALUE=hello world\\000'",
        );
        let env = capture(&Environment::new(), &shell, Duration::from_secs(1)).unwrap();
        let env = child_environment(env);
        assert_eq!(env[&OsString::from("USER_VALUE")], "hello world");
        assert!(!env.contains_key(&OsString::from("CLAUDECODE")));
    }

    #[test]
    fn failed_or_timed_out_capture_returns_fallback_signal() {
        let (_dir, shell) = fixture("sleep 30");
        let start = Instant::now();
        assert!(capture(&Environment::new(), &shell, Duration::from_millis(100)).is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
        let (_dir, shell) = fixture("exit 1");
        assert!(capture(&Environment::new(), &shell, Duration::from_secs(1)).is_none());
    }
}
