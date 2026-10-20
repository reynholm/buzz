//! All-copy targeting and the single deliberate-retirement return boundary.
use super::*;

impl AgentPool {
    /// Every worker that may hold this scope, including checked-out and held work.
    pub(crate) fn retirement_targets(&self, scope: &SessionScope) -> Vec<usize> {
        let mut targets = self.scope_workers.get(scope).cloned().unwrap_or_default();
        targets.extend(
            self.runtime_holds
                .iter()
                .filter(|(_, hold)| &hold.scope == scope)
                .map(|(index, _)| *index),
        );
        targets.extend(
            self.task_map
                .values()
                .filter(|task| task.scope.as_ref() == Some(scope))
                .map(|task| task.agent_index),
        );
        targets.extend(
            self.agents
                .iter()
                .flatten()
                .filter(|agent| agent.state.sessions.contains_key(scope))
                .map(|agent| agent.index),
        );
        let mut targets: Vec<_> = targets.into_iter().collect();
        targets.sort_unstable();
        targets
    }

    pub(crate) fn retirement_waits_for_foreign(&self, index: usize, scope: &SessionScope) -> bool {
        self.runtime_holds
            .get(&index)
            .is_some_and(|hold| &hold.scope != scope)
            || self
                .task_map
                .values()
                .any(|task| task.agent_index == index && task.scope.as_ref() != Some(scope))
    }

    /// Reserve a slot immediately; foreign foreground work still owns its turn.
    pub(crate) fn mark_retire_on_return(&mut self, index: usize) {
        self.retire_on_return.insert(index);
    }

    /// This boundary precedes outcome-specific return/crash handling.
    pub(crate) fn return_for_retirement(&mut self, mut agent: OwnedAgent) {
        self.refresh_runtime_hold(&mut agent);
        let index = agent.index;
        self.agents[index] = Some(agent);
    }

    pub(crate) fn needs_retirement(&self, agent: &OwnedAgent) -> bool {
        self.retire_on_return.contains(&agent.index)
    }

    /// Foreign background holds precede the idle-slot branch. A deadline may
    /// expire a background hold, but can never take a checked-out foreground turn.
    pub(crate) fn take_retirement_target(
        &mut self,
        index: usize,
        scopes: &HashSet<SessionScope>,
        background_expired: bool,
    ) -> Option<OwnedAgent> {
        if self.task_map.values().any(|task| task.agent_index == index) {
            return None;
        }
        if self
            .runtime_holds
            .get(&index)
            .is_some_and(|hold| !scopes.contains(&hold.scope))
            && !background_expired
        {
            return None;
        }
        let agent = self.agents.get_mut(index)?.take()?;
        self.clear_runtime_hold(index);
        Some(agent)
    }

    /// Forget only the retired process's ownership; durable provider state stays.
    pub(crate) fn finish_worker_retirement(&mut self, index: usize) {
        self.retire_on_return.remove(&index);
        self.runtime_holds.remove(&index);
        self.session_owners
            .retain(|_, owner| owner.agent_index != index);
        self.scope_workers.retain(|_, workers| {
            workers.remove(&index);
            !workers.is_empty()
        });
    }
}

impl OwnedAgent {
    /// Publish neighbor departure only after the old process was verified stopped.
    /// A newer owner is deliberately left untouched by the lease CAS.
    pub(crate) fn prepare_retired_neighbors(
        &mut self,
        scopes: &HashSet<SessionScope>,
        controls: &crate::session_controls::SessionControlRuntime,
    ) -> Result<(), crate::session_store::StoreError> {
        for scope in self
            .state
            .sessions
            .keys()
            .filter(|scope| !scopes.contains(*scope))
        {
            if let Some(store) = &self.state.store {
                if let Some(lease) = self.state.leases.get(scope) {
                    match store.mark_departed(
                        scope,
                        *lease,
                        crate::session_store::Departure::WorkerRecycled,
                    ) {
                        Ok(()) | Err(crate::session_store::StoreError::Conflict) => {}
                        Err(error) => return Err(error),
                    }
                }
            } else {
                controls.recycle_neighbor(scope)?;
            }
        }
        self.state.retire_all_status();
        Ok(())
    }
}
