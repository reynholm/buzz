//! Transition logging for capacity stalls, without one message per timer tick.
use super::*;

impl AgentPool {
    pub(crate) fn report_unavailable(&mut self, batch: &FlushBatch) {
        let now = std::time::Instant::now();
        let previous = self.blocked_scopes.get(&batch.scope).copied();
        if previous.is_some_and(|last| now.duration_since(last) < Duration::from_secs(60)) {
            return;
        }
        self.blocked_scopes.insert(batch.scope.clone(), now);
        let queue_age_secs = batch
            .events
            .first()
            .map(|event| event.received_at)
            .or_else(|| {
                batch
                    .continuation
                    .as_ref()
                    .map(|continuation| continuation.received_at)
            })
            .map(|received| now.saturating_duration_since(received).as_secs())
            .unwrap_or_default();
        let active = self.agents.iter().filter(|slot| slot.is_none()).count();
        let held = self.runtime_holds.len();
        let idle = self
            .agents
            .iter()
            .flatten()
            .filter(|agent| self.runtime_claim_allowed(agent.index, None))
            .count();
        let holds: Vec<_> = self.runtime_holds.iter().map(|(index, hold)| {
            let mut pids: Vec<_> = hold.blocking_pids.iter().copied().collect();
            pids.sort_unstable();
            serde_json::json!({
                "worker": index,
                "scope": hold.scope.telemetry_label(),
                "reason": if hold.collection_pending { "collection" } else if pids.is_empty() { "unknown" } else { "processes" },
                "blocking_count": pids.len(),
                "pids": pids.into_iter().take(16).collect::<Vec<_>>(),
            })
        }).collect();
        tracing::warn!(scope = %batch.scope.telemetry_label(), queue_age_secs, active, held, idle, ?holds,
            repeated = previous.is_some(), "scope waiting for worker capacity");
    }

    pub(crate) fn report_available(&mut self, scope: &SessionScope) {
        if self.blocked_scopes.remove(scope).is_some() {
            tracing::info!(scope = %scope.telemetry_label(), "worker capacity available; dispatch resumed");
        }
    }
}
