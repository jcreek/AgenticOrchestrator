use super::TerminalError;
use portable_pty::Child;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

pub type Error = std::io::Error;

/// portable-pty 0.9.0 calls setsid before exec, making the child its session/group leader.
pub(super) fn join_job(
    job: &Job,
    child: &mut (dyn Child + Send + Sync),
) -> Result<(), TerminalError> {
    let pid = child
        .process_id()
        .ok_or_else(|| Error::other("PTY child has no pid"))?;
    job.set_group(pid)?;
    Ok(())
}

pub(super) struct Job {
    group: AtomicI32,
    terminated: AtomicBool,
}

impl Job {
    pub(super) fn new() -> Result<Self, Error> {
        Ok(Self {
            group: AtomicI32::new(0),
            terminated: AtomicBool::new(false),
        })
    }

    pub(super) fn set_group(&self, pid: u32) -> Result<(), Error> {
        let pid = i32::try_from(pid).map_err(|_| Error::other("invalid process group"))?;
        if pid <= 1 || pid == unsafe { libc::getpgrp() } {
            return Err(Error::other(
                "refusing to own the application's process group",
            ));
        }
        self.group.store(pid, Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn terminate(&self) -> Result<(), TerminalError> {
        let group = self.group.load(Ordering::SeqCst);
        if group <= 1 || self.terminated.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let result = self.terminate_session(group);
        if result.is_err() {
            self.terminated.store(false, Ordering::SeqCst);
        }
        result
    }

    fn terminate_session(&self, session: i32) -> Result<(), TerminalError> {
        // Interactive shells put foreground/background jobs in separate groups. All remain
        // in the PTY's session unless they deliberately detach with setsid.
        let members = crate::processes::snapshot();
        let mut groups = std::collections::BTreeSet::from([session]);
        if let Ok(members) = &members {
            groups.extend(
                members
                    .iter()
                    .filter(|row| row["session"] == session && row["running"] == true)
                    .filter_map(|row| row["group"].as_i64().map(|n| n as i32)),
            );
        }
        for group in groups {
            if group <= 1 || group == unsafe { libc::getpgrp() } {
                return Err(
                    Error::other("refusing to signal the application's process group").into(),
                );
            }
            if unsafe { libc::killpg(group, libc::SIGKILL) } != 0 {
                let error = Error::last_os_error();
                // Darwin reports EPERM for groups with only zombies. Linux may report
                // success instead. Neither case has live work left to terminate.
                let empty = members.as_ref().is_ok_and(|rows| {
                    !rows
                        .iter()
                        .any(|row| row["group"] == group && row["running"] == true)
                });
                if error.raw_os_error() != Some(libc::ESRCH)
                    && !(error.raw_os_error() == Some(libc::EPERM) && empty)
                {
                    return Err(error.into());
                }
            }
        }
        members?;
        Ok(())
    }

    pub(super) fn active_processes(&self) -> Result<u32, TerminalError> {
        let session = self.group.load(Ordering::SeqCst);
        if session <= 1 {
            return Ok(0);
        }
        Ok(crate::processes::snapshot()?
            .iter()
            .filter(|row| row["session"] == session && row["running"] == true)
            .count() as u32)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
