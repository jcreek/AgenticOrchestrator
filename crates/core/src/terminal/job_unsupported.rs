use super::TerminalError;
use portable_pty::Child;

#[derive(Debug, thiserror::Error)]
#[error("process trees are only implemented on Windows")]
pub struct Error;

pub(super) struct Job;

impl Job {
    pub(super) fn new() -> Result<Self, Error> {
        Err(Error)
    }

    pub(super) fn assign_child(&self, _child: &std::process::Child) -> Result<(), Error> {
        Err(Error)
    }

    pub(super) fn terminate(&self) -> Result<(), TerminalError> {
        Err(Error.into())
    }

    pub(super) fn active_processes(&self) -> Result<u32, TerminalError> {
        Err(Error.into())
    }
}

pub(super) fn join_job(_job: &Job, _child: &mut (dyn Child + Send + Sync)) -> Result<(), TerminalError> {
    Err(Error.into())
}
