//! Route explicit permission votes to the unchanged prompt and reply route.
use super::*;

impl AgentPool {
    /// A stale, cross-route or retired request never obtains a permission vote.
    pub(crate) fn permission_decision(
        &self,
        scope: &SessionScope,
        tags: &ThreadTags,
        decision: crate::acp::kimi_permission::PermissionDecision,
    ) -> bool {
        let Some(task) = self
            .task_map
            .values()
            .find(|task| task.scope.as_ref() == Some(scope))
        else {
            return false;
        };
        if !task
            .control_dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.current())
        {
            return false;
        }
        let Some(batch) = &task.recoverable_batch else {
            return false;
        };
        if batch
            .work
            .as_ref()
            .is_some_and(crate::queue::LogicalWork::cancelled)
            || batch.reply_tags().root_event_id != tags.root_event_id
        {
            return false;
        }
        task.permission_tx
            .as_ref()
            .is_some_and(|sender| sender.try_send(decision).is_ok())
    }
}
