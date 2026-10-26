//! Ordered shutdown of serving dependencies after all admitted producers finish.
use std::{future::Future, time::Duration};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{deploy_drain::DrainController, state::AuditShutdownHandle};

pub(crate) const WORKER: &str = "runtime-dependencies";

/// Owns read-only subscribers and their cooperative cancellation handles.
/// These remain available throughout admission fencing and accepted work drain.
#[derive(Default)]
pub struct RuntimeDependencies {
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
    cancellations: Vec<CancellationToken>,
}

impl RuntimeDependencies {
    /// Cancellation for owned consumers that must join their own child tasks.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Own a read-only task. Dropping its read future after serving work ends
    /// cannot cancel an accepted mutation; mutators use deployment workers.
    pub fn spawn_reader(&mut self, future: impl Future<Output = ()> + Send + 'static) {
        let cancel = self.cancel.clone();
        self.tasks.push(tokio::spawn(async move {
            tokio::select! {
                _ = cancel.cancelled() => {},
                _ = future => {},
            }
        }));
    }

    /// Own an existing cooperative task and the token that stops it.
    pub fn own(&mut self, task: JoinHandle<()>, cancel: CancellationToken) {
        self.tasks.push(task);
        self.cancellations.push(cancel);
    }

    /// Cancel read-only loops and join every owned task. No timeout fabricates
    /// completion, and a panic keeps the deployment stop barrier closed.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        for cancel in self.cancellations {
            cancel.cancel();
        }
        let mut failure = None;
        for task in self.tasks {
            if let Err(error) = task.await {
                failure = Some(error);
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Close bootstrap only for the qualified production feature combination,
    /// after every mutator and the final dependency joiner registered.
    pub fn seal_census(drain: &DrainController) -> Result<(), crate::deploy_drain::DrainError> {
        drain.seal_worker_inventory(&[
            "partition-maintenance",
            "workflow-scheduler",
            "channel-reaper",
            "operator-registration-reaper",
            "operator-outbox-reaper",
            "admin-outbox",
            "admin-action",
            "reminder-scheduler",
            "pool-lease-reaper",
            "usage-invite-reaper",
            WORKER,
        ])
    }

    /// Register before exposing control, then retain dependencies and the audit
    /// queue until no accepted parent can create another descendant.
    pub fn start_drain(self, drain: DrainController, audit: AuditShutdownHandle) -> JoinHandle<()> {
        if drain.register_worker(WORKER).is_err() {
            drain.record_fault("runtime-registration-failed");
        }
        tokio::spawn(async move {
            while !drain.ready_to_join_runtime() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let readers = self.shutdown().await;
            let audit = audit.join().await;
            if readers.is_err() || audit.is_err() {
                drain.record_fault("runtime-join-failed");
            } else if drain.worker_quiesced(WORKER).is_err() {
                drain.record_fault("runtime-quiescence-failed");
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_runtime_inventory_requires_every_registered_worker_before_serving() {
        let drain = DrainController::with_workers(&["worker-inventory-incomplete"]);
        assert!(drain.seal_worker_inventory(&["first", "second"]).is_err());
        drain.register_worker("first").unwrap();
        assert!(drain.seal_worker_inventory(&["first", "second"]).is_err());
        drain.register_worker("second").unwrap();
        drain.seal_worker_inventory(&["first", "second"]).unwrap();
        assert!(!drain
            .snapshot()
            .blocking_workers
            .contains(&"worker-inventory-incomplete".into()));
        let op = uuid::Uuid::new_v4();
        let boot = drain.begin(op).unwrap().boot_id;
        drain.fence_http(op, boot, "test-proof").unwrap();
        assert!(!drain.snapshot().safe_to_stop);
        drain.worker_quiesced("first").unwrap();
        assert!(!drain.snapshot().safe_to_stop);
        drain.worker_quiesced("second").unwrap();
        assert!(drain.snapshot().safe_to_stop);
        assert!(drain.seal_worker_inventory(&["first", "second"]).is_err());
        let unknown =
            DrainController::with_workers(&["worker-inventory-incomplete", "unqualified"]);
        assert!(unknown.seal_worker_inventory(&[]).is_err());
    }
}
