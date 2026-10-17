//! Checked, bounded retirement, separate from crash/shutdown cleanup.
use super::AcpClient;
#[cfg(unix)]
use std::time::Duration;

/// Bound before a hung adapter receives SIGTERM.
#[cfg(unix)]
pub const TERMINATE_TERM_AFTER: Duration = Duration::from_secs(3);
/// Total graceful process-group wait before SIGKILL.
#[cfg(unix)]
pub const TERMINATE_KILL_AFTER: Duration = Duration::from_secs(15);
#[cfg(unix)]
const VERIFY_AFTER_KILL: Duration = Duration::from_secs(10);

/// Confirmed outcome of explicitly retiring a worker and its observed work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationReport {
    /// Every targeted process group disappeared and the direct child was reaped.
    Clean,
    /// Process groups remain after the bounded verification window.
    Leftover { count: usize },
    /// Ownership or enumeration could not be verified; no fresh session is safe.
    Unverified,
}

#[derive(Default)]
pub(super) struct TerminationState {
    pub(super) checked: bool,
    #[cfg(unix)]
    gone_groups: std::collections::HashSet<u32>,
    #[cfg(unix)]
    evidence: Option<(u32, crate::process_tree::retirement::RetirementSnapshot)>,
}

impl AcpClient {
    /// Retire this worker without claiming success until its work has stopped.
    /// On uncertainty the caller must retain this client and keep dispatch gated.
    pub async fn terminate(&mut self) -> TerminationReport {
        #[cfg(unix)]
        {
            self.terminate_with_snapshot(|| async {
                tokio::task::spawn_blocking(crate::process_tree::retirement::snapshot)
                    .await
                    .ok()
                    .flatten()
            })
            .await
        }
        #[cfg(not(unix))]
        {
            // Windows containment is supplied by the separate Job Object task.
            self.termination.checked = true;
            TerminationReport::Unverified
        }
    }

    #[cfg(unix)]
    pub(crate) async fn terminate_with_snapshot<F, Fut>(
        &mut self,
        mut snapshot: F,
    ) -> TerminationReport
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<
            Output = Option<crate::process_tree::retirement::RetirementSnapshot>,
        >,
    {
        use nix::sys::signal::Signal;
        use nix::unistd::getpgrp;
        use tokio::time::{sleep, Instant};
        self.termination.checked = true;
        let own_group = getpgrp().as_raw() as u32;
        if self.termination.evidence.is_none() {
            let Some(pid) = self.child.id().filter(|pid| *pid > 1 && *pid != own_group) else {
                return TerminationReport::Unverified;
            };
            for _ in 0..3 {
                if let Some(first) = snapshot().await.filter(|tree| tree.worker_present(pid)) {
                    self.termination.evidence = Some((pid, first));
                    break;
                }
            }
        }
        let Some((pid, first)) = self.termination.evidence.clone() else {
            return TerminationReport::Unverified;
        };
        // Do not close stdin until the pre-shutdown snapshot is retained.
        self.stdin.take();
        let started = Instant::now();
        let mut group_gone = self.termination.gone_groups.contains(&pid);
        let mut term_sent = false;
        while !group_gone && started.elapsed() < TERMINATE_KILL_AFTER {
            // Reap a zombie leader before probing its process group.
            let adapter_alive = !matches!(self.child.try_wait(), Ok(Some(_)));
            if !group_alive(pid) {
                group_gone = true;
                self.termination.gone_groups.insert(pid);
                break;
            }
            if adapter_alive && !term_sent && started.elapsed() >= TERMINATE_TERM_AFTER {
                signal_group(pid, own_group, Signal::SIGTERM);
                term_sent = true;
            }
            sleep(Duration::from_millis(25)).await;
        }
        let Some(last) = snapshot().await else {
            return TerminationReport::Unverified;
        };
        let mut groups = first.verified_groups(&last, pid, own_group);
        groups.retain(|group| !self.termination.gone_groups.contains(group));
        if group_gone {
            // ESRCH remains permanent across explicit retries: never signal a reused PGID.
            groups.remove(&pid);
        } else {
            groups.insert(pid);
        }
        for group in &groups {
            signal_group(*group, own_group, Signal::SIGKILL);
        }
        let deadline = Instant::now() + VERIFY_AFTER_KILL;
        loop {
            let reaped = matches!(self.child.try_wait(), Ok(Some(_)));
            groups.retain(|group| {
                if group_alive(*group) {
                    true
                } else {
                    self.termination.gone_groups.insert(*group);
                    false
                }
            });
            if groups.is_empty() && reaped {
                return TerminationReport::Clean;
            }
            if Instant::now() >= deadline {
                return TerminationReport::Leftover {
                    count: groups.len().max(usize::from(!reaped)),
                };
            }
            sleep(Duration::from_millis(25)).await;
        }
    }
}

#[cfg(unix)]
fn signal_group(group: u32, own_group: u32, signal: nix::sys::signal::Signal) {
    if group > 1 && group != own_group {
        if let Ok(group) = i32::try_from(group) {
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(group), signal);
        }
    }
}

#[cfg(unix)]
fn group_alive(group: u32) -> bool {
    use nix::{sys::signal::killpg, unistd::Pid};
    i32::try_from(group).map_or(true, |group| {
        // EPERM and every error other than ESRCH are conservative evidence of life.
        probe_alive(killpg(Pid::from_raw(group), None))
    })
}

#[cfg(unix)]
fn probe_alive(result: Result<(), nix::errno::Errno>) -> bool {
    result != Err(nix::errno::Errno::ESRCH)
}

#[cfg(all(test, unix))]
mod probe_tests {
    #[test]
    fn permission_denied_process_group_is_alive() {
        use nix::errno::Errno;
        for result in [Ok(()), Err(Errno::EPERM), Err(Errno::EINVAL)] {
            assert!(super::probe_alive(result));
        }
        assert!(!super::probe_alive(Err(Errno::ESRCH)));
    }
}
