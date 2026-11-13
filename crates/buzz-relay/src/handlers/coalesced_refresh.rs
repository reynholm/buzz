//! Per-key single-flight refresh with a minimum interval.
//!
//! A refresh for a key runs at once when none is in flight. Requests that
//! arrive while one is running or cooling down collapse into one trailing
//! rerun, so a key refreshes at most once per interval per process and the
//! last request is always covered by a refresh that started after it. Each
//! run is bounded by a timeout, so a hung refresh can't wedge its key.

use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;

use crate::deploy_drain::WorkGuard;

/// Coalesces refresh requests per key. The map holds only keys with a
/// refresh in flight; the value is "another request arrived since it began".
pub struct CoalescedRefresh<K> {
    pending: DashMap<K, bool>,
    min_interval: Duration,
    run_timeout: Duration,
}

impl<K> CoalescedRefresh<K>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
{
    /// A coalescer that refreshes each key at most once per `min_interval`,
    /// abandoning any single run that takes longer than `run_timeout`.
    pub fn new(min_interval: Duration, run_timeout: Duration) -> Self {
        Self {
            pending: DashMap::new(),
            min_interval,
            run_timeout,
        }
    }

    /// Request a refresh of `key`. Runs `refresh` on a spawned task now when
    /// `key` is idle; otherwise marks the in-flight refresh to run once more
    /// after its interval elapses.
    #[cfg(test)]
    pub fn request<F, Fut>(self: &Arc<Self>, key: K, refresh: F)
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        self.request_tracked(
            key,
            || None,
            move || {
                let run = refresh();
                async move {
                    run.await;
                    true
                }
            },
        );
    }

    /// Own one accepted task across its initial refresh, cooldown and all
    /// trailing refreshes. Coalesced requests share that ownership; they do
    /// not create guards that could be abandoned without executing work.
    pub fn request_tracked<F, Fut>(
        self: &Arc<Self>,
        key: K,
        create_work: impl FnOnce() -> Option<WorkGuard>,
        refresh: F,
    ) where
        F: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = bool> + Send,
    {
        match self.pending.entry(key.clone()) {
            Entry::Occupied(mut rerun) => {
                *rerun.get_mut() = true;
                return;
            }
            Entry::Vacant(slot) => {
                slot.insert(false);
            }
        }
        // Frees the key if the task panics or is cancelled, including before
        // its first poll, so a failed refresh can't block the key forever.
        let mut release = Release {
            owner: Arc::clone(self),
            key: Some(key.clone()),
        };
        let work = create_work();
        tokio::spawn(async move {
            let run = async move {
                let this = Arc::clone(&release.owner);
                let mut complete = true;
                loop {
                    match tokio::time::timeout(this.run_timeout, refresh()).await {
                        Ok(success) => complete &= success,
                        Err(_) => {
                            complete = false;
                            tracing::warn!(
                                timeout_ms = this.run_timeout.as_millis() as u64,
                                "coalesced refresh timed out"
                            );
                        }
                    }
                    tokio::time::sleep(this.min_interval).await;
                    if !this.rerun_or_release(&key) {
                        break;
                    }
                }
                // Finished normally: rerun_or_release already freed the key.
                release.key.take();
                complete
            };
            if let Some(work) = work {
                if work.scope(run).await {
                    work.complete();
                }
            } else {
                run.await;
            }
        });
    }

    /// Clear the rerun flag and report it, or free the key when no request
    /// arrived. Atomic with [`Self::request`]'s entry check, so a request
    /// either sets the flag before this reads it or finds the key free.
    fn rerun_or_release(&self, key: &K) -> bool {
        match self.pending.entry(key.clone()) {
            Entry::Occupied(mut entry) if *entry.get() => {
                *entry.get_mut() = false;
                true
            }
            Entry::Occupied(entry) => {
                entry.remove();
                false
            }
            Entry::Vacant(_) => false,
        }
    }
}

struct Release<K: Eq + Hash> {
    owner: Arc<CoalescedRefresh<K>>,
    key: Option<K>,
}

impl<K: Eq + Hash> Drop for Release<K> {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            self.owner.pending.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const INTERVAL: Duration = Duration::from_secs(1);
    const TIMEOUT: Duration = Duration::from_secs(5);

