//! Protect background children and queue collection in their original session.

use super::*;
use crate::queue::{EventQueue, RuntimeContinuation};

pub(super) const FOLLOWUP_PROMPT: &str = "The background child process you left running has exited. Collect its result and continue the work in the same Buzz thread. Post the update there. Do not start a second copy of the task.";

/// Shell arguments stay literal even when workspace names contain apostrophes.
fn shell_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn followup_prompt(
    channel: Uuid,
    continuation: &RuntimeContinuation,
    cwd: &str,
) -> String {
    let journal = std::path::Path::new(cwd).join(".buzz-deliveries");
    let parent = continuation
        .thread_tags
        .parent_event_id
        .as_deref()
        .or(continuation.thread_tags.root_event_id.as_deref());
    let command = parent.map(|parent| format!(
        "buzz messages deliver --delivery-key {} --channel {} --reply-to {} --journal-dir {} --content <FINAL_REPORT>",
        shell_argument(&continuation.delivery_key.to_string()),
        shell_argument(&channel.to_string()),
        shell_argument(parent),
        shell_argument(&journal.to_string_lossy()),
    )).unwrap_or_else(|| "Original reply event is unavailable; do not invent a destination or publish a report elsewhere.".into());
    format!("{FOLLOWUP_PROMPT}\nChannel: {channel}\nReply thread: {}\nPublish the final report using this managed command (replace <FINAL_REPORT> with properly quoted text or use --content - and stdin):\n{command}\nKeep this delivery key, original route and journal directory for every retry, including after an application error. An unconfirmed managed delivery can be safely retried with the SAME key; never replace it with a fresh key or bare messages send. Replays return the first report even if the summary changes. Preserve callback notifications with --mention or ordinary @mentions. Use messages send for separate progress and messages edit for an explicit correction. Managed delivery does not cover other publishing tools.", continuation.thread_tags.root_event_id.as_deref().unwrap_or("none"))
}

/// A worker with live or pending background work; an invalidated origin never resumes.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeHold {
    pub scope: SessionScope,
    pub blocking_pids: HashSet<u32>,
    pub continuation: Option<RuntimeContinuation>,
    /// Child exit has been queued; keep its session protected until collection ends.
    pub collection_pending: bool,
}

/// Whether a queued completion can still use its original provider session.
pub(crate) enum ContinuationState {
    Ready,
    Busy,
    Stale,
}

impl OwnedAgent {
    pub(super) fn remember_background_context(
        &mut self,
        source: &PromptSource,
        batch: Option<&FlushBatch>,
        session_id: &str,
    ) {
        if self.has_background_collection() {
            return;
        }
        self.background_context = source.scope().map(|scope| RuntimeHold {
            scope: scope.clone(),
            blocking_pids: HashSet::new(),
            collection_pending: false,
            continuation: Some(RuntimeContinuation {
                delivery_key: Uuid::new_v4(),
                ordinary_preempted: false,
                worker_index: self.index,
                session_id: session_id.to_owned(),
                generation: self
                    .state
                    .scope_owner_generations
                    .get(scope)
                    .copied()
                    .unwrap_or_default(),
                thread_tags: batch.map(FlushBatch::reply_tags).unwrap_or_default(),
                received_at: std::time::Instant::now(),
            }),
        });
    }

    pub(crate) fn has_background_collection(&self) -> bool {
        self.background_context.as_ref().is_some_and(|hold| {
            hold.continuation.is_some()
                && (hold.collection_pending || !hold.blocking_pids.is_empty())
        })
    }

    /// Stop retaining an original collection after success or terminal failure.
    pub(crate) fn finish_background_collection(&mut self) {
        if let Some(hold) = &mut self.background_context {
            hold.continuation = None;
            hold.collection_pending = false;
            // An empty completed hold must not turn into another collection.
            hold.blocking_pids.clear();
        }
    }

    pub(super) fn background_work_running(&mut self, source: &PromptSource) -> bool {
        if source.scope().is_none() {
            return false;
        }
        let children = self.acp.background_child_pids();
        if let (Some(hold), Some(children)) = (&mut self.background_context, &children) {
            // Retain evidence of observed work that just exited until the pool
            // queues it; an ordinary turn must not rotate in this gap.
            if !children.is_empty() {
                hold.blocking_pids = children.clone();
            }
        }
        self.has_background_collection() || children.is_none_or(|children| !children.is_empty())
    }
}

impl AgentPool {
    pub fn set_runtime_hold(&mut self, index: usize, hold: RuntimeHold) {
        self.runtime_holds.insert(index, hold);
    }

