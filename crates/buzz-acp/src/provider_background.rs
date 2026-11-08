//! Host-owned identities and durable collection receipts for negotiated ACP work.
use crate::{
    queue::{BatchEvent, ResolvedEdit, ThreadTags},
    scope::SessionScope,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(crate) const VERSION: u64 = 1;
pub(crate) const UPDATE: &str = "_buzz/background_tasks/update";
pub(crate) const SNAPSHOT: &str = "_buzz/background_tasks/snapshot";
const MAX_TASKS: usize = 128;
const MAX_BYTES: usize = 192 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Identity {
    pub session_id: String,
    pub work_id: Uuid,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Running,
    CompletedPendingCollection,
    FailedPendingCollection,
    Collected,
    Cancelled,
}
impl State {
    fn pending(self) -> bool {
        matches!(
            self,
            Self::CompletedPendingCollection | Self::FailedPendingCollection
        )
    }
    fn terminal(self) -> bool {
        matches!(self, Self::Collected | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Task {
    pub task_id: String,
    pub state: State,
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedEvent {
    pub event: nostr::Event,
    pub edit: Option<ResolvedEdit>,
}
impl From<&BatchEvent> for SavedEvent {
    fn from(event: &BatchEvent) -> Self {
        Self {
            event: event.event.clone(),
            edit: event.edit.clone(),
        }
    }
}

/// Durable cancellation intent and its independently confirmed provider receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cancellation {
    pub identity: Identity,
    pub cancel_id: Uuid,
    pub cause: String,
    pub acknowledged: bool,
}
impl Cancellation {
    pub fn valid(&self) -> bool {
        !self.cancel_id.is_nil()
            && matches!(
                self.cause.as_str(),
                "user_cancel" | "steering_replacement" | "host_shutdown" | "provider_failure"
            )
    }
}

/// Persists with the provider session's existing lease and namespace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    /// Child receipts do not complete the parent or its final delivery.
    #[serde(default)]
    pub parent_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<Cancellation>,
    pub identity: Identity,
    pub scope: SessionScope,
    pub thread_tags: ThreadTags,
    pub events: Vec<SavedEvent>,
    pub runtime_final: bool,
    pub deadline_unix: u64,
    pub tasks: BTreeMap<String, Task>,
    /// A continuation start is distinct from the provider's durable acknowledgement.
    pub started: BTreeMap<String, u64>,
}

impl Record {
    pub fn recoverable(&self) -> bool {
        self.cancellation.is_none()
            && self.terminal_reason.is_none()
            && (self.parent_pending || self.outstanding())
    }
    pub fn outstanding(&self) -> bool {
        self.cancellation.is_none()
            && self.terminal_reason.is_none()
            && self.tasks.values().any(|task| !task.state.terminal())
    }
    pub fn pending(&self) -> Vec<Task> {
        if self.cancellation.is_some() {
            return Vec::new();
        }
        self.tasks
            .values()
            .filter(|task| task.state.pending())
            .cloned()
            .collect()
    }
    pub fn unacknowledged(&self) -> bool {
        if self.cancellation.is_some() {
            return false;
        }
        self.started.iter().any(|(id, sequence)| {
            self.tasks
                .get(id)
                .is_some_and(|task| task.sequence >= *sequence && task.state.pending())
        })
    }
    pub fn start_collection(&mut self) {
        for task in self.pending() {
            self.started.insert(task.task_id, task.sequence);
        }
    }
    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }
    pub fn remaining(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.deadline_unix)
            .saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default(),
            )
            .min(std::time::Duration::from_secs(86400))
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.identity.session_id.is_empty()
            || self.identity.session_id.len() > 512
            || self.identity.work_id.is_nil()
            || self
                .cancellation
                .as_ref()
                .is_some_and(|cancel| !cancel.valid() || cancel.identity != self.identity)
            || self.tasks.len() > MAX_TASKS
            || self.started.len() > MAX_TASKS
            || self.events.len() > 50
            || self
                .tasks
                .iter()
                .any(|(key, task)| key != &task.task_id || !valid_task(task))
            || self
                .events
                .iter()
                .any(|event| event.event.verify().is_err())
            || serde_json::to_vec(self)
                .map_err(|_| "background_record_encoding")?
                .len()
                > MAX_BYTES
        {
            return Err("background_record_invalid_or_oversized");
        }
        Ok(())
    }
    fn identity_matches(&self, value: &Value) -> Result<bool, &'static str> {
        let identity: Identity =
            serde_json::from_value(value.clone()).map_err(|_| "background_identity_invalid")?;
        Ok(identity == self.identity)
    }
    pub fn apply(&mut self, value: &Value) -> Result<bool, &'static str> {
        if self.cancellation.is_some()
            || self.terminal_reason.is_some()
            || !self.identity_matches(value)?
        {
            return Ok(false);
        }
        let task: Task =
            serde_json::from_value(value.clone()).map_err(|_| "background_task_invalid")?;
        self.apply_task(task, false)
    }
    fn apply_task(&mut self, task: Task, snapshot: bool) -> Result<bool, &'static str> {
        if !valid_task(&task) {
            return Err("background_task_invalid");
        }
        if let Some(previous) = self.tasks.get(&task.task_id) {
            if task.sequence < previous.sequence {
                return Ok(false);
            }
            if task.sequence == previous.sequence {
                return if *previous == task {
                    Ok(false)
                } else {
                    Err("background_sequence_conflict")
                };
            }
            if previous.state.terminal()
                || (previous.state.pending() && task.state == State::Running)
            {
                return Err("background_task_reused");
            }
            let missed_completion = snapshot
                && previous.state == State::Running
                && task
                    .completion_sequence
                    .is_some_and(|sequence| sequence > previous.sequence);
            if task.state == State::Collected
                && !missed_completion
                && (!previous.state.pending()
                    || task.completion_sequence != Some(previous.sequence))
            {
                return Err("background_acknowledgement_mismatch");
            }
        } else {
            if self.tasks.len() >= MAX_TASKS {
                return Err("background_task_capacity");
            }
            if task.state == State::Collected && !snapshot {
                return Err("background_unknown_acknowledgement");
            }
        }
        self.parent_pending = true;
        self.tasks.insert(task.task_id.clone(), task);
        Ok(true)
    }
    /// Omitted entries never authorize forgetting an uncollected result.
    pub fn reconcile(&mut self, value: &Value) -> Result<(), &'static str> {
        if self.cancellation.is_some() || self.terminal_reason.is_some() {
            return Err("background_work_terminal");
        }
        if !self.identity_matches(value)? {
            return Err("background_snapshot_identity_mismatch");
        }
        let tasks = value
            .get("tasks")
            .and_then(Value::as_array)
            .filter(|tasks| tasks.len() <= MAX_TASKS)
            .ok_or("background_snapshot_invalid")?;
        let mut next = self.clone();
        let mut seen = std::collections::HashSet::new();
        for task in tasks {
            let task: Task =
                serde_json::from_value(task.clone()).map_err(|_| "background_snapshot_invalid")?;
            if !seen.insert(task.task_id.clone()) {
                return Err("background_snapshot_duplicate_task");
            }
            next.apply_task(task, true)?;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}

fn valid_task(task: &Task) -> bool {
    !task.task_id.is_empty()
        && task.task_id.len() <= 256
        && task.sequence > 0
        && (task.state != State::Collected
            || task
                .completion_sequence
                .is_some_and(|sequence| sequence > 0 && sequence < task.sequence))
}

pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
#[path = "provider_background_tests.rs"]
mod tests;
