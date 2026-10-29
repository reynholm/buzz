//! Logical Kimi work outlives individual ACP requests and worker generations.
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
#[path = "recovery_observer.rs"]
mod recovery_observer;
#[path = "recovery_state.rs"]
mod recovery_state;
use recovery_state::WorkRuntime;

fn lock_runtime<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Retain no task data or runtime ownership after the final owner is dropped.
pub(super) struct WorkRevocation {
    revoked: std::sync::Weak<AtomicBool>,
    runtime: std::sync::Weak<WorkRuntime>,
}

impl WorkRevocation {
    fn alive(&self) -> bool {
        self.runtime.strong_count() != 0
    }

    fn cancel(&self) -> bool {
        let (Some(revoked), Some(runtime)) = (self.revoked.upgrade(), self.runtime.upgrade())
        else {
            return false;
        };
        revoked.store(true, Ordering::Release);
        runtime.cancelled.notify_waiters();
        true
    }
}

/// Bounded Kimi retry profile. Existing adapters do not opt into this profile.
#[derive(Clone, Debug)]
pub struct RecoveryPolicy {
    pub max_retries: u32,
    pub delays: Vec<Duration>,
    pub total_budget: Duration,
    pub approval_progress_timeout: Duration,
    pub approval_decision_timeout: Duration,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 4,
            delays: [5, 15, 45, 120].map(Duration::from_secs).to_vec(),
            total_budget: Duration::from_secs(1800),
            approval_progress_timeout: Duration::from_secs(120),
            approval_decision_timeout: Duration::from_secs(120),
        }
    }
}

impl RecoveryPolicy {
    /// Validate before any duration is added to a clock or used for scheduling.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_retries > 16
            || self.delays.is_empty()
            || self.delays.len() > 16
            || self
                .delays
                .iter()
                .any(|delay| delay.is_zero() || *delay > Duration::from_secs(600))
            || self.total_budget.is_zero()
            || self.total_budget > Duration::from_secs(86400)
            || self.approval_progress_timeout.is_zero()
            || self.approval_progress_timeout > self.total_budget
            || self.approval_decision_timeout.is_zero()
            || self.approval_decision_timeout > Duration::from_secs(86400)
        {
            return Err("Kimi recovery requires 0..16 retries, 1..16 nonzero delays <=600s, a 1..86400s budget, and a nonzero progress timeout within that budget");
        }
        Ok(())
    }

    /// Delay for a retry after the initial attempt, using an injected unit draw.
    /// The inclusive jitter range is 0.8..1.2; retry zero is the initial attempt.
    pub fn delay(&self, retry: u32, unit: f64) -> Option<Duration> {
        if retry == 0 || retry > self.max_retries {
            return None;
        }
        let base = self
            .delays
            .get((retry - 1) as usize)
            .or_else(|| self.delays.last())?;
        let unit = if unit.is_finite() {
            unit.clamp(0., 1.)
        } else {
            0.5
        };
        Some(Duration::from_secs_f64(
            base.as_secs_f64() * (0.8 + 0.4 * unit),
        ))
    }
}

/// One logical request, including publication identity and explicit revocation.
#[derive(Clone, Debug)]
pub struct LogicalWork {
    pub delivery_key: Uuid,
    pub scope: SessionScope,
    pub original_events: Arc<Vec<BatchEvent>>,
    pub thread_tags: ThreadTags,
    pub journal_dir: PathBuf,
    pub started_at: Instant,
    pub deadline: Instant,
    pub attempt: u32,
    pub next_attempt: Option<Instant>,
    pub recovery_reason: Option<&'static str>,
    pub policy: RecoveryPolicy,
    pub is_dm: Arc<OnceLock<bool>>,
    pub ordinary_preempted: bool,
    revoked: Arc<AtomicBool>,
    runtime: Arc<WorkRuntime>,
}

impl LogicalWork {
    /// Explicit scope controls share this token with all late batch clones.
    pub fn cancelled(&self) -> bool {
        self.revoked.load(Ordering::Acquire) || self.runtime.finished.load(Ordering::Acquire)
    }

    /// Execution consumes the same wall-clock budget as backoff.
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub(super) fn cancel(&self) {
        self.revoked.store(true, Ordering::Release);
        self.runtime.cancelled.notify_waiters();
    }

    /// Resolve the actual managed route once channel metadata is known.
    pub fn reply_tags(&self) -> ThreadTags {
        if self.is_dm.get() == Some(&true)
            && self
                .original_events
                .last()
                .is_some_and(|event| event.routing_thread_tags().root_event_id.is_none())
        {
            return ThreadTags::default();
        }
        self.thread_tags.clone()
    }

