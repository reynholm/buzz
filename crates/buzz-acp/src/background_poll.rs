//! One bounded process enumeration at a time, independent of relay reception.
use crate::{pool::BackgroundPollRequest, process_tree::ProcessTree};
use std::time::{Duration, Instant};
use tokio::task::{JoinError, JoinSet};

pub(crate) struct BackgroundPollResult {
    pub requests: Vec<BackgroundPollRequest>,
    pub tree: Option<ProcessTree>,
    pub elapsed: Duration,
}

#[derive(Default)]
pub(crate) struct BackgroundPoll {
    tasks: JoinSet<BackgroundPollResult>,
    last_warning: Option<Instant>,
}

impl BackgroundPoll {
    pub fn report(&mut self, result: &BackgroundPollResult) {
        let unhealthy = result.tree.is_none() || result.elapsed >= Duration::from_secs(1);
        if unhealthy
            && self
                .last_warning
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(60))
        {
            self.last_warning = Some(Instant::now());
            tracing::warn!(
                snapshot_ms = result.elapsed.as_millis(),
                successful = result.tree.is_some(),
                "background process snapshot slow or unavailable; unknown work remains held"
            );
        }
    }
    pub fn in_flight(&self) -> bool {
        !self.tasks.is_empty()
    }

    pub fn start(&mut self, requests: Vec<BackgroundPollRequest>) {
        if !requests.is_empty() {
            self.start_with(requests, crate::process_tree::snapshot);
        }
    }

    pub(super) fn start_with(
        &mut self,
        requests: Vec<BackgroundPollRequest>,
        snapshot: impl FnOnce() -> Option<ProcessTree> + Send + 'static,
    ) -> bool {
        if self.in_flight() {
            return false;
        }
        self.tasks.spawn_blocking(move || {
            let started = Instant::now();
            let tree = snapshot();
            #[cfg(unix)]
            let tree = tree.map(|mut tree| {
                for request in &requests {
                    if let Some(registry) = &request.registry {
                        registry.apply(&mut tree);
                    }
                }
                tree
            });
            BackgroundPollResult {
                requests,
                tree,
                elapsed: started.elapsed(),
            }
        });
        true
    }

    pub async fn wait(&mut self) -> Option<Result<BackgroundPollResult, JoinError>> {
        if self.in_flight() {
            self.tasks.join_next().await
        } else {
            std::future::pending().await
        }
    }
}
