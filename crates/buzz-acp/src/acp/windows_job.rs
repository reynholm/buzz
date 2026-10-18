//! Suspended Windows spawn, owned job containment and independently queried retirement.
use super::{AcpClient, TerminationReport};
use process_wrap::tokio::{
    ChildWrapper, CommandWrap, CommandWrapper, CreationFlags, JobObject, KillOnDrop,
};
use std::{io, os::windows::io::AsRawHandle, process::ExitStatus, sync::Arc, time::Duration};
use tokio::{
    process::Command,
    time::{sleep, Instant},
};
use win32job::{ExtendedLimitInfo, Job};

pub(super) struct ContainedChild {
    inner: Box<dyn ChildWrapper>,
    job: Arc<Job>,
    pub(super) stdin: Option<tokio::process::ChildStdin>,
    pub(super) stdout: Option<tokio::process::ChildStdout>,
}

impl ContainedChild {
    pub(super) fn id(&self) -> Option<u32> {
        self.inner.id()
    }

    pub(super) fn start_kill(&mut self) -> io::Result<()> {
        self.inner.start_kill()
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // Only JobObject wraps the native child. Its safe inner_mut view avoids
        // completion-port events entirely: this reaps just the direct parent.
        self.inner.inner_mut().try_wait()
    }

    pub(super) async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.inner.inner_mut().wait().await
    }
}

#[derive(Debug)]
struct AssignQueryJob(Arc<Job>);

impl CommandWrapper for AssignQueryJob {
    fn post_spawn(
        &mut self,
        _command: &mut Command,
        child: &mut tokio::process::Child,
        _core: &CommandWrap,
    ) -> io::Result<()> {
        // All post_spawn hooks precede JobObject::wrap_child, which resumes the
        // suspended child. Use the borrowed PROCESS handle, never its PID.
        let handle = child
            .process_handle()
            .ok_or_else(|| io::Error::other("missing suspended process handle"))?;
        self.0.assign_process(handle.as_raw_handle() as isize)?;
        Ok(())
    }
}

pub(super) fn spawn(command: Command) -> io::Result<ContainedChild> {
    let mut limits = ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    // Neither breakaway flag is enabled. Nested jobs cannot escape this job.
    let job = Arc::new(Job::create_with_limit_info(&limits)?);
    let mut command = CommandWrap::from(command);
    let mut flags = CreationFlags(Default::default());
    flags.0 .0 = 0x0800_0000; // CREATE_NO_WINDOW, preserved through suspended spawn.
    command
        .wrap(KillOnDrop)
        .wrap(flags)
        .wrap(AssignQueryJob(Arc::clone(&job)))
        .wrap(JobObject);
    let mut inner = command.spawn()?;
    Ok(ContainedChild {
        stdin: inner.stdin().take(),
        stdout: inner.stdout().take(),
        inner,
        job,
    })
}

#[derive(Default)]
pub(super) struct Retirement {
    graceful_deadline: Option<Instant>,
    verify_deadline: Option<Instant>,
}

fn parent_reaped(child: &mut ContainedChild) -> io::Result<bool> {
    // Read only the direct child. process-wrap wait/completion packets are not
    // evidence that the job has zero descendants.
    child.try_wait().map(|status| status.is_some())
}

impl AcpClient {
    pub(super) async fn terminate_windows(&mut self) -> TerminationReport {
        self.termination.checked = true;
        let state = &mut self.termination.windows;
        // Save deadlines before any await, so cancellation/reentry never extends
        // either the graceful wait or the post-termination verification bound.
        let graceful = *state
            .graceful_deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_secs(15));
        self.stdin.take();
        loop {
            let reaped = match parent_reaped(&mut self.child) {
                Ok(reaped) => reaped,
                Err(_) => return TerminationReport::Unverified,
            };
            let count = match self.child.job.query_process_id_list() {
                Ok(processes) => processes.len(),
                // The library's bounded query also fails for oversized jobs.
                Err(_) => return TerminationReport::Unverified,
            };
            if count == 0 && reaped {
                return TerminationReport::Clean;
            }
            let now = Instant::now();
            if now >= graceful {
                let deadline = *state
                    .verify_deadline
                    .get_or_insert_with(|| now + Duration::from_secs(10));
                // This terminates the inner job and all nested descendants.
                // Retrying is safe and does not reset the verification deadline.
                if self.child.start_kill().is_err() {
                    return TerminationReport::Unverified;
                }
                if now >= deadline {
                    return TerminationReport::Leftover {
                        count: count.max(usize::from(!reaped)),
                    };
                }
            }
            sleep(Duration::from_millis(25)).await;
        }
    }
}