    /// The identical key, route and journal accompany every attempt.
    pub fn managed_instruction(&self) -> String {
        let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
        let tags = self.reply_tags();
        let parent = tags
            .root_event_id
            .as_ref()
            .or(tags.parent_event_id.as_ref());
        let reply = parent
            .map(|id| format!(" --reply-to {}", quote(id)))
            .unwrap_or_default();
        let command = format!(
            "buzz messages deliver --delivery-key {} --channel {}{} --journal-dir {} --content -",
            quote(&self.delivery_key.to_string()),
            quote(&self.scope.channel_id().to_string()),
            reply,
            quote(&self.journal_dir.to_string_lossy())
        );
        format!("<managed-work>\nLogical work: {}. Attempt: {}. Final report command (pipe the final text through stdin):\n{}\nPreserve this exact key, route and journal on every retry and background collection. Use messages send only for separate progress, and messages edit for corrections. Publish the final report after all background work and result collection finish. If the report was already confirmed, the same command returns it; do not send a second completion or failure report. This protects report publication only. Before repeating any external action whose result is uncertain, verify its actual outcome. If it cannot be established safely, ask the human and stop; do not repeat the action or claim recovery succeeded.\n</managed-work>",self.delivery_key,self.attempt,command)
    }
}

/// A fresh OS-random UUID provides 48 unfixed random bits, independent of clocks
/// and other harness processes. Draws can coincide; uniqueness is not promised.
fn jitter_unit() -> f64 {
    let uuid = Uuid::new_v4();
    let mut bytes = [0_u8; 8];
    bytes[..6].copy_from_slice(&uuid.as_bytes()[..6]);
    u64::from_le_bytes(bytes) as f64 / ((1_u64 << 48) - 1) as f64
}

impl EventQueue {
    /// Terminal work can be reported without a free provider process.
    pub(crate) fn next_work_terminal_deadline(&self) -> Option<Instant> {
        self.recovery_batches
            .iter()
            .filter(|(scope, _)| !self.in_flight_scopes.contains(*scope))
            .filter_map(|(_, batch)| batch.work.as_ref())
            .chain(
                self.continuations
                    .iter()
                    .filter(|(scope, _)| !self.in_flight_scopes.contains(*scope))
                    .filter_map(|(_, continuation)| continuation.work.as_ref()),
            )
            .map(|work| {
                if work.retirement_unverified() {
                    Instant::now()
                } else {
                    work.deadline
                }
            })
            .min()
    }

    /// Opt into stable Kimi work. Callers validate configured policy at startup.
    pub fn with_kimi_recovery(mut self, policy: RecoveryPolicy, cwd: &str) -> Self {
        self.recovery_policy = Some((policy, PathBuf::from(cwd).join(".buzz-deliveries")));
        self
    }

    pub(super) fn attach_logical_work(&mut self, mut batch: FlushBatch) -> FlushBatch {
        if batch.work.is_none() {
            if let Some((policy, journal_dir)) = &self.recovery_policy {
                let now = Instant::now();
                let work = LogicalWork {
                    delivery_key: batch
                        .continuation
                        .as_ref()
                        .map(|c| c.delivery_key)
                        .unwrap_or_else(Uuid::new_v4),
                    scope: batch.scope.clone(),
                    original_events: Arc::new(batch.events.clone()),
                    thread_tags: batch.reply_tags(),
                    journal_dir: journal_dir.clone(),
                    started_at: now,
                    deadline: now + policy.total_budget,
                    attempt: 0,
                    next_attempt: None,
                    recovery_reason: None,
                    policy: policy.clone(),
                    revoked: Arc::new(AtomicBool::new(false)),
                    is_dm: Arc::new(OnceLock::new()),
                    ordinary_preempted: false,
                    runtime: Arc::new(WorkRuntime::default()),
                };
                batch.work = Some(work);
            }
        }
        if let Some(work) = &batch.work {
            self.revocable_work.retain(|_, (_, handle)| handle.alive());
            self.revocable_work
                .entry(work.delivery_key)
                .or_insert_with(|| {
                    (
                        work.scope.clone(),
                        WorkRevocation {
                            revoked: Arc::downgrade(&work.revoked),
                            runtime: Arc::downgrade(&work.runtime),
                        },
                    )
                });
            if let Some(continuation) = &mut batch.continuation {
                continuation.delivery_key = work.delivery_key;
                continuation.work = Some(work.clone());
            }
            self.in_flight_work
                .insert(batch.scope.clone(), work.clone());
        }
        batch
    }

