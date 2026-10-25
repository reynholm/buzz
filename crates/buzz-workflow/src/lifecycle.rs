//! Optional host lifecycle admission, independent of any relay implementation.
use std::future::{poll_fn, Future};
use std::task::Poll;

/// The host has closed admission for new autonomous runs.
#[derive(Debug)]
pub struct AdmissionClosed;

/// Host ownership of a run from before its first durable mutation through
/// execution and durable finalization. Dropping is not a success acknowledgment.
pub trait RunLease: Send + Sync {
    /// Enter the accepted parent's context for one synchronous future poll.
    fn with_context(&self, callback: &mut dyn FnMut());
    /// Acknowledge known durable completion.
    fn complete(self: Box<Self>);
}

/// Host admission policy, installed before exposing the engine to callers.
pub trait RunAdmission: Send + Sync {
    /// Admit an autonomous run or inherit an already accepted serving parent.
    /// Hosts preserve accepted effects while blocking termination if lineage
    /// is unknown; autonomous admission obeys the drain barrier.
    fn admit(&self, autonomous: bool) -> Result<Box<dyn RunLease>, AdmissionClosed>;
}

/// Owned run lifetime and context supplied by the host.
pub struct RunGuard(Box<dyn RunLease>);

impl RunGuard {
    /// Wrap a newly admitted host lease.
    pub fn new(lease: Box<dyn RunLease>) -> Self {
        Self(lease)
    }

    /// Poll the complete run inside its inherited context, including polls
    /// after suspension. Detached descendants need their own admitted leases.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        poll_fn(|context| {
            let mut result = Poll::Pending;
            self.0
                .with_context(&mut || result = future.as_mut().poll(context));
            result
        })
        .await
    }

    /// Acknowledge completion after durable finalization succeeded.
    pub fn complete(self) {
        self.0.complete();
    }
}

/// Run an accepted task and acknowledge its lease only for known completion.
/// No lease preserves the host's legacy behavior.
pub async fn finish(guard: Option<RunGuard>, task: impl Future<Output = bool>) {
    if let Some(guard) = guard {
        if guard.scope(task).await {
            guard.complete();
        }
    } else {
        task.await;
    }
}
