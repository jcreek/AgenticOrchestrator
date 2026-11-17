use super::{KILLED_EXIT_CODE, TerminalError};
use portable_pty::Child;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows::core::PCWSTR;
pub use windows::core::Error;

pub(super) fn join_job(job: &Job, child: &mut (dyn Child + Send + Sync)) -> Result<(), TerminalError> {
    let Some(process) = child.as_raw_handle() else {
        return Err(TerminalError::Pty("child process has no handle".into()));
    };
    match job.assign(HANDLE(process)) {
        Ok(()) => Ok(()),
        // A child that has already exited cannot join a job and leaves nothing to kill.
        Err(_) if matches!(child.try_wait(), Ok(Some(_))) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) struct Job(HANDLE);

// A job handle is a kernel handle usable from any thread.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub(super) fn new() -> windows::core::Result<Self> {
        let job = Self(unsafe { CreateJobObjectW(None, PCWSTR::null())? });
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
        }
        Ok(job)
    }

    fn assign(&self, process: HANDLE) -> windows::core::Result<()> {
        unsafe { AssignProcessToJobObject(self.0, process) }
    }

    pub(super) fn assign_child(&self, child: &std::process::Child) -> windows::core::Result<()> {
        self.assign(HANDLE(std::os::windows::io::AsRawHandle::as_raw_handle(child)))
    }

    pub(super) fn terminate(&self) -> Result<(), TerminalError> {
        unsafe { TerminateJobObject(self.0, KILLED_EXIT_CODE)? };
        Ok(())
    }

    pub(super) fn active_processes(&self) -> Result<u32, TerminalError> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(self.0),
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )?;
        }
        Ok(info.ActiveProcesses)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