    pub(super) fn flush_logical_retry(&mut self) -> Option<FlushBatch> {
        let now = Instant::now();
        let scope = self
            .recovery_batches
            .iter()
            .filter(|(scope, batch)| {
                !self.in_flight_scopes.contains(*scope)
                    && batch.work.as_ref().is_some_and(|work| {
                        work.remaining().is_zero()
                            || work.retirement_unverified()
                            || (!work.awaiting_retirement()
                                && work.next_attempt.is_none_or(|next| next <= now)
                                && batch
                                    .continuation
                                    .as_ref()
                                    .is_none_or(|continuation| continuation.background_ready))
                    })
            })
            .min_by_key(|(_, batch)| batch.work.as_ref().map(|work| work.started_at))
            .map(|(scope, _)| scope.clone())?;
        let batch = self.recovery_batches.remove(&scope)?;
        self.in_flight_scopes.insert(scope.clone());
        self.in_flight_deadlines
            .insert(scope.clone(), now + self.in_flight_deadline);
        self.in_flight_batch_sizes
            .insert(scope.clone(), batch.events.len());
        if let Some(continuation) = &batch.continuation {
            self.in_flight_continuations.insert(scope.clone());
            self.record_continuation_reply_route(&scope, continuation);
        } else {
            self.record_in_flight_reply_route(&scope, &batch.events);
        }
        Some(self.attach_logical_work(batch))
    }

    pub(super) fn requeue_logical_work(&mut self, mut batch: FlushBatch) -> Option<FlushBatch> {
        let work = batch.work.as_mut()?;
        if work.cancelled() {
            return None;
        }
        work.attempt = work.attempt.saturating_add(1);
        let now = Instant::now();
        let Some(delay) = work.policy.delay(work.attempt, jitter_unit()) else {
            return Some(batch);
        };
        if now >= work.deadline || delay >= work.deadline.saturating_duration_since(now) {
            return Some(batch);
        }
        work.next_attempt = Some(now + delay);
        work.transition(
            "retrying",
            work.recovery_reason.unwrap_or("execution_failure"),
        );
        tracing::info!(work=%work.delivery_key,scope=%work.scope.telemetry_label(),
            attempt=work.attempt,max_retries=work.policy.max_retries,delay_secs=delay.as_secs_f64(),
            remaining_secs=work.remaining().as_secs_f64(),reason=work.recovery_reason,
            "logical work retry scheduled");
        self.recovery_batches.insert(batch.scope.clone(), batch);
        None
    }

    /// Cancel pending and in-flight logical work; late clones remain revoked.
    /// New messages admitted afterward get their own independent work token.
    pub(crate) fn cancel_logical_work(&mut self, scope: &SessionScope) -> bool {
        let mut cancelled = self.revoke_work_publication(|candidate| candidate == scope);
        if let Some(work) = self
            .continuations
            .get(scope)
            .and_then(|continuation| continuation.work.as_ref())
        {
            work.cancel();
            cancelled = true;
        }
        if let Some(work) = self.in_flight_work.get(scope) {
            work.cancel();
            cancelled = true;
        }
        if let Some(batch) = self.recovery_batches.remove(scope) {
            if let Some(work) = batch.work {
                work.cancel();
                cancelled = true;
            }
        }
        cancelled
    }

    pub(super) fn revoke_work_publication(
        &mut self,
        matches: impl Fn(&SessionScope) -> bool,
    ) -> bool {
        let mut cancelled = false;
        self.revocable_work.retain(|_, (scope, handle)| {
            if matches(scope) {
                cancelled |= handle.cancel();
                false
            } else {
                handle.alive()
            }
        });
        cancelled
    }

    /// Fresh traffic with another route remains queued behind original work.
    pub(crate) fn has_logical_work_in_flight(&self, scope: &SessionScope) -> bool {
        self.in_flight_work.contains_key(scope)
    }

    pub(super) fn merge_logical_control(&mut self, mut batch: FlushBatch, reason: CancelReason) {
        let Some(work) = batch.work.as_mut() else {
            return;
        };
        if work.cancelled() {
            return;
        }
        work.ordinary_preempted = true;
        let route = work.original_events.last().map(BatchEvent::reply_route);
        let is_dm = work.is_dm.get().copied().unwrap_or(false);
        let available = MAX_PENDING_PER_SCOPE
            .saturating_sub(batch.events.len() + batch.cancelled_events.len())
            .min(MAX_BATCH_EVENTS);
        let mut incoming = Vec::new();
        if let (Some(route), Some(queued)) = (route, self.queues.get_mut(&batch.scope)) {
            let mut retained = VecDeque::new();
            for event in queued.drain(..) {
                let candidate = event.clone().into_batch_event();
                if incoming.len() < available
                    && route.accepts_steer(&candidate.reply_route(), is_dm)
                {
                    incoming.push(candidate);
                } else {
                    retained.push_back(event);
                }
            }
            *queued = std::mem::take(&mut retained);
        }
        if !incoming.is_empty() {
            batch.cancelled_events.append(&mut batch.events);
            batch.events = incoming;
            batch.cancel_reason = Some(reason);
        }
        self.recovery_batches
            .entry(batch.scope.clone())
            .or_insert(batch);
    }
}