    pub fn clear_runtime_hold(&mut self, index: usize) {
        self.runtime_holds.remove(&index);
    }

    pub fn has_runtime_hold(&self) -> bool {
        !self.runtime_holds.is_empty()
    }

    pub(crate) fn has_background_collection_for_scope(&self, scope: &SessionScope) -> bool {
        self.runtime_holds
            .values()
            .any(|hold| hold.scope == *scope && hold.continuation.is_some())
    }

    pub(super) fn runtime_claim_allowed(&self, index: usize, scope: Option<&SessionScope>) -> bool {
        !self.retire_on_return.contains(&index)
            && self
                .runtime_holds
                .get(&index)
                .is_none_or(|hold| scope == Some(&hold.scope))
    }

    fn continuation_matches(
        &self,
        agent: &OwnedAgent,
        scope: &SessionScope,
        continuation: &RuntimeContinuation,
    ) -> bool {
        agent.index == continuation.worker_index
            && self.agent_owns_scope(agent, scope)
            && agent.state.sessions.get(scope) == Some(&continuation.session_id)
            && agent.state.scope_owner_generations.get(scope) == Some(&continuation.generation)
    }

    pub fn continuation_state(
        &self,
        scope: &SessionScope,
        continuation: &RuntimeContinuation,
    ) -> ContinuationState {
        match self.agents.get(continuation.worker_index) {
            Some(Some(agent)) if self.continuation_matches(agent, scope, continuation) => {
                if self.runtime_claim_allowed(agent.index, Some(scope)) {
                    ContinuationState::Ready
                } else {
                    ContinuationState::Busy
                }
            }
            Some(None)
                if self
                    .task_map
                    .values()
                    .any(|task| task.agent_index == continuation.worker_index) =>
            {
                ContinuationState::Busy
            }
            _ => ContinuationState::Stale,
        }
    }

    pub(super) fn refresh_runtime_hold(&mut self, agent: &mut OwnedAgent) {
        self.runtime_holds.remove(&agent.index);
        let Some(mut hold) = agent.background_context.take() else {
            return;
        };
        match agent.acp.background_child_pids() {
            // Work seen at prompt completion may exit while publishing metrics.
            // Keep that completion pending so the poll still queues collection.
            Some(children)
                if children.is_empty()
                    && hold.blocking_pids.is_empty()
                    && !hold.collection_pending =>
            {
                return
            }
            Some(children) if !children.is_empty() => hold.blocking_pids = children,
            Some(_) => {} // Preserve observed exit until polling queues collection.
            None => {}    // Unknown state must not authorize killing live work.
        }
        if hold.continuation.as_ref().is_some_and(|continuation| {
            !self.continuation_matches(agent, &hold.scope, continuation)
        }) {
            hold.continuation = None;
            hold.collection_pending = false;
            if hold.blocking_pids.is_empty() {
                return;
            }
        }
        self.set_runtime_hold(agent.index, hold);
    }

    /// Poll independently of lazy-pool sleep. Only a valid original session resumes.
    pub fn queue_finished_background_work(&mut self, queue: &mut EventQueue) {
        let indexes: Vec<_> = self.runtime_holds.keys().copied().collect();
        for index in indexes {
            let Some(Some(agent)) = self.agents.get_mut(index) else {
                continue;
            };
            let Some(children) = agent.acp.background_child_pids() else {
                continue;
            };
            if !children.is_empty() {
                if let Some(hold) = self.runtime_holds.get_mut(&index) {
                    hold.blocking_pids = children;
                }
                continue;
            }
            if self.retire_on_return.contains(&index) {
                self.runtime_holds.remove(&index);
                continue;
            }
            let Some(hold) = self.runtime_holds.get(&index) else {
                continue;
            };
            if hold.blocking_pids.is_empty() && !hold.collection_pending {
                // Unknown discovery held this worker conservatively, but no
                // workload was observed. Empty now is not a completion event.
                self.runtime_holds.remove(&index);
                continue;
            }
            let valid = hold.continuation.as_ref().is_some_and(|continuation| {
                self.agents[index].as_ref().is_some_and(|agent| {
                    self.continuation_matches(agent, &hold.scope, continuation)
                })
            });
            if !valid {
                self.runtime_holds.remove(&index);
                continue;
            }
            if let Some(hold) = self.runtime_holds.get_mut(&index) {
                hold.blocking_pids.clear();
                if !hold.collection_pending {
                    if let Some(continuation) = &hold.continuation {
                        queue.enqueue_continuation(hold.scope.clone(), continuation.clone());
                        hold.collection_pending = true;
                    }
                }
            }
        }
    }
}