    fn counting(runs: &Arc<AtomicUsize>) -> impl Fn() -> std::future::Ready<()> + Send + 'static {
        let runs = Arc::clone(runs);
        move || {
            runs.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        }
    }

    /// Let spawned tasks run up to their next timer.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn accepted_refresh_owns_every_trailing_run_with_one_guard() {
        use crate::deploy_drain::{DrainController, WorkKind};
        let drain = DrainController::new();
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        let runs = Arc::new(AtomicUsize::new(0));
        let factories = AtomicUsize::new(0);
        parent.with_context(|| {
            for _ in 0..100 {
                let runs = Arc::clone(&runs);
                refresh.request_tracked(
                    "root",
                    || {
                        factories.fetch_add(1, Ordering::SeqCst);
                        Some(drain.admit_current_child(WorkKind::Background).unwrap())
                    },
                    move || {
                        runs.fetch_add(1, Ordering::SeqCst);
                        std::future::ready(true)
                    },
                );
            }
        });
        assert_eq!(factories.load(Ordering::SeqCst), 1);
        let op = uuid::Uuid::new_v4();
        let boot = drain.begin(op).unwrap().boot_id;
        drain.fence_http(op, boot, "test-edge-retired").unwrap();
        parent.complete();
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(drain.snapshot().background_inflight, 1);
        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        assert_eq!(drain.snapshot().background_inflight, 1);
        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert!(drain.snapshot().safe_to_stop);
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_timeout_releases_key_but_cannot_attest_successful_drain() {
        use crate::deploy_drain::{DrainController, WorkKind};
        let drain = DrainController::new();
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        parent.with_context(|| {
            refresh.request_tracked(
                "root",
                || Some(drain.admit_current_child(WorkKind::Background).unwrap()),
                std::future::pending::<bool>,
            )
        });
        let op = uuid::Uuid::new_v4();
        let boot = drain.begin(op).unwrap().boot_id;
        drain.fence_http(op, boot, "test-edge-retired").unwrap();
        parent.complete();
        settle().await;
        tokio::time::advance(TIMEOUT).await;
        settle().await;
        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert!(refresh.pending.is_empty());
        assert_eq!(drain.snapshot().background_inflight, 0);
        assert!(drain
            .snapshot()
            .blocking_workers
            .iter()
            .any(|name| name == "unresolved-background"));
        assert!(!drain.snapshot().safe_to_stop);
    }

    #[tokio::test(start_paused = true)]
    async fn burst_runs_once_now_and_once_trailing_then_frees_the_key() {
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        let runs = Arc::new(AtomicUsize::new(0));

        for _ in 0..100 {
            refresh.request("root", counting(&runs));
        }
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 1, "first request runs at once");

        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "the burst gets one trailing run"
        );

        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 2, "no request, no rerun");
        assert!(refresh.pending.is_empty(), "idle key is released");

        refresh.request("root", counting(&runs));
        settle().await;
        assert_eq!(
            runs.load(Ordering::SeqCst),
            3,
            "a later request runs at once"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn request_during_a_run_gets_a_trailing_run_that_sees_its_state() {
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        let state = Arc::new(AtomicUsize::new(1));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        // Records the state it reads, then holds until the gate opens.
        let reader = {
            let (state, seen, gate) = (Arc::clone(&state), Arc::clone(&seen), Arc::clone(&gate));
            move || {
                let (state, seen, gate) =
                    (Arc::clone(&state), Arc::clone(&seen), Arc::clone(&gate));
                async move {
                    seen.lock().unwrap().push(state.load(Ordering::SeqCst));
                    let _permit = gate.acquire().await;
                }
            }
        };

        refresh.request("root", reader.clone());
        settle().await;
        assert_eq!(*seen.lock().unwrap(), [1], "the first run read state 1");

        state.store(2, Ordering::SeqCst);
        refresh.request("root", reader.clone());
        settle().await;
        assert_eq!(
            *seen.lock().unwrap(),
            [1],
            "no second run while one is in flight"
        );

        gate.add_permits(1);
        settle().await;
        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(
            *seen.lock().unwrap(),
            [1, 2],
            "the trailing run reads the state written mid-run"
        );

        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(*seen.lock().unwrap(), [1, 2], "no further run");
        assert!(refresh.pending.is_empty(), "idle key is released");
    }

    #[test]
    fn a_task_dropped_before_its_first_poll_releases_the_key() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        let runs = Arc::new(AtomicUsize::new(0));
        // `block_on` returns without yielding, so the spawned task is never
        // polled; dropping the runtime drops it unpolled.
        runtime.block_on(async { refresh.request("root", counting(&runs)) });
        assert!(!refresh.pending.is_empty(), "the queued task holds the key");

        drop(runtime);
        assert_eq!(runs.load(Ordering::SeqCst), 0, "the task never ran");
        assert!(
            refresh.pending.is_empty(),
            "an unpolled task must not wedge the key"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn keys_refresh_independently() {
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        let runs = Arc::new(AtomicUsize::new(0));

        refresh.request("a", counting(&runs));
        refresh.request("b", counting(&runs));
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn panicking_refresh_releases_the_key() {
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        refresh.request("root", || async { panic!("refresh failed") });
        settle().await;
        assert!(refresh.pending.is_empty(), "panic must not wedge the key");

        let runs = Arc::new(AtomicUsize::new(0));
        refresh.request("root", counting(&runs));
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn hung_refresh_times_out_and_releases_the_key() {
        let refresh = Arc::new(CoalescedRefresh::new(INTERVAL, TIMEOUT));
        refresh.request("root", std::future::pending::<()>);
        settle().await;
        assert!(!refresh.pending.is_empty(), "the hung run holds the key");

        // Step one timer at a time: the cooldown sleep is armed only after
        // the timeout fires, so a single jump would overshoot it.
        tokio::time::advance(TIMEOUT).await;
        settle().await;
        tokio::time::advance(INTERVAL).await;
        settle().await;
        assert!(
            refresh.pending.is_empty(),
            "a hung run must not wedge the key"
        );

        let runs = Arc::new(AtomicUsize::new(0));
        refresh.request("root", counting(&runs));
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }
}
